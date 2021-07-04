#![allow(clippy::needless_return)]
#![warn(clippy::correctness, clippy::perf, clippy::pedantic)]
#![allow(arithmetic_overflow)]

#[macro_use]
extern crate log;

pub use crate::image::Decoder;
pub use crate::misc::ColorSpace;

pub mod bitstream;
mod components;
pub mod errors;
mod headers;
mod huffman;
mod idct;
pub mod image;
mod marker;
mod mcu;
mod misc;

mod color_convert;
mod worker;

#[test]
fn decode_jpeg() {
    let image =
        Decoder::decode_file("/home/caleb/IMG_7376.jpg")
            .unwrap();
    println!("{:?}", &image.len());
    println!("{:?}", &image[0..63]);
}
