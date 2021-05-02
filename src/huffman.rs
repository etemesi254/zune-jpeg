//! Parse Huffman tables
//!
//! This module implements a struct that is capable of parsing JPEG Huffman tables
//!
//!

use std::collections::HashMap;

/// Parse a Huffman Entry and create a lookup table
///
/// ![Image of Huffman table](https://upload.wikimedia.org/wikipedia/commons/thumb/8/82/Huffman_tree_2.svg/220px-Huffman_tree_2.svg.png)
///
/// A Huffman lookup table usually consists of a binary tree with nodes which can contain a sub node
/// or a leaf.
/// The leaf contains the actual data, for our case that is a `u8`
///
/// To get actual data, we traverse the tree using bits until we reach a node.
/// Due to the tendency of Huffman Coding allocating most repeating numbers with shorter codes Huffman
/// tables become efficient for decoding, with worst case being `O(N)` where `N` is `16`
///
/// But I use a string,partly because I had a hard time implementing a Binary tree and partly because
/// this implementation is `O(1)` since a request like `HuffmanTable::lookup("00100")` is finding out if
/// `00100` exists in the `HashMap`
#[allow(clippy::module_name_repetitions)]
#[derive(Clone)]
pub struct HuffmanTable {
    pub(crate) lookup_table: HashMap<String, u8>,
}
impl HuffmanTable {
    /// Create a `HuffmanTable` from a Huffman entry in the JPEG
    ///
    /// # Arguments
    /// - symbols:Number of symbols with codes of length 1..16,the sum(n) of these bytes is the total number of codes,
    /// - data:  The symbols in order of increasing,code length ( n = total number of codes ).
    ///
    /// For help on Huffman tables watch [this](https://www.youtube.com/channel/UCyX0cdP8BoSVKddrdluBpyw)
    pub fn from(symbols: [u8; 16], data: &[u8]) -> HuffmanTable {
        let mut lookup_table: HashMap<String, u8> = HashMap::with_capacity(data.len());
        let mut prev_sum = 0;
        let mut i_sum = 0;
        let mut lookup = 0;
        for (pos, i) in symbols.iter().enumerate() {
            i_sum += *i as usize;
            for i in prev_sum..i_sum {
                // $b change the value `lookup` to it's binary representation using `Binary` Trait see
                // https://doc.rust-lang.org/std/fmt/trait.Binary.html

                // :0>width:If the binary digit is smaller than how it's supposed to be, pad it with zeroes
                // a good example being 00 which would be seen as 0,but in a binary tree would have been 2 nodes
                lookup_table.insert(
                    format!("{:0>width$b}", lookup, width = pos + 1),
                    *data.get(i).unwrap(),
                );
                lookup += 1;
            }
            prev_sum += *i as usize;
            // Add zero to the last digit by binary shifting
            lookup <<= 1;
        }

        HuffmanTable { lookup_table }
    }
    /// Lookup bits returning the corresponding value if the bit sequence represents a certain value
    /// or `None` if otherwise
    pub fn lookup(&self, value: &str) -> Option<&u8> {
        // check for value
        if value.len() > 16 {
            panic!("Too long Huffman Code:{}", value);
        }
        return self.lookup_table.get(value);
    }
}
