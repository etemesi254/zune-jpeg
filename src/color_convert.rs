#![allow(
    clippy::many_single_char_names,
    clippy::similar_names,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    clippy::too_many_arguments,
    clippy::doc_markdown
)]
//! Color space conversion routines
//!
//! This files exposes functions to convert one colorspace to another in a jpeg
//! image
//!
//! Currently supported conversions are
//!
//! - `YCbCr` to `RGB,RGBA,GRAYSCALE,RGBX`.
//!
//!
//! Hey there, if your reading this it means you probably need something, so let me help you.
//!
//! There are 3 supported cpu extensions here.
//! 1. Scalar
//! 2. SSE
//! 3. AVX
//!
//! There are two types of the color convert functions
//!
//! 1. Acts on 16 pixels.
//! 2. Acts on 8 pixels.
//!
//! The reason for this is because when implementing the AVX part it occurred to me that we can actually
//! do better and process 2 MCU's if we change IDCT return type to be `i16's`, since a lot of
//! CPU's these days support AVX extensions, it becomes nice if we optimize for that path ,
//! therefore AVX routines can process 16 pixels directly and SSE and Scalar just compensate.
//!
//! By compensating, I mean I wrote the 16 pixels version operating on the 8 pixel version twice.
//!
//! Therefore if your looking to optimize some routines, probably start there.

use std::cmp::{max, min};
use std::convert::TryInto;

#[cfg(feature = "x86")]
pub use crate::color_convert::avx::{ycbcr_to_rgb_avx2, ycbcr_to_rgba_avx2, ycbcr_to_rgbx_avx2};
#[cfg(feature = "x86")]
pub use crate::color_convert::sse::{
    ycbcr_to_rgb_sse, ycbcr_to_rgb_sse_16, ycbcr_to_rgba_sse, ycbcr_to_rgba_sse_16,
};
use crate::{ColorConvert16Ptr, ColorConvertPtr, ColorSpace};

pub mod avx;
pub mod sse;

/// Limit values to 0 and 255
#[inline]
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, dead_code)]
fn clamp(a: i16) -> u8
{
    min(max(a, 0), 255) as u8
}

pub fn ycbcr_to_rgb_scalar(y: &[i16; 8], cb: &[i16; 8], cr: &[i16; 8], output: &mut [u8], pos: &mut usize)
{
    let mut p = 0;

    //Okay Rust sucks with this bound's checking.
    // To get it to vectorize this  we need the below lines of code

    let (_, output_position) = output.split_at_mut(*pos);

    // Convert into a slice with 24 elements for Rust to FINALLY SEE WE WON'T GO OUT
    // OF BOUNDS
    let opt: &mut [u8; 24] = output_position
        .get_mut(0..24)
        .expect("Slice to small cannot write")
        .try_into()
        .unwrap();

    for (y, (cb, cr)) in y.iter().zip(cb.iter().zip(cr.iter()))
    {
        let cr = cr - 128;

        let cb = cb - 128;

        let r = y + ((45 * cr) >> 5);

        let g = y - ((11 * cb + 23 * cr) >> 5);

        let b = y + ((113 * cb) >> 6);

        opt[p] = clamp(r);

        opt[p + 1] = clamp(g);

        opt[p + 2] = clamp(b);

        p += 3;
    }

    // Increment pos
    *pos += 24;
}

pub fn ycbcr_to_rgba_scalar(y: &[i16; 8], cb: &[i16; 8], cr: &[i16; 8], output: &mut [u8], pos: &mut usize)
{
    let mut p = 0;

    //Okay Rust sucks with this bound's checking.
    // To get it to vectorize this  we need the below lines of code

    let (_, output_position) = output.split_at_mut(*pos);

    // Convert into a slice with 32 elements for Rust to FINALLY SEE WE WON'T GO OUT
    // OF BOUNDS
    let opt: &mut [u8; 32] = output_position
        .get_mut(0..32)
        .expect("Slice to small cannot write")
        .try_into()
        .unwrap();

    for (y, (cb, cr)) in y.iter().zip(cb.iter().zip(cr.iter()))
    {
        let cr = cr - 128;

        let cb = cb - 128;

        let r = y + ((45 * cr) >> 5);

        let g = y - ((11 * cb + 23 * cr) >> 5);

        let b = y + ((113 * cb) >> 6);

        opt[p] = clamp(r);

        opt[p + 1] = clamp(g);

        opt[p + 2] = clamp(b);

        opt[p + 3] = 255;

        p += 4;
    }

    // Increment pos
    *pos += 32;
}

