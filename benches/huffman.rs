//! Benchmark the Huffman implementation
//!
//! This is here to compare different implementations of a JPEG Huffman decoder
use std::collections::HashMap;
use criterion::{black_box, criterion_group, criterion_main, Criterion};


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
    /// For help on Huffman tables see [this](https://www.youtube.com/channel/UCyX0cdP8BoSVKddrdluBpyw)
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

fn test_huffman(symbols:[u8;16],data:Vec<u8>) -> HuffmanTable {
    return HuffmanTable::from(symbols,data);
}
fn criterion_benchmark(c: &mut Criterion) {
    let symbols=[0, 2, 1, 2, 4, 4, 3, 4, 7, 5, 4, 4, 0, 1, 2, 119];
    let data=vec![0, 1, 2, 3, 17, 4, 5, 33, 49, 6, 18, 65, 81, 7, 97, 113, 19, 34, 50, 129, 8, 20, 66, 145, 161, 177, 193, 9, 35, 51, 82, 240, 21, 98, 114, 209, 10, 22, 36, 52, 225, 37, 241, 23, 24, 25, 26, 38, 39, 40, 41, 42, 53, 54, 55, 56, 57, 58, 67, 68, 69, 70, 71, 72, 73, 74, 83, 84, 85, 86, 87, 88, 89, 90, 99, 100, 101, 102, 103, 104, 105, 106, 115, 116, 117, 118, 119, 120, 121, 122, 130, 131, 132, 133, 134, 135, 136, 137, 138, 146, 147, 148, 149, 150, 151, 152, 153, 154, 162, 163, 164, 165, 166, 167, 168, 169, 170, 178, 179, 180, 181, 182, 183, 184, 185, 186, 194, 195, 196, 197, 198, 199, 200, 201, 202, 210, 211, 212, 213, 214, 215, 216, 217, 218, 226, 227, 228, 229, 230, 231, 232, 233, 234, 242, 243, 244, 245, 246, 247, 248, 249, 250];

    c.bench_function("Huffman Decoder", |b| b.iter(|| test_huffman(black_box(symbols.clone()),black_box(data.clone()))));



}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);