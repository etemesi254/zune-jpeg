#![allow(
    clippy::many_single_char_names,
    clippy::similar_names,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    clippy::too_many_arguments,
clippy::doc_markdown
)]

//! YUV to RGB Conversion
//!
//! Conversion equation can be implemented as
//! ```text
//! R = Y + 1.40200 * Cr
//! G = Y - 0.34414 * Cb - 0.71414 * Cr
//! B = Y + 1.77200 * Cb
//! ```
//!
//!
use std::cmp::{max, min};

#[cfg(feature = "x86")]
pub use crate::color_convert::avx::{ycbcr_to_rgb_avx2, ycbcr_to_rgba, ycbcr_to_rgbx};
#[cfg(feature = "x86")]
pub use crate::color_convert::sse::{ycbcr_to_rgb_sse,ycbcr_to_rgb_sse_16,ycbcr_to_rgba_sse_16};

pub mod avx;
pub mod sse;

/// Limit values to 0 and 255
///
/// This is the Simple fallback implementation and should work
/// on all architectures without SSE support, its slower than SSE(
/// even though it has no branches since, but since it works per element)
#[inline]
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, dead_code)]
fn clamp(a: i16) -> u8 {
    min(max(a, 0), 255) as u8
}

/// Build a lookup table
const fn build_ycbcr_rgb_table() -> ([i32; 256], [i32; 256], [i32; 256], [i32; 256]) {
    let mut cr_r: [i32; 256] = [0; 256];

    let mut cb_g: [i32; 256] = [0; 256];
    let mut cr_g: [i32; 256] = [0; 256];

    let mut cb_b: [i32; 256] = [0; 256];

    let mut i = 0;
    while i < 255 {
        // Rust does not allow fp calculations inside const functions so we use
        // integer
        //  This implementation is borrowed from wikipedia
        cr_r[i] = (45 * ((i as i32) - 128)) >> 5;

        cb_g[i] = 11 * ((i as i32) - 128);
        cr_g[i] = 23 * ((i as i32) - 128);

        cb_b[i] = (113 * ((i as i32) - 128)) >> 6;
        i += 1;
    }
    (cr_r, cb_g, cr_g, cb_b)
}

const ALL_TABLES: ([i32; 256], [i32; 256], [i32; 256], [i32; 256]) = build_ycbcr_rgb_table();
const CR_R: [i32; 256] = ALL_TABLES.0;

const CB_G: [i32; 256] = ALL_TABLES.1;
const CR_G: [i32; 256] = ALL_TABLES.2;

const CB_B: [i32; 256] = ALL_TABLES.3;

/// Safe (and slower) version of YCbCr to RGB conversion
///
/// # Performance
/// - We still use lookup tables but we bounds-check(even though we know
/// it can never panic_)
/// - We use a slow version of clamping, that is possible of clamping 1 value at a time
pub fn ycbcr_to_rgb(y: &[i16], cb: &[i16], cr: &[i16], output: &mut [u8], pos: &mut usize) {
    let pos = pos;
    for (y, (cb, cr)) in y.iter().zip(cb.iter().zip(cr.iter())) {
        let r = y + CR_R[*cr as usize] as i16;
        let g = y - ((CB_G[*cb as usize] + CR_G[*cr as usize]) >> 5) as i16;
        let b = y + (CB_B[*cb as usize]) as i16;
        output[*pos] = clamp(r);
        output[*pos + 1] = clamp(g);
        output[*pos + 2] = clamp(b);
        *pos += 3;
    }
}

pub fn ycbcr_to_rgb_16(
    y: &[i16],
    y2: &[i16],
    cb: &[i16],
    cb2: &[i16],
    cr: &[i16],
    cr2: &[i16],
    output: &mut [u8],
    pos: &mut usize,
) {
    // first mcu
    for (y, (cb, cr)) in y.iter().zip(cb.iter().zip(cr.iter())) {
        let r = y + CR_R[*cr as usize] as i16;
        let g = y - ((CB_G[*cb as usize] + CR_G[*cr as usize]) >> 5) as i16;
        let b = y + (CB_B[*cb as usize]) as i16;
        output[*pos] = clamp(r);
        output[*pos + 1] = clamp(g);
        output[*pos + 2] = clamp(b);
        *pos += 3;
    }
    // Second MCU
    for (y, (cb, cr)) in y2.iter().zip(cb2.iter().zip(cr2.iter())) {
        let r = y + CR_R[*cr as usize] as i16;
        let g = y - ((CB_G[*cb as usize] + CR_G[*cr as usize]) >> 5) as i16;
        let b = y + (CB_B[*cb as usize]) as i16;
        output[*pos] = clamp(r);
        output[*pos + 1] = clamp(g);
        output[*pos + 2] = clamp(b);
        *pos += 3;
    }
}
