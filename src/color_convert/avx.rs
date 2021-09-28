//! AVX color conversion routines
#![cfg(feature = "x86")]
#![allow(
    clippy::wildcard_imports,
    clippy::cast_possible_truncation,
    clippy::too_many_arguments,
    clippy::inline_always,
    clippy::doc_markdown
)]

#[cfg(target_arch = "x86")]
use std::arch::x86::*;
#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::*;

pub union YmmRegister
{
    // both are 32 when using std::mem::size_of
    mm256: __m256i,
    // for avx color conversion
    array: [i16; 16],
}

//--------------------------------------------------------------------------------------------------
// AVX conversion routines
//--------------------------------------------------------------------------------------------------
///
/// Convert YCBCR to RGB using AVX instructions
///
///  # Note
///**IT IS THE RESPONSIBILITY OF THE CALLER TO CALL THIS IN CPUS SUPPORTING
/// AVX2 OTHERWISE THIS IS UB**
///
/// *Peace*
///
/// This library itself will ensure that it's never called in CPU's not
/// supporting AVX2
///
/// # Arguments
/// - `y`,`cb`,`cr`: A reference of 8 i32's
/// - `out`: The output  array where we store our converted items
/// - `offset`: The position from 0 where we write these RGB values
#[inline(always)]

pub fn ycbcr_to_rgb_avx2(
    y1: &[i16; 8],
    y2: &[i16; 8],
    cb1: &[i16; 8],
    cb2: &[i16; 8],
    cr1: &[i16; 8],
    cr2: &[i16; 8],
    out: &mut [u8],
    offset: &mut usize,
)
{
    // call this in another function to tell RUST to vectorize this
    // storing
    unsafe {
        ycbcr_to_rgb_avx2_1(y1, y2, cb1, cb2, cr1, cr2, out, offset);
    }
}

#[inline]
#[target_feature(enable = "avx2")]
#[target_feature(enable = "avx")]

unsafe fn ycbcr_to_rgb_avx2_1(
    y1: &[i16; 8],
    y2: &[i16; 8],
    cb1: &[i16; 8],
    cb2: &[i16; 8],
    cr1: &[i16; 8],
    cr2: &[i16; 8],
    out: &mut [u8],
    offset: &mut usize,
)
{
    let (r, g, b) = ycbcr_to_rgb_baseline(y1, y2, cb1, cb2, cr1, cr2);

    // This is badly vectorised in AVX2,
    // With it extracting values from ymm to xmm registers
    // Hence it might be a tad slower than sse(9 more instructions)
    for i in 0..16
    {
        // Reason
        //  - Bounds checking will prevent autovectorization of this
        // Safety
        // - Array is pre initialized and the way this is called ensures
        // it will never go out of bounds
        *out.get_unchecked_mut(*offset) = r.array[i] as u8;

        *out.get_unchecked_mut(*offset + 1) = g.array[i] as u8;

        *out.get_unchecked_mut(*offset + 2) = b.array[i] as u8;

        *offset += 3;
    }
}

/// Baseline implementation of YCBCR to RGB for avx,
///
/// It uses integer operations as opposed to floats, the approximation is
/// difficult for the  eye to see, but this means that it may produce different
/// values with libjpeg_turbo.  if accuracy is of utmost importance, use that.
///
/// this function should be called for most implementations, including
/// - ycbcr->rgb
/// - ycbcr->rgba
/// - ycbcr->brga
/// - ycbcr->rgbx
#[inline]
#[target_feature(enable = "avx2")]
#[target_feature(enable = "avx")]

