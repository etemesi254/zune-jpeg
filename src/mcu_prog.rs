use std::io::Cursor;

use crate::bitstream::BitStream;
use crate::mcu::DCT_BLOCK;
use crate::{errors::DecodeErrors, Decoder};
use std::sync::{Arc, Mutex};

impl Decoder {
    /// Decode a progressive image with no interleaving.
    pub(crate) fn decode_mcu_ycbcr_non_interleaved_prog(
        &mut self,
        reader: &mut Cursor<Vec<u8>>,
    ) -> Result<Vec<u8>, DecodeErrors> {
        let _pool = threadpool::ThreadPool::default();
        let mcu_width = ((self.info.width + 7) / 8) as usize;
        let mcu_height = ((self.info.width + 7) / 8) as usize;

        // Bitstream
        let mut stream = BitStream::new_progressive(self.ah, self.al, self.ss, self.se);
        // size of output image(width*height), larger than normal to give space for uneven images

        // Size of our output image(width*height)
        let capacity =
            usize::from(self.info.width + 7) * usize::from(self.info.height + 7) as usize;

        let component_capacity = mcu_width * DCT_BLOCK;

        // The following contains containers for unprocessed values
        // by unprocessed we mean they haven't been dequantized and inverse DCT carried on them

        // for those pointers storing unprocessed items, zero them out here
        // num_components cannot go above MAX_COMPONENTS(currently all are at a max of 4)
        for (pos, comp) in self.components.iter().enumerate() {
            // multiply capacity with sampling factor, it  should be 1*1 for un-sampled images
            self.mcu_block[pos] =
                vec![0; component_capacity * comp.vertical_sample * comp.horizontal_sample];
        }
        let mut _position = 0;
        // Create an Arc of components to prevent cloning on every MCU width
        // we can just send this Arc on clone
        let _global_component = Arc::new(self.components.clone());
        let _global_channel = Arc::new(Mutex::new(vec![
            0;
            capacity
                * self
                    .output_colorspace
                    .num_components()
        ]));

        // things needed for post processing that we can remove out of the loop
        // since they are copy, it should not be that hard
        let _input = self.input_colorspace;
        let _output = self.output_colorspace;
        let _idct_func = self.idct_func;
        let _color_convert = self.color_convert;
        let _color_convert_16 = self.color_convert_16;
        let _width = usize::from(self.width());

        // check that dc and AC tables exist outside the hot path
        // If none of these routines fail,  we can use unsafe in the inner loop without it being UB
        for i in 0..self.input_colorspace.num_components() {
            //println!("{},{}",self.components[i].dc_huff_table,self.components[i].ac_huff_table);
            let _ = &self
                .dc_huffman_tables
                .get(self.components[i].dc_huff_table)
                .as_ref()
                .ok_or_else(|| {
                    DecodeErrors::HuffmanDecode(format!(
                        "No Huffman DC table for component {:?} ",
                        self.components[i].component_id
                    ))
                })?
                .as_ref()
                .ok_or_else(|| {
                    DecodeErrors::HuffmanDecode(format!(
                        "No DC table for component {:?}",
                        self.components[i].component_id
                    ))
                })?;

            let _ = &self
                .ac_huffman_tables
                .get(self.components[i].ac_huff_table)
                .as_ref()
                .ok_or_else(|| {
                    DecodeErrors::HuffmanDecode(format!(
                        "No Huffman AC table for component {:?} ",
                        self.components[i].component_id
                    ))
                })?
                .as_ref()
                .ok_or_else(|| {
                    DecodeErrors::HuffmanDecode(format!(
                        "No AC table for component {:?}",
                        self.components[i].component_id
                    ))
                })?;
        }
        // Hot loop begins here
        for _ in 0..mcu_height {
            '_label: for i in 0..mcu_width {
                let _start = i * 64;
                // iterate over components, for non-interleaved scans
                for pos in 0..self.input_colorspace.num_components() {
                    let component = &mut self.components[pos];

                    // The checks were performed above, before we get to the hot loop
                    // Basically, the decoder will panic ( see where the else statement starts)
                    // so these `un-safes` are safe

                    // These cause performance regressions  if I do the normal bounds-check here ,
                    // so I won't remove them.
                    let dc_table = unsafe {
                        self.dc_huffman_tables
                            .get_unchecked(component.dc_qt_pos)
                            .as_ref()
                            .unwrap_or_else(|| std::hint::unreachable_unchecked())
                    };
                    let _ac_table = unsafe {
                        self.ac_huffman_tables
                            .get_unchecked(component.ac_qt_pos)
                            .as_ref()
                            .unwrap_or_else(|| std::hint::unreachable_unchecked())
                    };
                    if self.ss == 0 {
                        // first iteration
                        let mut block = [0_i16; DCT_BLOCK];
                        // decode block dc
                        stream.decode_block_dc(
                            reader,
                            dc_table,
                            &mut block,
                            &mut component.dc_pred,
                        )?;
                    }
                }
            }
        }

        return Ok(Vec::new());
    }
}
