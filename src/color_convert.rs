#![allow(clippy::many_single_char_names, clippy::similar_names)]
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

use crate::misc::{Aligned16};

/// Limit values to 0 and 255
///
/// This is the Simple fallback implementation and should work
/// on all architectures without SSE support, its slower than SSE(
/// even though it has no branches since, but since it works per element)
#[inline]
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, dead_code)]
fn clamp(a: i32) -> u8 {
    min(max(a, 0), 255) as u8
}

/// Build a lookup table
const fn build_ycbr_rgb_table() -> ([i32; 256], [i32; 256], [i32; 256], [i32; 256]) {
    let mut cr_r: [i32; 256] = [0; 256];

    let mut cb_g: [i32; 256] = [0; 256];
    let mut cr_g: [i32; 256] = [0; 256];

    let mut cb_b: [i32; 256] = [0; 256];

    let mut i = 0;
    while i < 255 {
        // Rust does not allow fp calculations inside const functions so we use
        // integer
        //  This implementation is borrowed from wikipedia
        cr_r[i] = 45 * ((i as i32) - 128) >> 5;

        cb_g[i] = 11 * ((i as i32) - 128);
        cr_g[i] = 23 * ((i as i32) - 128);

        cb_b[i] = 113 * ((i as i32) - 128) >> 6;
        i += 1;
    }
    (cr_r, cb_g, cr_g, cb_b)
}

const ALL_TABLES: ([i32; 256], [i32; 256], [i32; 256], [i32; 256]) = build_ycbr_rgb_table();
const CR_R: [i32; 256] = ALL_TABLES.0;

const CB_G: [i32; 256] = ALL_TABLES.1;
const CR_G: [i32; 256] = ALL_TABLES.2;

const CB_B: [i32; 256] = ALL_TABLES.3;

/// Faster version of YcbCr to RGB colorspace conversion
///  which uses lookup tables and optionally an SSE clamper that clamps
///  values faster than the naive one
/// # Arguments
/// - y:  `&[i32;8]` - 8 values for the Y color space
/// - cb: `&[i32:8]` - 8 values for the Cb color space
/// - cr: `&[i32;8]` - 8 values for the Cr color space
/// - output: `&[i32;8]` - Where we will write our results
/// - offset : `usize` - Which position in the output should
///  we write values
/// # Performance
/// - We may suffer cache miss penalties ( due to competition for cache space
///   with other parts) when loading the tables  but since its called repeatedly, the
///  miss will be small for subsequent calls
/// - On platforms with SSE2 (x86,x86_64) we can clamp values a little faster than the naive clamping since
/// we can clamp 3 values at once
#[cfg(feature = "perf")]
#[inline(always)]
pub fn ycbcr_to_rgb(y: &[i32], cb: &[i32], cr: &[i32], output: &mut [u8], offset: usize) {
    let mut pos = offset;
    for i in 0..8 {
        unsafe {
            // SAFETY: The last function (IDCT/upsample ensures values are between 0..255)
            // SAFETY: y,cb,cr methods are called with array of 8 slices
            let r = y.get_unchecked(i) + CR_R.get_unchecked(*cr.get_unchecked(i) as usize);
            let g = y.get_unchecked(i)
                - ((CB_G.get_unchecked(*cb.get_unchecked(i) as usize)
                + CR_G.get_unchecked(*cr.get_unchecked(i) as usize))
                >> 5);
            let b = y.get_unchecked(i) + (CB_B.get_unchecked(*cb.get_unchecked(i) as usize));
            #[cfg(all(
            target_feature = "sse2",
            any(target_arch = "x86", target_arch = "x86_64")
            ))]
                {
                    // clamp using SSE(if available)
                    let mut p = Aligned16([r, g, b, 0]);
                    clamp_sse(&mut p);
                    // SAFETY, the array is pre-initialized
                    *output.get_unchecked_mut(pos) = p.0[0] as u8;
                    *output.get_unchecked_mut(pos + 1) = p.0[1] as u8;
                    *output.get_unchecked_mut(pos + 2) = p.0[2] as u8;
                }
            // If we lack SSE support, we can use the normal clamp
            #[cfg(not(all(
            target_feature = "sse2",
            any(target_arch = "x86", target_arch = "x86_64")
            )))]
                {
                    *output.get_unchecked_mut(pos) = clamp(r);
                    *output.get_unchecked_mut(pos + 1) = clamp(g);
                    *output.get_unchecked_mut(pos + 2) = clamp(b);
                }
            pos += 3;
        }
    }
}

