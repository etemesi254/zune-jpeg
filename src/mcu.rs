//! Implements routines to decode a MCU

use std::cmp::min;
use std::io::Cursor;
use std::sync::{Arc, Mutex};

use crate::{Decoder};
use crate::bitstream::BitStream;
use crate::errors::DecodeErrors;
use crate::marker::Marker;
use crate::worker::{post_process_interleaved, post_process_non_interleaved};

/// The size of a DC block for a MCU.

pub const DCT_BLOCK: usize = 64;

impl Decoder
{
    /// Decode an MCU with no interleaving aka the components were not down-sampled
    /// hence are arranged in Y,Cb,Cr fashion
    #[allow(clippy::similar_names)]
    #[inline(never)]
    #[rustfmt::skip]
    pub(crate) fn decode_mcu_ycbcr_non_interleaved_baseline(&mut self,
                                                            reader: &mut Cursor<Vec<u8>>,
    ) -> Result<Vec<u8>, DecodeErrors> {
        #[cfg(feature = "threadpool")]
            let pool = threadpool::ThreadPool::default();

        let mcu_width = ((self.info.width + 7) / 8) as usize;
        let mcu_height = ((self.info.height + 7) / 8) as usize;

        // A bitstream instance, which will do bit-wise decoding for us
        let mut stream = BitStream::new();
        // Size of our output image(width*height)
        let capacity = usize::from(self.info.width + 7) * usize::from(self.info.height + 7);

        let component_capacity = mcu_width * DCT_BLOCK;

        // The following contains containers for unprocessed values
        // by unprocessed we mean they haven't been dequantized and inverse DCT carried on them

        // for those pointers storing unprocessed items, zero them out here
        // num_components cannot go above MAX_COMPONENTS(currently all are at a max of 4)
        for (pos, comp) in self.components.iter().enumerate() {
            // multiply capacity with sampling factor, it  should be 1*1 for un-sampled images

            //NOTE: We only allocate a block if we need it, so e.g for grayscale
            // we don't allocate for CB and Cr channels
            if min(self.output_colorspace.num_components()-1, pos) == pos
            {
                self.mcu_block[pos] = vec![0; component_capacity * comp.vertical_sample * comp.horizontal_sample];
            }
        }
        let mut position = 0;

        // Create an Arc of components to prevent cloning on every MCU width
        // we can just send this Arc on clone
        let global_component = Arc::new(self.components.clone());

        // Storage for decoded pixels
        let global_channel = Arc::new(Mutex::new(
            vec![0; capacity * self.output_colorspace.num_components()]));

        // things needed for post processing that we can remove out of the loop
        // since they are copy, it should not be that hard
        let input = self.input_colorspace;
        let output = self.output_colorspace;
        let idct_func = self.idct_func;
        let color_convert = self.color_convert;
        let color_convert_16 = self.color_convert_16;
        let width = usize::from(self.width());

        // check that dc and AC tables exist outside the hot path
        // If none of these routines fail,  we can use unsafe in the inner loop without it being UB
        for i in 0..self.input_colorspace.num_components()
        {
            let _ = &self.dc_huffman_tables
                .get(self.components[i].dc_huff_table).as_ref()
                .ok_or_else(|| {
                    DecodeErrors::HuffmanDecode(format!("No Huffman DC table for component {:?} ",
                                                        self.components[i].component_id
                    ))
                })?.as_ref()
                .ok_or_else(|| {
                    DecodeErrors::HuffmanDecode(format!("No DC table for component {:?}",
                                                        self.components[i].component_id
                    ))
                })?;

            let _ = &self.ac_huffman_tables
                .get(self.components[i].ac_huff_table).as_ref()
                .ok_or_else(|| {
                    DecodeErrors::HuffmanDecode(format!("No Huffman AC table for component {:?} ",
                                                        self.components[i].component_id
                    ))
                })?.as_ref()
                .ok_or_else(|| {
                    DecodeErrors::HuffmanDecode(format!("No AC table for component {:?}",
                                                        self.components[i].component_id
                    ))
                })?;
        }
        for _ in 0..mcu_height
        {
            'label: for i in 0..mcu_width
            {
                // iterate over components, for non interleaved scans
                for pos in 0..self.input_colorspace.num_components()
                {
                    let component = &mut self.components[pos];
                    let dc_table = unsafe {
                        self.dc_huffman_tables
                            .get_unchecked(component.dc_huff_table).as_ref()
                            .unwrap_or_else(|| std::hint::unreachable_unchecked())
                    };
                    let ac_table = unsafe {
                        self.ac_huffman_tables
                            .get_unchecked(component.ac_huff_table).as_ref()
                            .unwrap_or_else(|| std::hint::unreachable_unchecked())
                    };
                    let mut tmp = [0; DCT_BLOCK];
                    // decode the MCU
                    if !(stream.decode_mcu_block(reader, dc_table, ac_table, &mut tmp, &mut component.dc_pred))
                    {

                        // Read stream and see what marker is stored there
                        //
                        // THe unwrap is safe as the only way for us to hit this is if BitStream::refill_fast() returns
                        // false, which happens after it writes a marker to the destination.
                        let marker = stream.marker.expect("No marker found");

                        match marker {
                            Marker::RST(_) => {
                                // reset stream
                                stream.reset();
                                // Initialize dc predictions to zero for all components
                                self.components.iter_mut().for_each(|x| x.dc_pred = 0);
                                // continue;
                            }
                            Marker::EOI => {
                                debug!("EOI marker found, wrapping up here ");
                                // Okay encountered end of Image break to IDCT and color convert.
                                break 'label;
                            }
                            _ => {
                                return Err(DecodeErrors::MCUError(format!("Marker {:?} found in bitstream, cannot continue", marker)));
                            }
                        }
                    }
                    // A simple trick to ensure we store only needed components
                    if min(self.output_colorspace.num_components()-1, pos) == pos {
                        self.mcu_block[pos][(i * 64)..((i * 64) + 64)].copy_from_slice(&tmp);
                    }
                }
            }

            // Clone things, to make multithreading safe
            // TODO:If need be implement a stream-store copy version..
            let component = global_component.clone();
            let gc = global_channel.clone();

            // Now using threads might affect us in certain ways,
            // And a pretty bad one is writing of MCU's
            // An MCU row may finish faster than one on top of it
            // maybe because it has more zeroes ( we have a short path for this in IDCT)
            // so new we give each thread its start position here and then increment that to fix it
            #[cfg(feature = "threadpool")]
                {
                    // clone block
                    let mut block = self.mcu_block.clone();

                    pool.execute(move || {
                        post_process_non_interleaved(&mut block, &component,
                                                     idct_func, color_convert_16, color_convert,
                                                     input, output, gc,
                                                     mcu_width, width, position);
                    });
                };
            #[cfg(not(feature = "threadpool"))]
                {
                    post_process_non_interleaved(&mut self.mcu_block, &component,
                                                 idct_func, color_convert_16, color_convert,
                                                 input, output, gc,
                                                 mcu_width, width, position);
                }
            // update position here

            // The position will be MCU width * a block * number of pixels written (components per pixel)
            position += width * 8 * self.output_colorspace.num_components();
        }
        // Block this thread until worker threads have finished
        #[cfg(feature = "threadpool")]
            pool.join();
        debug!("Finished decoding image");
        // Global channel may be over allocated for uneven images, shrink it back
        global_channel.lock().expect("Could not get the pixels").truncate(
            usize::from(self.width())
                * usize::from(self.height())
                * self.output_colorspace.num_components(),
        );
        // remove the global channel and return it
        Arc::try_unwrap(global_channel)
            .map_err(|_| DecodeErrors::Format("Could not get pixels, Arc has more than one strong reference".to_string()))?
            .into_inner().map_err(|x| DecodeErrors::Format(format!("Poisoned Mutex\n{}", x)))
    }

