#![allow(
    clippy::cast_sign_loss,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_lossless
)]
pub(crate) const FAST_BITS: usize = 9;

#[allow(clippy::module_name_repetitions)]
pub struct HuffmanTable {
    pub(crate) fast: [u8; 1 << FAST_BITS],
    pub(crate) code: [u16; 256],
    pub(crate) values: Vec<u8>,
    pub(crate) size: [u8; 256],
    pub(crate) maxcode: [u32; 18],
    pub(crate) delta: [i32; 17],
    // table for decoding fast AC values
    pub(crate) fast_ac: Option<[i16; 1 << FAST_BITS]>,
}

impl Default for HuffmanTable {
    fn default() -> Self {
        HuffmanTable {
            // set 255 for non-spec acceleration table
            // 255 means not set
            fast: [255; 1 << FAST_BITS],
            // others remain default
            code: [0; 256],
            values: Vec::with_capacity(256),
            size: [0; 256],
            maxcode: [0; 18],
            delta: [0; 17],
            fast_ac: None,
        }
    }
}
impl HuffmanTable {
    pub fn new(codes: &[u8; 16], data: Vec<u8>, is_ac: bool) -> HuffmanTable {
        let mut table = HuffmanTable::default();
        table.build_huffman(codes);
        table.values = data;
        if is_ac {
            table.build_fast_ac();
        }

        table
    }

    fn build_huffman(&mut self, count: &[u8; 16]) {
        //List of Huffman codes corresponding to lengths in HUFFSIZE
        let mut code = 0_u32;

        let mut k = 0;

        // Generate a table of Huffman code sizes
        // Figure C.1
        for i in 0..16 {
            for _ in 0..count[i] {
                self.size[k] = (i + 1) as u8;
                k += 1;
            }
        }
        // last size should be zero.
        self.size[k] = 0;

        // compute actual symbols
        k = 0;
        for j in 1..=16 {
            // compute delta to add code to compute symbol id
            self.delta[j] = k as i32 - code as i32;
            if usize::from(self.size[k]) == j {
                while usize::from(self.size[k]) == j {
                    self.code[k] = code as u16;
                    code += 1;
                    k += 1;
                }
            }
            // compute the largest code_a for this size , pre-shifted as needed later
            self.maxcode[j] = code << (16 - j);
            // shift up by 1(multiply by 2)
            code <<= 1;
        }
        self.maxcode[16] = 0xffff_ffff;

        // build a non spec acceleration table; 255 is flag for not accelerated
        for i in 0..k {
            let s = self.size[i] as usize;

            if s <= (FAST_BITS as usize) {
                let c = self.code[i] << (FAST_BITS - s);
                let m = 1 << (FAST_BITS - s);
                for j in 0..m {
                    self.fast[(c + j) as usize] = i as u8;
                }
            }
        }
    }
    ///  build a table that decodes both magnitude and value of small AC's in one go

    fn build_fast_ac(&mut self) {
        // Build a lookup table for small AC coefficients which both decodes the value and does the
        // equivalent of receive_extend.
        let mut fast_ac = [0; 1 << FAST_BITS];
        for i in 0..1 << FAST_BITS {
            let fast = self.fast[i];
            if fast < 255 {
                let rs = self.values[fast as usize];
                let run = ((rs >> 4) & 15) as i32;
                let magnitude_category = i32::from(rs & 15);
                let len = i32::from(self.size[fast as usize]);
                if magnitude_category != 0 && (len + magnitude_category <= FAST_BITS as i32) {
                    // magnitude code ,followed by receive_extend code
                    let mut k = (((i as i32) << len) & ((1 << FAST_BITS) - 1))
                        >> (FAST_BITS - (magnitude_category) as usize);
                    let m = 1 << (magnitude_category - 1);
                    if k < m {
                        k += (!0_i32 << magnitude_category) + 1;
                    }
                    // if the result is small enough fit in the fast_ac table
                    if k >= -128 && k <= 127 {
                        let r = ((k * 256) + (run * 16) + (len + magnitude_category)) as i16;
                        fast_ac[i] = r;
                    }
                }
            }
        }
        self.fast_ac = Some(fast_ac);
    }
}