/// Safe (and slower) version of YCbCr to RGB conversion
///
/// # Performance
/// - We still use lookup tables but we bounds-check(even though we know
/// it can never panic_)
/// - We use a slow version of clamping, that is possible of clamping 1 value at a time
#[cfg(not(feature = "perf"))]
pub fn ycbcr_to_rgb(y: &[i32], cb: &[i32], cr: &[i32], output: &mut [u8], pos: usize) {
    let mut pos = pos;
    for (y, (cb, cr)) in y.iter().zip(cb.iter().zip(cr.iter())) {
        let r = y + CR_R[*cr as usize];
        let g = y - ((CB_G[*cb as usize] + CR_G[*cr as usize]) >> 5);
        let b = y + (CB_B[*cb as usize]);
        output[pos] = clamp(r);
        output[pos + 1] = clamp(g);
        output[pos + 2] = clamp(b);
        pos += 3;
    }
}

/// Clamp using SSE
///
/// This shelves off about 16 instructions per conversion.
///
/// # Arguments
/// a: A mutable reference to a memory location containing
/// 4 i32's aligned to a 16 byte boundary.
///
/// The data is modified in place
///
#[cfg(feature = "perf")]
#[target_feature(enable = "sse2")]
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
pub unsafe fn clamp_sse(a: &mut Aligned16<[i32; 4]>) {
    #[cfg(target_arch = "x86")]
    use std::arch::x86::*;
    #[cfg(target_arch = "x86_64")]
    use std::arch::x86_64::*;

    let p = _mm_load_si128(a.0.as_ptr() as *const _);
    // the lowest value
    let min: __m128i = _mm_set1_epi32(0);
    // Highest value
    let max: __m128i = _mm_set1_epi32(255);
    // epi16 works better here than epi32
    let max_v = _mm_max_epi16(p, min); //max(a,0)
    let min_v = _mm_min_epi16(max_v, max); //min(max(a,0),255)
    // Store it back to the array
    _mm_store_si128(a.0.as_mut_ptr() as *mut _, min_v);
}

union YmmRegister {
    // both are 32 when using std::mem::size_of
    array: [i32; 8],
    mm256: __m256i,
}

/// Convert YCBCR to RGB using AVX instructions
///
///  # Note
///**IT IS THE RESPONSIBILITY OF THE CALLER TO CALL THIS IN CPUS SUPPORTING AVX2
/// OTHERWISE THIS IS UB**
///
/// *Peace*
///
/// This library itself will ensure that it's never called in CPU's not supporting AVX2
///
/// # Arguments
/// - `y`,`cb`,`cr`: A reference of 8 i32's
/// - `out`: The output  array where we store our converted items
/// - `offset`: The position from 0 where we write these RGB values
#[cfg(feature = "perf")]
pub fn ycbcr_to_rgb_avx2(y: &[i32], cb: &[i32], cr: &[i32], out: &mut [u8], offset: usize) {
    // we can't make the function unsafe and use target feature
    // because the signature won't match the other functions
    unsafe {
        // load data into memory
        // for contemporary processors , unaligned loads should not be slower than aligned loads
        // when using avx2
        let y_c = _mm256_loadu_si256(y.as_ptr() as *const _);
        let cb_c = _mm256_loadu_si256(cb.as_ptr() as *const _);
        let cr_c = _mm256_loadu_si256(cr.as_ptr() as *const _);

        // AVX version of integer version in https://stackoverflow.com/questions/4041840/function-to-convert-ycbcr-to-rgb

        // Cb = Cb-128;
        let cb_r = _mm256_sub_epi32(cb_c, _mm256_set1_epi32(128));
        // cr = Cb -128;
        let cr_r = _mm256_sub_epi32(cr_c, _mm256_set1_epi32(128));

        // Calculate Y->R
        // r = Y + 45 * Cr / 32
        // 45*cr
        let r1 = _mm256_mullo_epi32(_mm256_set1_epi32(45), cr_r);
        // r1>>5
        let r2 = _mm256_srai_epi32(r1, 5);
        //y+r2

        let r = YmmRegister { mm256: clamp_avx(_mm256_add_epi32(y_c, r2)) };
