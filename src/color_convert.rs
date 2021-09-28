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
//! The `RGB` routines use an  integer approximation routine

use std::cmp::{max, min};
use std::convert::TryInto;

#[cfg(feature = "x86")]
pub use crate::color_convert::avx::{ycbcr_to_rgb_avx2, ycbcr_to_rgba_avx2, ycbcr_to_rgbx_avx2};
#[cfg(feature = "x86")]
pub use crate::color_convert::sse::{
    ycbcr_to_grayscale_16_sse, ycbcr_to_rgb_sse, ycbcr_to_rgb_sse_16, ycbcr_to_rgba_sse,
    ycbcr_to_rgba_sse_16,
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

pub fn ycbcr_to_rgb(y: &[i16; 8], cb: &[i16; 8], cr: &[i16; 8], output: &mut [u8], pos: &mut usize)
{
    let mut p = 0;

    //Okay Rust sucks with this bound's checking.
    // To get it to vectorize this  we need the below lines of code

    let (_, v) = output.split_at_mut(*pos);

    // Convert into a slice with 24 elements for Rust to FINALLY SEE WE WON'T GO OUT
    // OF BOUNDS
    let opt: &mut [u8; 24] = v
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

pub fn ycbcr_to_rgba(y: &[i16; 8], cb: &[i16; 8], cr: &[i16; 8], output: &mut [u8], pos: &mut usize)
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

pub fn ycbcr_to_rgba_16(
    y: &[i16; 8],
    y2: &[i16; 8],
    cb: &[i16; 8],
    cb2: &[i16; 8],
    cr: &[i16; 8],
    cr2: &[i16; 8],
    output: &mut [u8],
    pos: &mut usize,
)
{
    // first mcu
    ycbcr_to_rgba(y, cb, cr, output, pos);

    // second MCU
    ycbcr_to_rgba(y2, cb2, cr2, output, pos);
}

pub fn ycbcr_to_rgb_16(
    y: &[i16; 8],
    y2: &[i16; 8],
    cb: &[i16; 8],
    cb2: &[i16; 8],
    cr: &[i16; 8],
    cr2: &[i16; 8],
    output: &mut [u8],
    pos: &mut usize,
)
{
    // first mcu
    ycbcr_to_rgb(y, cb, cr, output, pos);

    // second MCU
    ycbcr_to_rgb(y2, cb2, cr2, output, pos);
}

/// Convert 2 blocks of ycbcr to grayscale as slow as possible
///
/// We just copy the `Y(Luminance)` channel as it looks like a grayscale
///
/// A faster implementation is found in `color_convert/sse`
/// # Performance
/// 1. Really slow-> Auto vectorization isn't the best thing...

pub fn ycbcr_to_grayscale_16(
    y1: &[i16; 8],
    y2: &[i16; 8],
    _: &[i16; 8],
    _: &[i16; 8],
    _: &[i16; 8],
    _: &[i16; 8],
    output: &mut [u8],
    pos: &mut usize,
)
{
    // copy fist block
    output[*pos..*pos + 8].copy_from_slice(&y1.iter().map(|x| *x as u8).collect::<Vec<u8>>());

    // second block
    output[*pos + 8..*pos + 16].copy_from_slice(&y2.iter().map(|x| *x as u8).collect::<Vec<u8>>());

    *pos += 16;
}

/// Convert a single block of YCbCr to GrayScale
/// This is still slow

pub fn ycbcr_to_grayscale_8(
    y: &[i16; 8],
    _: &[i16; 8],
    _: &[i16; 8],
    output: &mut [u8],
    pos: &mut usize,
)
{
    // write
    output[*pos + 8..*pos + 16].copy_from_slice(&y.iter().map(|x| *x as u8).collect::<Vec<u8>>());

    // increment
    *pos += 8;
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
                ColorSpace::GRAYSCALE => Some((ycbcr_to_grayscale_16_sse, ycbcr_to_grayscale_8)),
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

                ColorSpace::GRAYSCALE => Some((ycbcr_to_grayscale_16_sse, ycbcr_to_grayscale_8)),
                _ => None,
            };
        }
    }

    // when there is no x86 or we haven't returned by here, resort to lookup tables
    return match type_need
    {
        ColorSpace::RGB => Some((ycbcr_to_rgb_16, ycbcr_to_rgb)),
        ColorSpace::RGBA | ColorSpace::RGBX => Some((ycbcr_to_rgba_16, ycbcr_to_rgba)),
        ColorSpace::GRAYSCALE => Some((ycbcr_to_grayscale_16, ycbcr_to_grayscale_8)),
        _ => None,
    };
}
