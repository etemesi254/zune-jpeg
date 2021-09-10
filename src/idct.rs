//! Routines for IDCT
#![allow(
    clippy::excessive_precision,
    clippy::unreadable_literal,
    clippy::module_name_repetitions,
    unused_parens,
    clippy::wildcard_imports
)]

#[cfg(target_arch = "x86")]
use std::arch::x86::*;
#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::*;

use crate::misc::Aligned32;
#[cfg(feature = "x86")]
use crate::unsafe_utils::YmmRegister;

const SCALE_BITS: i32 = 512 + 65536 + (128 << 17);

/// Perform Integer IDCT
/// and level shift (by adding 128 to each element)
/// This is a modified version of one in [`stbi_image.h`]
///
/// # Arguments
///  - vector: A mutable reference( so that i can reuse memory) to a MCU worth of numbers
///  - `qt_table`: A quantization table fro the MCU
///
/// [`stbi_image.h`]:https://github.com/nothings/stb/blob/c9064e317699d2e495f36ba4f9ac037e88ee371a/stb_image.h#L2356
#[allow(arithmetic_overflow)]
pub fn dequantize_and_idct_int(vector: &mut [i16], qt_table: &Aligned32<[i32; 64]>) {
    let mut tmp = [0; 64];
    for vector in vector.chunks_exact_mut(64) {
        let mut i = 0;
        // Putting this in a separate function makes it really bad
        // because the compiler fails to see that it can be auto_vectorised so i'll leave it here
        // check out [idct_int_slow, and idct_int_1D to get what i mean ] https://godbolt.org/z/8hqW9z9j9
        for ptr in 0..8 {
            // Due to quantization, we may find that all AC elements are zero, the IDCT of that column
            // Becomes a (scaled) DCT coefficient

            // We could short-circuit
            // but it leads to the below part not being vectorised, which makes it REALLY SLOW

            // and since it rarely is zero, I favour this, and for those cases where it's zero, we'll survive
            // even part
            let p2 = dequantize(vector[ptr + 16], qt_table.0[ptr + 16]);
            let p3 = dequantize(vector[ptr + 48], qt_table.0[ptr + 48]);

            let p1 = (p2 + p3) * 2217;

            let t2 = p1 + p3 * -7567;
            let t3 = p1 + p2 * 3135;

            let p2 = dequantize(vector[ptr], qt_table.0[ptr]);
            let p3 = dequantize(vector[32 + ptr], qt_table.0[32 + ptr]);

            let t0 = fsh(p2 + p3);
            let t1 = fsh(p2 - p3);

            let x0 = t0 + t3 + 512;
            let x3 = t0 - t3 + 512;
            let x1 = t1 + t2 + 512;
            let x2 = t1 - t2 + 512;

            // odd part
            let mut t0 = dequantize(vector[ptr + 56], qt_table.0[ptr + 56]);
            let mut t1 = dequantize(vector[ptr + 40], qt_table.0[ptr + 40]);

            let mut t2 = dequantize(vector[ptr + 24], qt_table.0[ptr + 24]);
            let mut t3 = dequantize(vector[ptr + 8], qt_table.0[ptr + 8]);

            let p3 = t0 + t2;
            let p4 = t1 + t3;
            let p1 = t0 + t3;
            let p2 = t1 + t2;
            let p5 = (p3 + p4) * 4816;

            t0 *= 1223;
            t1 *= 8410;
            t2 *= 12586;
            t3 *= 6149;

            let p1 = p5 + p1 * -3685;
            let p2 = p5 + p2 * -10497;
            let p3 = p3 * -8034;
            let p4 = p4 * -1597;

            t3 += p1 + p4;
            t2 += p2 + p3;
            t1 += p2 + p4;
            t0 += p1 + p3;

            // constants scaled things up by 1<<12; let's bring them back
            // down, but keep 2 extra bits of precision
            tmp[ptr] = (x0 + t3) >> 10;
            tmp[ptr + 8] = (x1 + t2) >> 10;
            tmp[ptr + 16] = (x2 + t1) >> 10;
            tmp[ptr + 24] = (x3 + t0) >> 10;
            tmp[ptr + 32] = (x3 - t0) >> 10;
            tmp[ptr + 40] = (x2 - t1) >> 10;
            tmp[ptr + 48] = (x1 - t2) >> 10;
            tmp[ptr + 56] = (x0 - t3) >> 10;
        }
        // This is vectorised in architectures supporting SSE 4.1
        while i < 64 {
            // We won't try to short circuit here because it rarely works

            // Even part
            let p2 = tmp[i + 2];
            let p3 = tmp[i + 6];

            let p1 = (p2 + p3) * 2217;

            let t2 = p1 + p3 * -7567;
            let t3 = p1 + p2 * 3135;

            let p2 = tmp[i];
            let p3 = tmp[i + 4];

            let t0 = fsh(p2 + p3);
            let t1 = fsh(p2 - p3);

            // constants scaled things up by 1<<12, plus we had 1<<2 from first
            // loop, plus horizontal and vertical each scale by sqrt(8) so together
            // we've got an extra 1<<3, so 1<<17 total we need to remove.
            // so we want to round that, which means adding 0.5 * 1<<17,
            // aka 65536. Also, we'll end up with -128 to 127 that we want
            // to encode as 0..255 by adding 128, so we'll add that before the shift
            let x0 = t0 + t3 + SCALE_BITS;
            let x3 = t0 - t3 + SCALE_BITS;
            let x1 = t1 + t2 + SCALE_BITS;
            let x2 = t1 - t2 + SCALE_BITS;

            // odd part
            let mut t0 = tmp[i + 7];
            let mut t1 = tmp[i + 5];

            let mut t2 = tmp[i + 3];
            let mut t3 = tmp[i + 1];

            let p3 = t0 + t2;
            let p4 = t1 + t3;
            let p1 = t0 + t3;
            let p2 = t1 + t2;
            let p5 = (p3 + p4) * f2f(1.175875602);

            t0 *= 1223;
            t1 *= 8410;
            t2 *= 12586;
            t3 *= 6149;

            let p1 = p5.wrapping_add(p1.wrapping_mul(-3685));
            let p2 = p5.wrapping_add(p2.wrapping_mul(-10497));
            let p3 = p3.wrapping_mul(-8034);
            let p4 = p4.wrapping_mul(-1597);

            t3 += p1 + p4;
            t2 += p2 + p3;
            t1 += p2 + p4;
            t0 += p1 + p3;

            // store store in i32

            vector[i] = clamp((x0 + t3) >> 17);
            vector[i + 1] = clamp((x1 + t2) >> 17);
            vector[i + 2] = clamp((x2 + t1) >> 17);
            vector[i + 3] = clamp((x3 + t0) >> 17);
            vector[i + 4] = clamp((x3 - t0) >> 17);
            vector[i + 5] = clamp((x2 - t1) >> 17);
            vector[i + 6] = clamp((x1 - t2) >> 17);
            vector[i + 7] = clamp((x0 - t3) >> 17);

            i += 8;
        }
    }
}

