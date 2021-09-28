//!  A pretty fast JPEG decoder
//!
//! Features
//!  - SSE and AVX accelerated functions to speed up certain decoding operations
//!  - Really fast and accurate 32 bit IDCT algorithm
//!  - Fast color convert functions
//!  - RGBA and RGBX (4-Channel) color conversion functions
//!  - YCbCr to GrayScale conversion.
//!
//! # Speedups unsafe and everything in between
//! - Platform specific vendor intrinsics -> Some implementations like IDCT and
//!   color convert functions contain heavily optimised AVX and SSE routines
//!  which offer a significant speedup , these intrinsics are unsafe hence are
//! annotated with `[target_enable(feature="a feature")]` > Even though they are
//! unsafe , the library gives you the guarantee that these functions will be
//! called only when the CPU running them has support(due to the use of
//! `is_x86_feature_detected`). If a CPU doesn't support them, we will fall back
//! to a scalar implementation
//!
//! - God damn fast `BitStream` decoder
//!
//!
//!
//! Todo
//!  - Up-sampling algorithms.
//!  - Support for interleaved images.
//!  - Support for progressive images.

#![allow(
    clippy::needless_return,
    clippy::similar_names,
    clippy::inline_always,
    clippy::similar_names,
    clippy::doc_markdown
)]
#![warn(
    clippy::correctness,
    clippy::perf,
    clippy::pedantic,
    clippy::inline_always,
    clippy::missing_errors_doc,
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used
)]
//clippy::missing_docs_in_private_items,
#![deny(missing_docs)]

#[macro_use]
extern crate log;

pub use crate::image::Decoder;
pub use crate::misc::ColorSpace;

mod bitstream;
mod components;
pub mod errors;
mod headers;
mod huffman;
mod idct;
mod image;
mod marker;
mod mcu;
mod mcu_prog;
mod misc;

mod color_convert;
mod unsafe_utils;
mod upsampler;
mod worker;

pub use image::*;

#[test]

fn decode_jpeg()
{
    let mut x = Decoder::new();

    x.set_output_colorspace(ColorSpace::RGB);

    let image = x
        .decode_file("/home/caleb/CLionProjects/zune-jpeg/test-images/speed_bench.jpg")
        .unwrap();

    println!("{:?}", &image[0..30]);
    //println!("{:?}", &image[10000..10512]);
}

#[test]

fn decode_jpeg_interleaved()
{
    let mut x = Decoder::new();

    x.set_output_colorspace(ColorSpace::RGB);

    let image = x.decode_file("/home/caleb/IMG_6128.JPG").unwrap();

    println!("{:?}", &image[0..30]);
    //println!("{:?}", &image[10000..10512]);
}

#[test]

fn decode_jpeg_prog()
{
    let _pixels = Decoder::new()
        .decode_file("/home/caleb/Pictures/backgrounds/wallpapers/backgrounds/Mr. Lee.jpg")
        .unwrap();

    println!("{:?}", &_pixels[0..30]);
}
