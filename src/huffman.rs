//! Parse huffman tables
use std::collections::HashMap;

#[derive(Default, Clone)]
pub struct HuffmanTable {
    lookup_table:HashMap<String,u8>,
}
impl HuffmanTable {
    /// Create HuffmanTable
    ///
    /// This negates use of binary tree(partly because I don't know how to implement it
    /// mainly because O(n) is a lot of time) to using a lookup table( basically a HashMap
    /// with a string which corresponds to a value)
    ///
    /// For help on Huffman tables watch [this](https://www.youtube.com/channel/UCyX0cdP8BoSVKddrdluBpyw)
    pub fn from(symbols: [u8; 16], data: Vec<u8>,) -> HuffmanTable {
        let mut lookup_table:HashMap<String,u8> = HashMap::with_capacity(data.len());
        let mut prev_sum = 0;
        let mut i_sum = 0;
        let mut lookup = 0;
        for (pos, i) in symbols.iter().enumerate() {
            i_sum += *i as usize;
            for i in prev_sum..i_sum {
                // $b change the value `lookup` to it's binary representation using `Binary` Trait see
                // https://doc.rust-lang.org/std/fmt/trait.Binary.html

                // :0>width:If the binary digit is smaller than it should be pad it with zeroes
                // a good example being 00 which would be seen as 0,but in a binary tree would have been 2 nodes
                lookup_table.insert(format!("{:0>width$b}", lookup, width = pos + 1), *data.get(i).unwrap());

                lookup += 1;
            }
            prev_sum += *i as usize;
            // Add zero to the last digit by binary shifting
            lookup <<= 1;
        }

        HuffmanTable {
            lookup_table

        }
    }
    /// Lookup bits returning the corresponding value if the bit sequence represents a certain value
    /// or `None` if otherwise
    pub fn lookup(&self, value: &str) -> Option<&u8> {
        // check for value
        return self.lookup_table.get(value)
    }
}