pub unsafe fn ycbcr_to_rgb_baseline(
    y1: &[i16; 8],
    y2: &[i16; 8],
    cb1: &[i16; 8],
    cb2: &[i16; 8],
    cr1: &[i16; 8],
    cr2: &[i16; 8],
) -> (YmmRegister, YmmRegister, YmmRegister)
{
    // Load values into a register
    //
    // dst[127:0] := MEM[loaddr+127:loaddr]
    // dst[255:128] := MEM[hiaddr+127:hiaddr]
    let y_c = _mm256_loadu2_m128i(y2.as_ptr().cast(), y1.as_ptr().cast());

    let cb_c = _mm256_loadu2_m128i(cb2.as_ptr().cast(), cb1.as_ptr().cast());

    let cr_c = _mm256_loadu2_m128i(cr2.as_ptr().cast(), cr1.as_ptr().cast());

    // AVX version of integer version in https://stackoverflow.com/questions/4041840/function-to-convert-ycbcr-to-rgb

    // Cb = Cb-128;
    let cb_r = _mm256_sub_epi16(cb_c, _mm256_set1_epi16(128));

    // cr = Cb -128;
    let cr_r = _mm256_sub_epi16(cr_c, _mm256_set1_epi16(128));

    // Calculate Y->R
    // r = Y + 45 * Cr / 32
    // 45*cr
    let r1 = _mm256_mullo_epi16(_mm256_set1_epi16(45), cr_r);

    // r1>>5
    let r2 = _mm256_srai_epi16::<5>(r1);

    //y+r2

    let r = YmmRegister {
        mm256: clamp_avx(_mm256_add_epi16(y_c, r2)),
    };

    // g = Y - (11 * Cb + 23 * Cr) / 32 ;

    // 11*cb
    let g1 = _mm256_mullo_epi16(_mm256_set1_epi16(11), cb_r);

    // 23*cr
    let g2 = _mm256_mullo_epi16(_mm256_set1_epi16(23), cr_r);

    //(11
    //(11 * Cb + 23 * Cr)
    let g3 = _mm256_add_epi16(g1, g2);

    // (11 * Cb + 23 * Cr) / 32
    let g4 = _mm256_srai_epi16::<5>(g3);

    // Y - (11 * Cb + 23 * Cr) / 32 ;
    let g = YmmRegister {
        mm256: clamp_avx(_mm256_sub_epi16(y_c, g4)),
    };

    // b = Y + 113 * Cb / 64
    // 113 * cb
    let b1 = _mm256_mullo_epi16(_mm256_set1_epi16(113), cb_r);

    //113 * Cb / 64
    let b2 = _mm256_srai_epi16::<6>(b1);

    // b = Y + 113 * Cb / 64 ;
    let b = YmmRegister {
        mm256: clamp_avx(_mm256_add_epi16(b2, y_c)),
    };

    return (r, g, b);
}

#[inline]
#[target_feature(enable = "avx2")]
/// A baseline implementation of YCbCr to RGB conversion which does not carry
/// out clamping
///
/// This is used by the `ycbcr_to_rgba_avx` and `ycbcr_to_rgbx` conversion
/// routines

pub unsafe fn ycbcr_to_rgb_baseline_no_clamp(
    y1: &[i16; 8],
    y2: &[i16; 8],
    cb1: &[i16; 8],
    cb2: &[i16; 8],
    cr1: &[i16; 8],
    cr2: &[i16; 8],
) -> (__m256i, __m256i, __m256i)
{
    // Load values into a register
    //
    // dst[127:0] := MEM[loaddr+127:loaddr]
    // dst[255:128] := MEM[hiaddr+127:hiaddr]
    let y_c = _mm256_loadu2_m128i(y2.as_ptr().cast(), y1.as_ptr().cast());

    let cb_c = _mm256_loadu2_m128i(cb2.as_ptr().cast(), cb1.as_ptr().cast());

    let cr_c = _mm256_loadu2_m128i(cr2.as_ptr().cast(), cr1.as_ptr().cast());

    // AVX version of integer version in https://stackoverflow.com/questions/4041840/function-to-convert-ycbcr-to-rgb

    // Cb = Cb-128;
    let cb_r = _mm256_sub_epi16(cb_c, _mm256_set1_epi16(128));

    // cr = Cb -128;
    let cr_r = _mm256_sub_epi16(cr_c, _mm256_set1_epi16(128));

    // Calculate Y->R
    // r = Y + 45 * Cr / 32
    // 45*cr
    let r1 = _mm256_mullo_epi16(_mm256_set1_epi16(45), cr_r);

    // r1>>5
    let r2 = _mm256_srai_epi16::<5>(r1);

    //y+r2

    let r = _mm256_add_epi16(y_c, r2);

    // g = Y - (11 * Cb + 23 * Cr) / 32 ;

    // 11*cb
    let g1 = _mm256_mullo_epi16(_mm256_set1_epi16(11), cb_r);

    // 23*cr
    let g2 = _mm256_mullo_epi16(_mm256_set1_epi16(23), cr_r);

    //(11
    //(11 * Cb + 23 * Cr)
    let g3 = _mm256_add_epi16(g1, g2);

    // (11 * Cb + 23 * Cr) / 32
    let g4 = _mm256_srai_epi16::<5>(g3);

    // Y - (11 * Cb + 23 * Cr) / 32 ;
    let g = _mm256_sub_epi16(y_c, g4);

    // b = Y + 113 * Cb / 64
    // 113 * cb
    let b1 = _mm256_mullo_epi16(_mm256_set1_epi16(113), cb_r);

    //113 * Cb / 64
    let b2 = _mm256_srai_epi16::<6>(b1);

    // b = Y + 113 * Cb / 64 ;
    let b = _mm256_add_epi16(b2, y_c);

    return (r, g, b);
}

