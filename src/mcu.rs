//! Implements routines to decode a MCU

use std::cmp::min;
use std::io::Cursor;
use std::sync::{Arc, Mutex};

use crate::bitstream::BitStream;
use crate::Decoder;
use crate::errors::DecodeErrors;
use crate::marker::Marker;
use crate::worker::post_process;

/// The size of a DC block for a MCU.

pub const DCT_BLOCK: usize = 64;

impl Decoder
{
    /// Decode an MCU with no interleaving aka the components were not down-sampled
    /// hence are arranged in Y,Cb,Cr fashion
    #[allow(clippy::similar_names)]
    #[inline(never)]
    #[rustfmt::skip]
    pub(crate) fn decode_mcu_ycbcr_baseline(&mut self,
                                            reader: &mut Cursor<Vec<u8>>,
    ) -> Result<Vec<u8>, DecodeErrors> {
        #[cfg(feature = "threadpool")]
            let pool = threadpool::ThreadPool::default();

        let (mcu_width, mcu_height);
        if self.interleaved {
            // set upsampling functions
            self.set_upsampling()?;

            mcu_width = self.mcu_x;

            mcu_height = self.mcu_y;
        } else {
            mcu_width = ((self.info.width + 7) / 8) as usize;

            mcu_height = ((self.info.height + 7) / 8) as usize;
        }


        let mut stream = BitStream::new();
        // Size of our output image(width*height)
        let capacity = usize::from(self.info.width + 7) * usize::from(self.info.height + 7);

        let component_capacity = usize::from((self.info.width + 7) / 8) * DCT_BLOCK;
        // for those pointers storing unprocessed items, zero them out here
        for (pos, comp) in self.components.iter().enumerate() {
            // multiply capacity with sampling factor, it  should be 1*1 for un-sampled images

            //NOTE: We only allocate a block if we need it, so e.g for grayscale
            // we don't allocate for CB and Cr channels
            if min(self.output_colorspace.num_components() - 1, pos) == pos
            {
                self.mcu_block[pos] = vec![0; component_capacity * comp.vertical_sample * comp.horizontal_sample];
            }
        }
        let mut position = 0;

        // Create an Arc of components to prevent cloning on every MCU width
        let global_component = Arc::new(self.components.clone());

        // Storage for decoded pixels
        let global_channel = Arc::new(Mutex::new(
            vec![0; capacity * self.output_colorspace.num_components()]));

        // things needed for post processing that we can remove out of the loop
        let input = self.input_colorspace;

        let output = self.output_colorspace;

        let idct_func = self.idct_func;

        let color_convert = self.color_convert;

        let color_convert_16 = self.color_convert_16;

        let width = usize::from(self.width());

        let h_max = self.h_max;

        let v_max = self.v_max;

        // check that dc and AC tables exist outside the hot path
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
            'label: for _ in 0..mcu_width
            {
                // iterate over components
                for pos in 0..self.input_colorspace.num_components()
                {
                    // if image is interleaved iterate over scan-n components,
                    // otherwise if it-s non-interleaved, these routines iterate in
                    // trivial scanline order(Y,Cb,Cr)
                    let component = &mut self.components[pos];
                    let dc_table = unsafe
                        {
                            self.dc_huffman_tables
                                .get_unchecked(component.dc_huff_table).as_ref()
                                .unwrap_or_else(|| std::hint::unreachable_unchecked())
                        };
                    let ac_table = unsafe {
                        self.ac_huffman_tables
                            .get_unchecked(component.ac_huff_table).as_ref()
                            .unwrap_or_else(|| std::hint::unreachable_unchecked())
                    };
                    for _ in 0..component.horizontal_sample{
                        for _ in 0..component.vertical_sample {

                            let mut tmp = [0; DCT_BLOCK];
                            // decode the MCU
                            if !(stream.decode_mcu_block(reader, dc_table, ac_table, &mut tmp, &mut component.dc_pred))
                            {
                                // Found a marker

                                // Read stream and see what marker is stored there
                                let marker = stream.marker.expect("No marker found");

                                match marker {
                                    Marker::RST(_) => {
                                        // reset stream
                                        stream.reset();
                                        // Initialize dc predictions to zero for all components
                                        // self.components.iter_mut().for_each(|x| x.dc_pred = 0);
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
                            // Store only needed components (i.e for YCbCr->Grayscale don't store Cb and Cr channels)
                            // improves speed when we do a clone(less items to clone)
                            if min(self.output_colorspace.num_components() - 1, pos) == pos {
                                let counter = component.counter;
                                self.mcu_block[pos][counter..counter + 64].copy_from_slice(&tmp);

                                component.counter += 64;
                            }
                        }
                    }
                }
            }

            // reset counter
            self.components.iter_mut().for_each(|x| x.counter = 0);

            // Clone things, to make multithreading safe
            let component = global_component.clone();

            let gc = global_channel.clone();

            // FIXME: Fix this for single-threaded functions, should not be cloning
            let mut block = self.mcu_block.clone();

            #[cfg(feature = "threadpool")]
                {

                    pool.execute(move || {
                        post_process(&mut block, &component, h_max, v_max,
                                     idct_func, color_convert_16, color_convert,
                                     input, output, gc,
                                     mcu_width, width, position);
                    });
                };
            #[cfg(not(feature = "threadpool"))]
                {
                    post_process(&mut block, &component, h_max, v_max,
                                 idct_func, color_convert_16, color_convert,
                                 input, output, gc,
                                 mcu_width, width, position);
                }

            // update position here
            position += width * 8 * self.output_colorspace.num_components() * h_max * v_max;
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

        // remove the global channel from Arc and return it
        Arc::try_unwrap(global_channel)
            .map_err(|_| DecodeErrors::Format("Could not get pixels, Arc has more than one strong reference".to_string()))?
            .into_inner().map_err(|x| DecodeErrors::Format(format!("Poisoned Mutex\n{}", x)))
    }
}
