use crate::Decoder;

impl Decoder {
    pub(crate) fn color_convert_ycbcr(&mut self, position: usize, output: &mut Vec<u8>) {
        // The logic here is a bit hard
        // The reason is how MCUs are designed, in the input every 64 step represents a MCU
        // but MCU's traverse rows  so we have to do weird skipping and slicing( which is bad cache wise)
        let mut pos = 0;

        let len = self.mcu_block[0].len();
        // for AVX, we can process 16 items for each color_convert function
        // but we do even more weird slicing and a lot of other stuff that increases
        // complexity

        // slice into 128(2 mcu's)
        let mcu_count = len / 128;
        // check if we have an MCU remaining
        let remainder = (len % 128) != 0;

        let mut position = position;
        for i in 0..8 {
            // Process MCU's in batches of 2, this allows us (where applicable) to convert two MCU rows
            // using fewer instructions
            for _ in 0..mcu_count {
                // Here lies bad ugly slicing, and the cause of 75% of the cache misses...

                // First MCU
                let y_c = &self.mcu_block[0][pos..pos + 8];
                let cb_c = &self.mcu_block[1][pos..pos + 8];
                let cr_c = &self.mcu_block[2][pos..pos + 8];

                //  Second MCU
                let y1_c = &self.mcu_block[0][pos + 64..pos + 72];
                let cb2_c = &self.mcu_block[1][pos + 64..pos + 72];
                let cr2_c = &self.mcu_block[2][pos + 64..pos + 72];
                // Call color convert function
                (self.color_convert_16)(y_c, y1_c, cb_c, cb2_c, cr_c, cr2_c, output, position);
                position += 48;
                // increase pos by 128, skip 2 MCU's

                pos += 128;
            }
            pos = (i + 1) * 8;
            if remainder {
                // last odd MCU in the column
                let y_c = &self.mcu_block[0][len - 64..len];
                let cb_c = &self.mcu_block[1][len - 64..len];
                let cr_c = &self.mcu_block[2][len - 64..len];
                // convert function should be able to handle
                // last mcu
                (self.color_convert)(y_c, cb_c, cr_c, output, position);
            }
        }
    }
}
