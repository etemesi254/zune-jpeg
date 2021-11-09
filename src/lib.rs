//!  A pretty fast JPEG decoder
//!
//! # Features
//!  - SSE and AVX accelerated functions to speed up certain decoding operations
//!  - Really fast and accurate 32 bit IDCT algorithm
//!  - Fast color convert functions
//!  - RGBA and RGBX (4-Channel) color conversion functions
//!  - YCbCr to GrayScale conversion.
//!
//! # Speedups unsafe and everything in between
//! ## Unsafety
//! - Platform specific vendor intrinsics -> Some implementations like IDCT and
//!   color convert functions contain heavily optimised AVX and SSE routines
//!   which offer a significant speedup , these intrinsics are unsafe hence are
//!   annotated with `[target_enable(feature="a feature")]` > Even though they are
//!   unsafe , the library gives you the guarantee that these functions will be
//!   called only when the CPU running them has support(due to the use of
//!   `is_x86_feature_detected`). If a CPU doesn't support them, we will fall back
//!   to a scalar implementation.
//!
//! - x86 platform intrinsics can be removed from resulting binary be removing feature `x86` in case
//!  of issues arising with compilation. But please do not do this explicitly. It just reduces performance.
//!
//! ## Multithreading
//! The library is a multi-threaded implementation of a normal decoder,
//! it will allocate too much intermediate memory.
//!
//!If you are limited by memory space, this is probably not for you.
//!
//!
//!## Wasm
//!- This library cannot be used in a wasm context(ideally no third party should be used in a wasm
//!  context use the browser provided one, its way faster than what you can come up with) due to multi
//!  threading which is not supported in the browser.
//! This won't probably change
//!
//!
//!
//! # Accuracy.
//! Accuracy is relative.
//!
//! JPEG is a lossy compression, there will be information lost during decompression and therefore accuracy is usually left
//! to the decode implementation.
//!
//! Pixels returned from this library **WILL** differ from other libraries specifically noting libjpeg-turbo by about
//! +-3. This is because we implement different color conversion functions. The library uses a simple integer
//! ycbcr->rgb conversion function and libjpeg uses fixed point fractional scaled functions.
//!
//! Personally , mine is easy to implement and fast while still maintaining quality but libjpeg-turbo is slightly more accurate.
//!
//! Visually, you probably can't detect differences in pixels returned by these two libraries.
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
    clippy::panic
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