    /// Decode an Interleaved(sub-sampled) image
    #[allow(clippy::similar_names)]
    #[inline(never)]
    #[rustfmt::skip]
    pub(crate) fn decode_mcu_ycbcr_interleaved_baseline(
        &mut self,
        reader: &mut Cursor<Vec<u8>>,
    ) -> Result<Vec<u8>, DecodeErrors> {
        #[cfg(feature = "threadpool")]
            let pool = threadpool::Builder::default()
            .thread_name("zune-worker".to_string())
            .build();
        let mcu_width = ((self.info.width + 7) / 8) as usize;


        // A bitstream instance, which will do bit-wise decoding for us
        let mut stream = BitStream::new();

        // Size of our output image(width*height)
        let capacity =
            usize::from(self.info.width + 7) * usize::from(self.info.height + 7) as usize;

        let component_capacity = mcu_width * 64;


        for (pos, comp) in self.components.iter().enumerate() {
            // multiply capacity with sampling factor, it  should be 1*1 for un-sampled images
            self.mcu_block[pos] = vec![0; component_capacity * comp.vertical_sample * comp.horizontal_sample];
        }
        let mut position = 0;

        self.set_upsampling()?;

        let global_channel = Arc::new(Mutex::new(
            vec![0; capacity * self.output_colorspace.num_components()]));

        let global_component = Arc::new(self.components.clone());

        let input = self.input_colorspace;
        let output = self.output_colorspace;

        // function pointers
        let idct_func = self.idct_func;
        let color_convert = self.color_convert;
        let color_convert_16 = self.color_convert_16;

        let h_max = self.h_max;
        let v_max = self.v_max;

        let width = usize::from(self.width());


        // check that dc and AC tables exist outside the hot path
        // If none of these routines fail,  we can use unsafe in the inner loop without it being UB
        for i in 0..self.input_colorspace.num_components()
        {
            let _ = &self.dc_huffman_tables.get(self.components[i].dc_huff_table)
                .as_ref().expect("No DC table for a component").as_ref()
                .expect("No DC table for a component");

            let _ = &self.ac_huffman_tables.get(self.components[i].ac_huff_table)
                .as_ref().expect("No Ac table for component").as_ref()
                .expect("No AC table for a component");
        }
        for _ in 0..self.mcu_y
        {
            'label: for i in 0..self.mcu_x
            {
                // Scan  an interleaved mcu... process scan_n components in order
                for j in 0..self.input_colorspace.num_components()
                {
                    let component = &mut self.components[j];
                    // Get DC and AC tables, we checked that they contain values
                    // before we entered here, so this is safe.
                    let dc_table = unsafe {
                        self.dc_huffman_tables
                            .get_unchecked(component.dc_qt_pos)
                            .as_ref()
                            .unwrap_or_else(|| std::hint::unreachable_unchecked())
                    };
                    let ac_table = unsafe {
                        self.ac_huffman_tables
                            .get_unchecked(component.ac_qt_pos)
                            .as_ref()
                            .unwrap_or_else(|| std::hint::unreachable_unchecked())
                    };

                    for k in 0..component.horizontal_sample
                    {
                        for l in 0..component.vertical_sample
                        {
                            let mut tmp = [0; DCT_BLOCK];
                            let t = &mut component.dc_pred;
                            if !(stream.decode_mcu_block(reader, dc_table, ac_table, &mut tmp, t))
                            {
                                let marker = stream.marker.unwrap();
                                match marker {
                                    Marker::RST(_) => {
                                        // For RST marker, we need to reset stream and initialize predictions to
                                        // zero
                                        // reset stream
                                        stream.reset();
                                        // Initialize dc predictions to zero for all components
                                        //self.components.iter_mut().for_each(|x| x.dc_pred = 0);
                                        // continue;
                                    }
                                    // Okay encountered end of Image break to IDCT and color convert.
                                    Marker::EOI => {
                                        debug!("EOI marker found, wrapping up here ");

                                        break 'label;
                                    }
                                    // pass through
                                    _ => {
                                        warn!(
                                            "Marker {:?} found in bitstream, ignoring it",
                                            marker
                                        );
                                    }
                                }
                            }
                            // write to the block
                            let start = (i * 64) + (k * 64) + (l * 64);
                            self.mcu_block[j][start..start + 64].copy_from_slice(&tmp);
                        }
                    }
                }
            }
            let component = global_component.clone();
            let gc = global_channel.clone();


            // Now using threads might affect us in certain ways,
            // And a pretty bad one is writing of MCU's
            // An MCU row may finish faster than one on top of it maybe because it has more zeroes hence IDCT can be short circuited
            // so new we give each idct its start position here ant then increment that to fix it
            #[cfg(feature = "threadpool")]
                {
                    let mut block = self.mcu_block.clone();

                    pool.execute(move || {
                        post_process_interleaved(&mut block, &component, h_max, v_max, idct_func, color_convert_16,
                                                 color_convert, input, output, gc, mcu_width * h_max * v_max, width, position,
                        );
                    });
                }
            #[cfg(not(feature = "threadpool"))]
                {
                    post_process_interleaved(&mut self.mcu_block, &component, h_max, v_max, idct_func, color_convert_16,
                                             color_convert, input, output, gc, mcu_width * h_max * v_max, width, position,
                    );
                }
            position += width
                * 8 * self.output_colorspace.num_components() * self.h_max * self.v_max;
        }
        #[cfg(feature = "threadpool")]
            pool.join();

        debug!("Finished decoding image");
        // Global channel may be over allocated for uneven images, shrink it back
        global_channel.lock().map_err(|_|
            DecodeErrors::Format("Poisoned value".to_string()))?
            .truncate(usize::from(self.width()) * usize::from(self.height())
                          * self.output_colorspace.num_components(),
            );
        // remove the global channel and return it
        Arc::try_unwrap(global_channel)
            .map_err(|_| DecodeErrors::Format("Arc has more than one strong reference".to_string()))?
            .into_inner().map_err(|x| DecodeErrors::Format(format!("Poisoned mutex\n{}", x)))
    }
}
