#![allow(clippy::needless_return,clippy::similar_names)]
#![warn(clippy::correctness, clippy::perf, clippy::pedantic,clippy::inline_always)]
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


