#![allow(clippy::needless_return)]
#![warn(clippy::correctness, clippy::perf, clippy::pedantic)]
#[macro_use]
extern crate log;

#[macro_use]
extern crate ndarray;

pub use zune_traits::image::Image;

pub use crate::image::JPEG;
mod bitstream;
pub mod errors;
mod huffman;
mod idct;
pub mod image;
mod markers;
mod mcu;
mod misc;
mod threads;
mod yuv_to_rgb;

#[test]
fn decode_jpeg() {
    let image = JPEG::decode_file(
        "/home/caleb/CLionProjects/zune-jpeg/test-images/test-baseline.jpg".to_string(),
    )
    .unwrap();


}