/// YcbCr to RGBA color conversion

pub fn ycbcr_to_rgba_16_scalar(
    y: &[i16; 16], cb: &[i16; 16], cr: &[i16; 16], output: &mut [u8], pos: &mut usize,
)
{
    // first mcu
    ycbcr_to_rgba_scalar(
        y[0..8].try_into().unwrap(),
        cb[0..8].try_into().unwrap(),
        cr[0..8].try_into().unwrap(),
        output,
        pos,
    );

    // second MCU
    ycbcr_to_rgba_scalar(
        y[8..16].try_into().unwrap(),
        cb[8..16].try_into().unwrap(),
        cr[8..16].try_into().unwrap(),
        output,
        pos,
    );
}

pub fn ycbcr_to_rgb_16_scalar(
    y: &[i16; 16], cb: &[i16; 16], cr: &[i16; 16], output: &mut [u8], pos: &mut usize,
)
{
    // first mcu
    ycbcr_to_rgb_scalar(
        y[0..8].try_into().unwrap(),
        cb[0..8].try_into().unwrap(),
        cr[0..8].try_into().unwrap(),
        output,
        pos,
    );

    // second MCU
    ycbcr_to_rgb_scalar(
        y[8..16].try_into().unwrap(),
        cb[8..16].try_into().unwrap(),
        cr[8..16].try_into().unwrap(),
        output,
        pos,
    );
}

/// This function determines the best color-convert function to carry out
/// based on the colorspace needed

pub fn choose_ycbcr_to_rgb_convert_func(
    type_need: ColorSpace,
) -> Option<(ColorConvert16Ptr, ColorConvertPtr)>
{
    #[cfg(feature = "x86")]
    {
        if is_x86_feature_detected!("avx2")
        {
            debug!("Using AVX optimised color conversion functions");

            // I believe avx2 means sse4 is also available
            // match colorspace
            return match type_need
            {
                ColorSpace::RGB =>
                {
                    // Avx for 16, sse for 8
                    Some((ycbcr_to_rgb_avx2, ycbcr_to_rgb_sse))
                }
                ColorSpace::RGBA =>
                {
                    // Avx for 16, sse for 8
                    Some((ycbcr_to_rgba_avx2, ycbcr_to_rgba_sse))
                }
                ColorSpace::RGBX => Some((ycbcr_to_rgbx_avx2, ycbcr_to_rgba_sse)),

                _ => None,
            };
        }
        // try sse
        else if is_x86_feature_detected!("sse4.1")
        {
            // I believe avx2 means sse4 is also available
            // match colorspace
            debug!("No support for avx2 switching to sse");

            debug!("Using sse color convert functions");

            return match type_need
            {
                ColorSpace::RGB => Some((ycbcr_to_rgb_sse_16, ycbcr_to_rgb_sse)),
                ColorSpace::RGBA | ColorSpace::RGBX =>
                {
                    Some((ycbcr_to_rgba_sse_16, ycbcr_to_rgba_sse))
                }

                //ColorSpace::GRAYSCALE => Some((ycbcr_to_grayscale_16_sse, ycbcr_to_grayscale_8)),
                _ => None,
            };
        }
    }

    // when there is no x86 or we haven't returned by here, resort to lookup tables
    return match type_need
    {
        ColorSpace::RGB => Some((ycbcr_to_rgb_16_scalar, ycbcr_to_rgb_scalar)),
        ColorSpace::RGBA | ColorSpace::RGBX => Some((ycbcr_to_rgba_16_scalar, ycbcr_to_rgba_scalar)),
        //    ColorSpace::GRAYSCALE => Some((ycbcr_to_grayscale_16, ycbcr_to_grayscale_8)),
        _ => None,
    };
}