#[inline(always)]

pub fn ycbcr_to_rgba_avx2(
    y1: &[i16; 8],
    y2: &[i16; 8],
    cb1: &[i16; 8],
    cb2: &[i16; 8],
    cr1: &[i16; 8],
    cr2: &[i16; 8],
    out: &mut [u8],
    offset: &mut usize,
)
{
    unsafe { ycbcr_to_rgba_unsafe(y1, y2, cb1, cb2, cr1, cr2, out, offset) }
}

#[inline]
#[target_feature(enable = "avx2")]

pub unsafe fn ycbcr_to_rgba_unsafe(
    y1: &[i16; 8],
    y2: &[i16; 8],
    cb1: &[i16; 8],
    cb2: &[i16; 8],
    cr1: &[i16; 8],
    cr2: &[i16; 8],
    out: &mut [u8],
    offset: &mut usize,
)
{
    let (r, g, b) = ycbcr_to_rgb_baseline_no_clamp(y1, y2, cb1, cb2, cr1, cr2);

    // set alpha channel to 255 for opaque

    // And no these comments were not from me pressing the keyboard

    // Pack the integers into u8's using signed saturation.
    let c = _mm256_packus_epi16(r, g); //aaaaa_bbbbb_aaaaa_bbbbbb
    let d = _mm256_packus_epi16(b, _mm256_set1_epi16(255)); // cccccc_dddddd_ccccccc_ddddd
                                                            // transpose and interleave channels
    let e = _mm256_unpacklo_epi8(c, d); //ab_ab_ab_ab_ab_ab_ab_ab
    let f = _mm256_unpackhi_epi8(c, d); //cd_cd_cd_cd_cd_cd_cd_cd
                                        // final transpose
    let g = _mm256_unpacklo_epi8(e, f); //abcd_abcd_abcd_abcd_abcd
    let h = _mm256_unpackhi_epi8(e, f);

    // undo packus shuffling...
    let i = _mm256_permute2x128_si256::<{ shuffle(3, 2, 1, 0) }>(g, h);

    let j = _mm256_permute2x128_si256::<{ shuffle(1, 2, 3, 0) }>(g, h);

    let k = _mm256_permute2x128_si256::<{ shuffle(3, 2, 0, 1) }>(g, h);

    let l = _mm256_permute2x128_si256::<{ shuffle(0, 3, 2, 1) }>(g, h);

    let m = _mm256_blend_epi32::<0b1111_0000>(i, j);

    let n = _mm256_blend_epi32::<0b1111_0000>(k, l);

    // Store
    // Use streaming instructions to prevent polluting the cache
    _mm256_storeu_si256(out.as_mut_ptr().add(*offset).cast(), m);

    _mm256_storeu_si256(out.as_mut_ptr().add(*offset + 32).cast(), n);

    *offset += 64;
}