#[inline]
#[allow(clippy::cast_possible_truncation)]
/// Multiply a number by 4096
fn f2f(x: f32) -> i32 {
    (x * 4096.0 + 0.5) as i32
}

#[inline]
/// Multiply a number by 4096
fn fsh(x: i32) -> i32 {
    x << 12
}

#[inline]
#[allow(clippy::cast_possible_truncation)]
fn clamp(a: i32) -> i16 {
    a.max(0).min(255) as i16
}

#[inline]
fn dequantize(a: i16, b: i32) -> i32 {
    i32::from(a) * b
}

/// SAFETY
/// ------
///
/// It is the responsibility of the CALLER to ensure that  this function is called in contexts where
/// the CPU supports it
///
/// # Performance
/// - This implementation contains 100 less instructions than `dequantize_and_idct_int`( with the same
///  opt levels and parameters) while being transparent.
/// - The bottleneck becomes memory loads and stores, which we can't sadly force to be faster
#[cfg(feature = "x86")]
pub fn dequantize_and_idct_avx2(vector: &mut [i16], qt_table: &Aligned32<[i32; 64]>) {
    unsafe {
        // We don't call this method directly because we need to flag the code function with #[target_feature]
        // so that the compiler does do weird stuff with it
        dequantize_and_idct_int_avx2(vector, qt_table);
    }
}

