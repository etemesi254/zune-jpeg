use std::io::Cursor;

use crate::{ColorSpace, Decoder};
use crate::bitstream::BitStream;
use crate::marker::Marker;

impl Decoder {
    /// Decode data from MCU's
    #[rustfmt::skip]
    #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
    pub(crate) fn decode_mcu_ycbcr(&mut self, reader: &mut Cursor<Vec<u8>>) -> Vec<u8> {
        let mcu_width = ((self.info.width + 7) / 8) as usize;
        let mcu_height = (self.info.height + 7) / 8;

        // A bitstream instance, which will do bit-wise decoding for us
        let mut stream = BitStream::new();
        // Size of our output image(width*height)
        let capacity = usize::from(self.info.width) * usize::from(self.info.height) as usize;

        let component_capacity = usize::from(mcu_width) * 64;

        // The following contains containers for unprocessed values
        // by unprocessed we mean they haven't been dequantized and inverse DCT carried on them

        // for those pointers storing unprocessed items, zero them out here
        // num_components cannot go above MAX_COMPONENTS(currently all are at a max of 4)
        for (pos, comp) in self.components.iter().enumerate() {
            // multiply be vertical and horizontal sample , it  should be 1*1 for non-sampled images
            self.mcu_block[pos] = vec![0; component_capacity * comp.vertical_sample * comp.horizontal_sample];
        }
        let mut position = 0;
        let mut global_channel = vec![0; capacity * self.output_colorspace.num_components()];
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

            for _ in 0..mcu_height {
                for i in 0..mcu_width {
                    // iterate over components, for non interleaved scans
                    for position in 0..self.input_colorspace.num_components() {
                        let component = &mut self.components[position];

                        let dc_table = self.dc_huffman_tables[component.dc_table_pos].as_ref().
                            unwrap_or_else(|| panic!("Could not get DC table for component {:?}", component.component_id));
                        let ac_table = self.ac_huffman_tables[component.ac_table_pos].as_ref().
                            unwrap_or_else(|| panic!("Could not get DC table for component {:?}", component.component_id));

                        let mut tmp = [0; 64];
                        // decode the MCU
                       if  !(stream.decode_fast(reader, dc_table, ac_table, &mut tmp, &mut component.dc_pred)){
                           // if false we should read stream and see what marker is stored there
                           let marker = stream.marker.unwrap();
                           // okay check if marker is a rst
                           match marker {
                               Marker::RST(_)=>{
                                   // reset stream
                                    stream.reset();
                                   // Initialize dc predictions to zero for all components
                                   self.components.iter_mut().for_each(|x| x.dc_pred=0);

                               }
                               // pass through
                               _=>{
                                   warn!("Marker {:?} found in bitstream, ignoring it",marker)
                               }
                           }
                       }
                        // Write to 64 elements the region containing unprocessed items
                        self.mcu_block[position][(i * 64)..(i * 64 + 64)].copy_from_slice(&tmp);
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
                        self.color_convert_ycbcr(position, &mut global_channel);
                    }
                    (ColorSpace::GRAYSCALE, ColorSpace::GRAYSCALE) => {
                        // for grayscale to grayscale we copy first MCU block(which should contain the MCU blocks) to the other
                        let x: &Vec<i16> = self.mcu_block[0].as_ref();
                        global_channel = x.iter().map(|c| *c as u8).collect();
                    }
                    // For the other components we do nothing(currently)
                    _ => {}
                }
                position += usize::from(mcu_width << 3);
            }
        }

        debug!("Finished decoding image");
        global_channel
    }
}
