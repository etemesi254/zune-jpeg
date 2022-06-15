//!  A pretty fast JPEG decoder
//!
//! # Features
//!  - SSE and AVX accelerated functions to speed up certain decoding operations
//!  - Really fast and accurate 32 bit IDCT algorithm
//!  - Fast color convert functions
//!  - RGBA and RGBX (4-Channel) color conversion functions
//!  - YCbCr to GrayScale conversion.
//!
//! # Examples
//!
//! ## Decode a JPEG file with default arguments.
//! ```no_run
//! use zune_jpeg::Decoder;
//! //will contain pixels
//! let mut pixels = Decoder::new().decode_file("a_jpeg_file").unwrap();
//!
//! ```
//!
//! ## Decode a JPEG file to RGBA format
//! ```no_run
//! use zune_jpeg::Decoder;
//! let mut decoder = Decoder::new();
//! decoder.rgba(); // or equivalently decoder.set_output_colorspace(ColorSpace::RGBA)
//! decoder.decode_file("a_jpeg_file");
//! ```
//!
//! ## Decode an image and get it's width and height.
//! ```no_run
//! use zune_jpeg::Decoder;
//! let mut decoder = Decoder::new();
//! decoder.set_output_colorspace(zune_jpeg::ColorSpace::GRAYSCALE);
//! decoder.decode_file("a_jpeg_file");
//! let image_info = decoder.info().unwrap();
//! println!("{},{}",image_info.width,image_info.height)
//! ```

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
    clippy::panic
)]
#![cfg_attr(not(feature = "x86"), forbid(unsafe_code))]

#[macro_use]
extern crate log;

pub use crate::decoder::{Decoder, ImageInfo};
pub use crate::misc::ColorSpace;

mod bitstream;
mod color_convert;
mod components;
mod decoder;
pub mod errors;
mod headers;
mod huffman;
mod idct;
mod marker;
mod mcu;
mod mcu_prog;
mod misc;
mod unsafe_utils;
mod upsampler;
mod worker;
