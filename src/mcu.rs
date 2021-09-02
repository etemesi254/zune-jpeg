use std::io::Cursor;

use crate::{ColorSpace, Decoder};
use crate::bitstream::BitStream;
use crate::marker::Marker;
use crate::unsafe_utils::align_zero_alloc;

const DCT_SIZE: usize = 64;

impl Decoder {
    /// Decode data from MCU's
    #[rustfmt::skip]
    #[allow(clippy::cast_possible_truncation,
    clippy::cast_possible_wrap, clippy::cast_sign_loss, clippy::similar_names)]
    pub(crate) fn decode_mcu_ycbcr(&mut self, reader: &mut Cursor<Vec<u8>>) -> Vec<u8> {
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
        let mut global_channel = unsafe { align_zero_alloc::<u8, 32>(capacity * self.output_colorspace.num_components()) };
        if self.interleaved {
            for _ in 0..self.mcu_y {
                for _ in 0..self.mcu_x {
                    // Scan  an interleaved mcu... process scan_n components in order
                }
            }
        }
        // Non-Interleaved MCUs
        // Carry out decoding in trivial scanline order
        else {
            // check that dc and AC tables exist
            // We can do this outside and for the inner loop, use unsafe
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
                        let mut tmp = [0; DCT_SIZE];
                        // decode the MCU
                        // if false
                        // if true
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
                // carry out IDCT
                (0..self.input_colorspace.num_components()).into_iter().for_each(|i| {
                    (self.idct_func)(self.mcu_block[i].as_mut_slice(), &self.components[i].quantization_table);
                });

                // upsample and color convert
                match (self.input_colorspace, self.output_colorspace) {

                    // YCBCR to RGB(A) colorspace conversion.
                    (ColorSpace::YCbCr, _) => {
                        self.color_convert_ycbcr(&mut position, &mut global_channel, mcu_width);
                    }
                    (ColorSpace::GRAYSCALE, ColorSpace::GRAYSCALE) => {
                        // for grayscale to grayscale we copy first MCU block(which should contain the MCU blocks) to the other
                        let x: &Vec<i16> = self.mcu_block[0].as_ref();
                        global_channel = x.iter().map(|c| *c as u8).collect();
                    }
                    // For the other components we do nothing(currently)
                    _ => {}
                }
                //  position += usize::from(mcu_width - 1) * usize::from(self.output_colorspace.num_components()) * DCT_SIZE;
                // println!("{:?}",position);
            }
        }

        debug!("Finished decoding image");
        // Global channel may be over allocated for uneven images, shrink it back
        global_channel.truncate(usize::from(self.width()) * usize::from(self.height()) * self.output_colorspace.num_components());
        global_channel
    }
}