/// YCbCr to RGBX conversion
///
/// The X in RGBX stands for `anything`, the compiler will make X anything it
/// sees fit, although most implementations use
///
/// This is meant to match libjpeg-turbo RGBX conversion and since its
/// a 4 way interleave instead of a three way interleave, the code is simple
/// to vectorize hence this is faster than YcbCr -> RGB conversion
#[inline(always)]

pub fn ycbcr_to_rgbx_avx2(
    y1: &[i16; 8],
    y2: &[i16; 8],
    cb1: &[i16; 8],
    cb2: &[i16; 8],
    cr1: &[i16; 8],
    cr2: &[i16; 8],
    out: &mut [u8],
    offset: &mut usize,
)
{
    unsafe { ycbcr_to_rgbx_unsafe(y1, y2, cb1, cb2, cr1, cr2, out, offset) }
}

#[inline]
#[allow(clippy::cast_possible_wrap)]
#[target_feature(enable = "avx2")]

pub unsafe fn ycbcr_to_rgbx_unsafe(
    y1: &[i16; 8],
    y2: &[i16; 8],
    cb1: &[i16; 8],
    cb2: &[i16; 8],
    cr1: &[i16; 8],
    cr2: &[i16; 8],
    out: &mut [u8],
    offset: &mut usize,
)
{
    let (r, g, b) = ycbcr_to_rgb_baseline_no_clamp(y1, y2, cb1, cb2, cr1, cr2);

    // Pack the integers into u8's using signed saturation.
    let c = _mm256_packus_epi16(r, g); //aaaaa_bbbbb_aaaaa_bbbbbb
                                       // Set alpha channel to random things, Mostly I see it using the b values
    let d = _mm256_packus_epi16(b, _mm256_undefined_si256()); // cccccc_dddddd_ccccccc_ddddd
                                                              // transpose and interleave channels
    let e = _mm256_unpacklo_epi8(c, d); //ab_ab_ab_ab_ab_ab_ab_ab
    let f = _mm256_unpackhi_epi8(c, d); //cd_cd_cd_cd_cd_cd_cd_cd
                                        // final transpose
    let g = _mm256_unpacklo_epi8(e, f); //abcd_abcd_abcd_abcd_abcd
    let h = _mm256_unpackhi_epi8(e, f);

    // undo packus shuffling...
    // This only applies to AVX because it's packus is a bit weird
    // and PLEASE DO NOT CHANGE SHUFFLE CONSTANTS.
    // COMING UP WITH THEM TOOK SOO LONG...
    let i = _mm256_permute2x128_si256::<{ shuffle(3, 2, 1, 0) }>(g, h);

    let j = _mm256_permute2x128_si256::<{ shuffle(1, 2, 3, 0) }>(g, h);

    let k = _mm256_permute2x128_si256::<{ shuffle(3, 2, 0, 1) }>(g, h);

    let l = _mm256_permute2x128_si256::<{ shuffle(0, 3, 2, 1) }>(g, h);

    let m = _mm256_blend_epi32::<0b1111_0000>(i, j);

    let n = _mm256_blend_epi32::<0b1111_0000>(k, l);

    // Store
    // Use streaming instructions to prevent polluting the cache
    _mm256_storeu_si256(out.as_mut_ptr().add(*offset).cast(), m);

    _mm256_storeu_si256(out.as_mut_ptr().add(*offset + 32).cast(), n);

    *offset += 64;
}

/// Clamp values between 0 and 255
///
/// This function clamps all values in `reg` to be between 0 and 255
///( the accepted values for RGB)
#[inline]
#[target_feature(enable = "avx2")]
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]

unsafe fn clamp_avx(reg: __m256i) -> __m256i
{
    // the lowest value
    let min_s = _mm256_set1_epi16(0);

    // Highest value
    let max_s = _mm256_set1_epi16(255);

    let max_v = _mm256_max_epi16(reg, min_s); //max(a,0)
    let min_v = _mm256_min_epi16(max_v, max_s); //min(max(a,0),255)
    return min_v;
}

#[inline]

const fn shuffle(z: i32, y: i32, x: i32, w: i32) -> i32
{
    ((z << 6) | (y << 4) | (x << 2) | w) as i32
}