#[cfg(feature = "x86")]
#[target_feature(enable = "avx2")]
#[allow(clippy::too_many_lines, clippy::cast_possible_truncation)]
unsafe fn dequantize_and_idct_int_avx2(coeff: &mut [i16], qt_table: &Aligned32<[i32; 64]>) {
    // since QT tables are reused, we can lift them from the loop and multiply them inside
    // This is still slow because cache misses
    let qt_row0 = _mm256_load_si256(qt_table.0[0..=7].as_ptr().cast());
    let qt_row1 = _mm256_load_si256(qt_table.0[8..=15].as_ptr().cast());
    let qt_row2 = _mm256_load_si256(qt_table.0[16..=23].as_ptr().cast());
    let qt_row3 = _mm256_load_si256(qt_table.0[24..=32].as_ptr().cast());
    let qt_row4 = _mm256_load_si256(qt_table.0[32..=39].as_ptr().cast());
    let qt_row5 = _mm256_load_si256(qt_table.0[40..=47].as_ptr().cast());
    let qt_row6 = _mm256_load_si256(qt_table.0[48..=55].as_ptr().cast());
    let qt_row7 = _mm256_load_si256(qt_table.0[56..=63].as_ptr().cast());
    // Iterate in chunks of 64
    for vector in coeff.chunks_exact_mut(64) {
        // load into registers
        //
        // We sign extend i16's to i32's and calculate them with extended precision and later reduce
        // them to i16's when we are done carrying out IDCT

        let mut row0 = YmmRegister {
            mm256: _mm256_cvtepi16_epi32(_mm_load_si128(vector[0..=7].as_ptr().cast())),
        };
        let mut row1 = YmmRegister {
            mm256: _mm256_cvtepi16_epi32(_mm_load_si128(vector[8..=15].as_ptr().cast())),
        };
        let mut row2 = YmmRegister {
            mm256: _mm256_cvtepi16_epi32(_mm_load_si128(vector[16..=23].as_ptr().cast())),
        };
        let mut row3 = YmmRegister {
            mm256: _mm256_cvtepi16_epi32(_mm_load_si128(vector[24..=31].as_ptr().cast())),
        };
        let mut row4 = YmmRegister {
            mm256: _mm256_cvtepi16_epi32(_mm_load_si128(vector[32..=39].as_ptr().cast())),
        };
        let mut row5 = YmmRegister {
            mm256: _mm256_cvtepi16_epi32(_mm_load_si128(vector[40..=47].as_ptr().cast())),
        };
        let mut row6 = YmmRegister {
            mm256: _mm256_cvtepi16_epi32(_mm_load_si128(vector[48..=55].as_ptr().cast())),
        };
        let mut row7 = YmmRegister {
            mm256: _mm256_cvtepi16_epi32(_mm_loadu_si128(vector[56..=63].as_ptr().cast())),
        };
        macro_rules! dct_pass {
            ($SCALE_BITS:tt,$scale:tt) => {

                // There are a lot of ways to do this
                // but to keep it simple(and beautiful), ill make a direct translation of the above to also make
                // this code fully transparent(this version and the non avx one should produce
                // identical code.


                // even part
                let p1 = (row2 + row6) * 2217;

                let mut t2 = p1 + row6 * -7567;

                let mut t3 = p1 + row2 * 3135;

                let mut t0 = YmmRegister{mm256:_mm256_slli_epi32((row0 + row4).mm256,12)};

                let mut t1 = YmmRegister{mm256:_mm256_slli_epi32((row0 - row4).mm256,12)};

                let x0 = t0 + t3 + $SCALE_BITS;
                let x3 = t0 - t3 + $SCALE_BITS;
                let x1 = t1 + t2 + $SCALE_BITS;
                let x2 = t1 - t2 + $SCALE_BITS;

                let p3 = row7 + row3;
                let p4 = row5 + row1;
                let p1 = row7 + row1;
                let p2 = row5 + row3;
                let p5 = (p3 + p4) * 4816;

                t0 = row7 * 1223;
                t1 = row5 * 8410;
                t2 = row3 * 12586;
                t3 = row1 * 6149;

                let p1 = p5 + p1 * -3685;
                let p2 = p5 + (p2 * -10497);
                let p3 = p3 * -8034;
                let p4 = p4 * -1597;

                t3 += p1 + p4;
                t2 += p2 + p3;
                t1 += p2 + p4;
                t0 += p1 + p3;
                row0.mm256 = _mm256_srai_epi32((x0 + t3).mm256, $scale);
                row1.mm256 = _mm256_srai_epi32((x1 + t2).mm256, $scale);
                row2.mm256 = _mm256_srai_epi32((x2 + t1).mm256, $scale);
                row3.mm256 = _mm256_srai_epi32((x3 + t0).mm256, $scale);

                row4.mm256 = _mm256_srai_epi32((x3 - t0).mm256, $scale);
                row5.mm256 = _mm256_srai_epi32((x2 - t1).mm256, $scale);
                row6.mm256 = _mm256_srai_epi32((x1 - t2).mm256, $scale);
                row7.mm256 = _mm256_srai_epi32((x0 - t3).mm256, $scale);
            };
        }
        {
            // Forward DCT and quantization may cause all the AC terms to be zero, for such cases
            // we can try to accelerate it

            // Basically the poop is that whenever the array has 63 zeroes, its idct is
            // (arr[0]>>3)or (arr[0]/8) propagated to all the elements so we first test to see if the array
            // contains zero elements and

            // Do another load for the first row, we don't want to check DC value, because we
            // only care about AC terms
            let tmp_load = _mm256_cvtepi16_epi32(_mm_loadu_si128(vector[1..8].as_ptr().cast()));
            // To test for zeroes, we use bitwise OR,operations, a| a => 0 if a is zero, so if all items
            // are zero, the resulting value of X should be zero
            let mut x = _mm256_or_si256(tmp_load, tmp_load);
            x = _mm256_or_si256(row1.mm256, x);
            x = _mm256_or_si256(row2.mm256, x);
            x = _mm256_or_si256(row3.mm256, x);
            x = _mm256_or_si256(row4.mm256, x);
            x = _mm256_or_si256(row5.mm256, x);
            x = _mm256_or_si256(row6.mm256, x);
            x = _mm256_or_si256(row7.mm256, x);
            //compare with ourselves, if the value of v  is 1 all AC terms are zero for this block
            let v = _mm256_testz_si256(x, x);

            if v == 1 {
                // AC terms all zero, idct is DC term + bias (and clamped to 255)

                let x = _mm256_set1_epi16(
                    (((vector[0] * (qt_table.0[0]) as i16) >> 3) + 128)
                        .max(0)
                        .min(255),
                );
                // store
                _mm256_storeu_si256(vector[0..16].as_mut_ptr().cast(), x);
                _mm256_storeu_si256(vector[16..32].as_mut_ptr().cast(), x);
                _mm256_storeu_si256(vector[32..48].as_mut_ptr().cast(), x);
                _mm256_storeu_si256(vector[48..64].as_mut_ptr().cast(), x);
                // Go to the next coefficient block
                continue;
            }
        }
        // multiply with qt tables
        row0 *= qt_row0;
        row1 *= qt_row1;
        row2 *= qt_row2;
        row3 *= qt_row3;
        row4 *= qt_row4;
        row5 *= qt_row5;
        row6 *= qt_row6;
        row7 *= qt_row7;
        transpose(
            &mut row0, &mut row1, &mut row2, &mut row3, &mut row4, &mut row5, &mut row6, &mut row7,
        );
        // Process rows
        dct_pass!(512, 10);

        transpose(&mut row0, &mut row1, &mut row2, &mut row3, &mut row4, &mut row5, &mut row6, &mut row7, );

        // process columns
        dct_pass!(SCALE_BITS, 17);

        // transpose to original

        //okay begin cool stuff
        macro_rules! permute_store {
            ($x:tt,$y:tt,$index:tt,$out:tt) => {
                let a = _mm256_packs_epi32($x, $y);
                // Clamp the values after packing, we can clamp more values at once
                let b = clamp_avx(a);
                // /Undo shuffling,
                // Magic number 216 is what does it for us..
                let c = _mm256_permute4x64_epi64(b, shuffle(3, 1, 2, 0));
                // Store back,the memory is aligned to a 32 byte boundary
                _mm256_storeu_si256(($out)[$index..$index + 16].as_mut_ptr().cast(), c);
            };
        }
        // Pack and write the values back to the array
        // https://play.rust-lang.org/?version=stable&mode=debug&edition=2018&gist=0fca534094f6cb20c43eca8d33ef3891
        permute_store!((row0.mm256), (row1.mm256), 0, vector);
        permute_store!((row2.mm256), (row3.mm256), 16, vector);
        permute_store!((row4.mm256), (row5.mm256), 32, vector);
        permute_store!((row6.mm256), (row7.mm256), 48, vector);
    }
}

