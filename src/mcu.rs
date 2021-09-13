//! Implements routines to decode a MCU
//!
use std::io::Cursor;
use std::sync::{Arc, Mutex};

use crate::Decoder;
use crate::bitstream::BitStream;
use crate::marker::Marker;
use crate::unsafe_utils::align_zero_alloc;
use crate::worker::{post_process_non_interleaved, post_process_interleaved};
use crate::errors::DecodeErrors;

const DCT_BLOCK: usize = 64;

impl Decoder {
    #[allow(clippy::similar_names)]
    #[inline(never)]
    pub(crate) fn decode_mcu_ycbcr_non_interleaved(&mut self, reader: &mut Cursor<Vec<u8>>) -> Vec<u8> {
        let pool = threadpool::ThreadPool::default();
        let mcu_width = ((self.info.width + 7) / 8) as usize;
        let mcu_height = ((self.info.height + 7) / 8) as usize;


        // A bitstream instance, which will do bit-wise decoding for us
        let mut stream = BitStream::new();
        // Size of our output image(width*height)
        let capacity = usize::from(self.info.width + 7) * usize::from(self.info.height + 7) as usize;

        let component_capacity = mcu_width * 64;

        // The following contains containers for unprocessed values
        // by unprocessed we mean they haven't been dequantized and inverse DCT carried on them

        // for those pointers storing unprocessed items, zero them out here
        // num_components cannot go above MAX_COMPONENTS(currently all are at a max of 4)
        for (pos, comp) in self.components.iter().enumerate() {
            // multiply be vertical and horizontal sample , it  should be 1*1 for non-sampled images
            self.mcu_block[pos] =
                unsafe {
                    align_zero_alloc::<i16, 32>(component_capacity * comp.vertical_sample * comp.horizontal_sample)
                };
        }
        let mut position = 0;
        // Create an Arc of components to prevent cloning on every MCU width
        // we can just send this Arc on clone
        let global_component = Arc::new(self.components.clone());
        let global_channel = Arc::new(Mutex::new(unsafe { align_zero_alloc::<u8, 32>(capacity * self.output_colorspace.num_components()) }));
        // check that dc and AC tables exist outside the hot path
        // If none of these routines fail,  we can use unsafe in the inner loop without it being UB
        for i in 0..self.input_colorspace.num_components() {
            let _ = &self.dc_huffman_tables.get(self.components[i].dc_table_pos).as_ref()
                .expect("No DC table for a component")
                .as_ref()
                .expect("No DC table for a component");
            let _ = &self.ac_huffman_tables.get(self.components[i].ac_table_pos).as_ref()
                .expect("No Ac table for component")
                .as_ref()
                .expect("No AC table for a component");
        }
        for _ in 0..mcu_height {
            'label: for i in 0..mcu_width {
                // iterate over components, for non interleaved scans
                for pos in 0..self.input_colorspace.num_components() {
                    let component = &mut self.components[pos];
                    // The checks were performed above, before we get to the hot loop
                    // Basically, the decoder will panic ( see where the else statement starts)
                    // so these `un-safes` are safe
                    let dc_table = unsafe {
                        self.dc_huffman_tables
                            .get_unchecked(component.dc_table_pos)
                            .as_ref()
                            .unwrap_or_else(|| std::hint::unreachable_unchecked())
                    };
                    let ac_table = unsafe {
                        self.ac_huffman_tables
                            .get_unchecked(component.ac_table_pos)
                            .as_ref()
                            .unwrap_or_else(|| std::hint::unreachable_unchecked())
                    };
                    let mut tmp = [0; DCT_BLOCK];
                    // decode the MCU
                    if !(stream.decode_fast(reader, dc_table, ac_table, &mut tmp, &mut component.dc_pred)) {
                        // if false we should read stream and see what marker is stored there
                        //
                        // THe unwrap is safe as the only way for us to hit this is if BitStream::refill_fast() returns
                        // false, which happens after it writes a marker to the destination.
                        let marker = stream.marker.unwrap();

                        match marker {
                            Marker::RST(_) => {
                                // For RST marker, we need to reset stream and initialize predictions to
                                // zero
                                // reset stream
                                stream.reset();
                                // Initialize dc predictions to zero for all components
                                self.components.iter_mut().for_each(|x| x.dc_pred = 0);
                                // continue;
                            }
                            // Okay encountered end of Image break to IDCT and color convert.
                            Marker::EOI => {
                                debug!("EOI marker found, wrapping up here ");

                                break 'label;
                            }
                            // pass through
                            _ => {
                                warn!("Marker {:?} found in bitstream, ignoring it",marker);
                            }
                        }
                    }
                    // Write to 64 elements the region containing unprocessed items
                    self.mcu_block[pos][(i * 64)..((i * 64) + 64)].copy_from_slice(&tmp);
                }
            }
            // Clone things, to make multithreading safe
            // Ideally most of the time will be wasted in mcu_block.clone()
            // TODO:If need be implement a stream-store copy version..
            let block = self.mcu_block.clone();
            let component = global_component.clone();
            let input = self.input_colorspace;
            let output = self.output_colorspace;
            let idct_func = self.idct_func;
            let color_convert = self.color_convert;
            let color_convert_16 = self.color_convert_16;
            let width = usize::from(self.width());
            let gc = global_channel.clone();


            // Now using threads might affect us in certain ways,
            // And a pretty bad one is writing of MCU's
            // An MCU row may finish faster than one on top of it maybe because it has more zeroes hence IDCT can be short circuited
            // so new we give each idct its start position here ant then increment that to fix it
            pool.execute(move || {
                post_process_non_interleaved(block, &component,
                                             idct_func, color_convert_16, color_convert,
                                             input, output, gc,
                                             mcu_width, width, position);
            });
            // update position here
            // The position will be MCU width * a block * number of pixels written (components per pixel)
            position += mcu_width * DCT_BLOCK * self.output_colorspace.num_components();
            //println!("{}",position);
        }
        // Block all pools waiting for it to join
        pool.join();
        debug!("Finished decoding image");
        // Global channel may be over allocated for uneven images, shrink it back
        global_channel.lock().unwrap().truncate(usize::from(self.width()) * usize::from(self.height()) * self.output_colorspace.num_components());
        // remove the global channel and return it
        Arc::try_unwrap(global_channel).unwrap().into_inner().unwrap()
    }
    #[inline(never)]
    pub(crate) fn decode_mcu_ycbcr_interleaved(&mut self, reader: &mut Cursor<Vec<u8>>) -> Result<Vec<u8>,DecodeErrors> {
        let pool = threadpool::ThreadPool::default();
        let mcu_width = ((self.info.width + 7) / 8) as usize;

        // A bitstream instance, which will do bit-wise decoding for us
        let mut stream = BitStream::new();
        // Size of our output image(width*height)
        let capacity = usize::from(self.info.width + 7) * usize::from(self.info.height + 7) as usize;

        let component_capacity = mcu_width * 64;

        // The following contains containers for unprocessed values
        // by unprocessed we mean they haven't been dequantized and inverse DCT carried on them

        // for those pointers storing unprocessed items, zero them out here
        // num_components cannot go above MAX_COMPONENTS(currently all are at a max of 4)
        for (pos, comp) in self.components.iter().enumerate() {
            // multiply be vertical and horizontal sample , it  should be 1*1 for non-sampled images
            self.mcu_block[pos] =
                unsafe {
                    align_zero_alloc::<i16, 32>(component_capacity * comp.vertical_sample * comp.horizontal_sample)
                };
        }
        let mut position = 0;
        self.set_upsampling()?;

        let global_channel = Arc::new(Mutex::new(unsafe { align_zero_alloc::<u8, 32>(capacity * self.output_colorspace.num_components()) }));
        let global_component = Arc::new(self.components.clone());

        // check that dc and AC tables exist outside the hot path
        // If none of these routines fail,  we can use unsafe in the inner loop without it being UB
        for i in 0..self.input_colorspace.num_components() {
            let _ = &self.dc_huffman_tables.get(self.components[i].dc_table_pos).as_ref()
                .expect("No DC table for a component")
                .as_ref()
                .expect("No DC table for a component");
            let _ = &self.ac_huffman_tables.get(self.components[i].ac_table_pos).as_ref()
                .expect("No Ac table for component")
                .as_ref()
                .expect("No AC table for a component");
        }
        for _ in 0..self.mcu_y {
            'label: for i in 0..self.mcu_x {
                // Scan  an interleaved mcu... process scan_n components in order
                for j in 0..self.input_colorspace.num_components() {
                    let component = &mut self.components[j];
                    // Get DC and AC tables, we checked that they contain values
                    // before we entered here, so this is safe.
                    let dc_table = unsafe {
                        self.dc_huffman_tables
                            .get_unchecked(component.dc_table_pos)
                            .as_ref()
                            .unwrap_or_else(|| std::hint::unreachable_unchecked())
                    };
                    let ac_table = unsafe {
                        self.ac_huffman_tables
                            .get_unchecked(component.ac_table_pos)
                            .as_ref()
                            .unwrap_or_else(|| std::hint::unreachable_unchecked())
                    };

                    for k in 0..component.horizontal_sample {
                        for l in 0..component.vertical_sample {
                            let mut tmp = [0; DCT_BLOCK];
                            let t = &mut component.dc_pred;
                            if !(stream.decode_fast(reader, dc_table, ac_table, &mut tmp, t)) {
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
                                        warn!("Marker {:?} found in bitstream, ignoring it",marker);
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
            let block = self.mcu_block.clone();
            let component = global_component.clone();
            let input = self.input_colorspace;
            let output = self.output_colorspace;
            let idct_func = self.idct_func;
            let color_convert = self.color_convert;
            let color_convert_16 = self.color_convert_16;
            let width = usize::from(self.width());
            let gc = global_channel.clone();


            // Now using threads might affect us in certain ways,
            // And a pretty bad one is writing of MCU's
            // An MCU row may finish faster than one on top of it maybe because it has more zeroes hence IDCT can be short circuited
            // so new we give each idct its start position here ant then increment that to fix it
            pool.execute(move || {
                post_process_interleaved(block, &component,
                                             idct_func, color_convert_16, color_convert,
                                             input, output, gc,
                                             mcu_width, width, position);
            });
            position += mcu_width * DCT_BLOCK * self.output_colorspace.num_components();



        }
        pool.join();
        debug!("Finished decoding image");
        // Global channel may be over allocated for uneven images, shrink it back
        global_channel.lock().unwrap().truncate(usize::from(self.width()) * usize::from(self.height()) * self.output_colorspace.num_components());
        // remove the global channel and return it
        Ok(Arc::try_unwrap(global_channel).unwrap().into_inner().unwrap())
    }
}
