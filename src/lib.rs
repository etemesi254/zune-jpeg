#![allow(
    clippy::needless_return,
    clippy::similar_names,
    clippy::inline_always,
    clippy::similar_names
)]
#![warn(
    clippy::correctness,
    clippy::perf,
    clippy::pedantic,
    clippy::inline_always
)]
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
mod unsafe_utils;
mod worker;

#[test]
fn decode_jpeg() {
    let mut x = Decoder::new();
    x.set_output_colorspace(ColorSpace::RGBX);
    let image = x
        .decode_file("/home/caleb/Pictures/backgrounds/wallpapers/backgrounds/Mr. Lee.jpg")
        .unwrap();

    println!("{:?}", &image[0..30]);
    //println!("{:?}", &image[10000..10512]);
}
