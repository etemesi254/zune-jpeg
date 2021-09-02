use crate::Decoder;

impl Decoder {
    #[allow(clippy::similar_names)]
    pub(crate) fn color_convert_ycbcr(
        &mut self,
        position: &mut usize,
        output: &mut Vec<u8>,
        mcu_len: usize,
    ) {
        // The logic here is a bit hard
        // The reason is how MCUs are designed, in the input every 64 step represents a MCU
        // but MCU's traverse rows  so we have to do weird skipping and slicing( which is bad cache wise)
        let mut pos = 0;

        // slice into 128(2 mcu's)
        //println!("{}",mcu_len);
        let mcu_count = (mcu_len - 1) >> 1;
        // check if we have an MCU remaining
        let remainder = ((mcu_len - 1) % 2) != 0;
        let mcu_width = usize::from(self.info.width) * self.output_colorspace.num_components();
        let mut expected_pos = *position + mcu_width;
        for i in 0..8 {
            // Process MCU's in batches of 2, this allows us (where applicable) to convert two MCU rows
            // using fewer instructions
            //println!("{},{}",position,pos);
            for _ in 0..mcu_count {
                //This isn't cache efficient as it hops around too much

                // SAFETY
                // 1. mcu_block is initialized, (note not assigned) with zeroes
                // enough to ensure that this is unsafe,
                // The bounds here can never go above the length
                unsafe {
                    // remove some cmp instructions that were slowing us down
                    let y_c = self.mcu_block[0].get_unchecked(pos..pos + 8);
                    let cb_c = self.mcu_block[1].get_unchecked(pos..pos + 8);
                    let cr_c = &self.mcu_block[2].get_unchecked(pos..pos + 8);
                    //  8 pixels of the second MCU
                    let y1_c = &self.mcu_block[0].get_unchecked(pos + 64..pos + 72);
                    let cb2_c = &self.mcu_block[1].get_unchecked(pos + 64..pos + 72);
                    let cr2_c = &self.mcu_block[2].get_unchecked(pos + 64..pos + 72);
                    // Call color convert function
                    (self.color_convert_16)(y_c, y1_c, cb_c, cb2_c, cr_c, cr2_c, output, position);
                    // increase pos by 128, skip 2 MCU's
                }
                pos += 128;
            }

            if remainder {
                // last odd MCU in the column
                let y_c = &self.mcu_block[0][pos..pos + 8];
                let cb_c = &self.mcu_block[1][pos..pos + 8];
                let cr_c = &self.mcu_block[2][pos..pos + 8];
                // convert function should be able to handle
                // last mcu
                (self.color_convert)(y_c, cb_c, cr_c, output, position);
                //*position+=24;
            }

            // Sometimes Color convert may overshoot, IE if the image is not
            // divisible by 8 it may have to pad the last MCU with extra pixels
            // The decoder is supposed to discard these extra bits
            //
            // But instead of discarding those bits, I just tell the color_convert to overwrite them
            // Meaning I have to reset position to the expected position, which is the width
            // of the MCU.

            *position = expected_pos;
            expected_pos += mcu_width;

            // Reset position to start fetching from the next MCU
            pos = (i + 1) * 8;
        }
    }
}