#[cfg(feature = "x86")]
#[inline]
#[target_feature(enable = "avx2")]
unsafe fn clamp_avx(reg: __m256i) -> __m256i {
    // the lowest value
    let min_s = _mm256_set1_epi16(0);
    // Highest value
    let max_s = _mm256_set1_epi16(255);
    let max_v = _mm256_max_epi16(reg, min_s); //max(a,0)
    let min_v = _mm256_min_epi16(max_v, max_s); //min(max(a,0),255)
    return min_v;
}

#[cfg(feature = "x86")]
type Reg = YmmRegister;

/// Transpose an array of 8 by 8 i32's using avx intrinsics
///
/// This was translated from [here](https://newbedev.com/transpose-an-8x8-float-using-avx-avx2)
///
#[cfg(feature = "x86")]
#[allow(unused_parens, clippy::too_many_arguments)]
#[target_feature(enable = "avx2")]
unsafe fn transpose(
    v0: &mut Reg,
    v1: &mut Reg,
    v2: &mut Reg,
    v3: &mut Reg,
    v4: &mut Reg,
    v5: &mut Reg,
    v6: &mut Reg,
    v7: &mut Reg,
) {
    macro_rules! merge_epi32 {
        ($v0:tt,$v1:tt,$v2:tt,$v3:tt) => {
            let va = _mm256_permute4x64_epi64($v0, shuffle(3, 1, 2, 0));
            let vb = _mm256_permute4x64_epi64($v1, shuffle(3, 1, 2, 0));
            $v2 = _mm256_unpacklo_epi32(va, vb);
            $v3 = _mm256_unpackhi_epi32(va, vb);
        };
    }
    macro_rules! merge_epi64 {
        ($v0:tt,$v1:tt,$v2:tt,$v3:tt) => {
            let va = _mm256_permute4x64_epi64($v0, shuffle(3, 1, 2, 0));
            let vb = _mm256_permute4x64_epi64($v1, shuffle(3, 1, 2, 0));
            $v2 = _mm256_unpacklo_epi64(va, vb);
            $v3 = _mm256_unpackhi_epi64(va, vb);
        };
    }
    macro_rules! merge_si128 {
        ($v0:tt,$v1:tt,$v2:tt,$v3:tt) => {
            $v2 = _mm256_permute2x128_si256($v0, $v1, shuffle(0, 2, 0, 0));
            $v3 = _mm256_permute2x128_si256($v0, $v1, shuffle(0, 3, 0, 1));
        };
    }
    let (w0, w1, w2, w3, w4, w5, w6, w7);
    merge_epi32!((v0.mm256), (v1.mm256), w0, w1);
    merge_epi32!((v2.mm256), (v3.mm256), w2, w3);
    merge_epi32!((v4.mm256), (v5.mm256), w4, w5);
    merge_epi32!((v6.mm256), (v7.mm256), w6, w7);
    let (x0, x1, x2, x3, x4, x5, x6, x7);
    merge_epi64!(w0, w2, x0, x1);
    merge_epi64!(w1, w3, x2, x3);
    merge_epi64!(w4, w6, x4, x5);
    merge_epi64!(w5, w7, x6, x7);

    merge_si128!(x0, x4, (v0.mm256), (v1.mm256));
    merge_si128!(x1, x5, (v2.mm256), (v3.mm256));
    merge_si128!(x2, x6, (v4.mm256), (v5.mm256));
    merge_si128!(x3, x7, (v6.mm256), (v7.mm256));
}

/// A copy of `_MM_SHUFFLE()` that doesn't require
/// a nightly compiler
#[inline]
#[cfg(feature = "x86")]
const fn shuffle(z: i32, y: i32, x: i32, w: i32) -> i32 {
    ((z << 6) | (y << 4) | (x << 2) | w) as i32
}
