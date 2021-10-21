#![cfg(feature = "x86")]

#[cfg(target_arch = "x86")]
use std::arch::x86::*;
#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::*;

use crate::misc::Aligned32;
use crate::unsafe_utils::{align_alloc, YmmRegister};

const SCALE_BITS: i32 = 512 + 65536 + (128 << 17);

/// SAFETY
/// ------
///
/// It is the responsibility of the CALLER to ensure that  this function is
/// called in contexts where the CPU supports it
///
/// # Performance
/// - This implementation contains 100 less instructions than
///   `dequantize_and_idct_int`( with the same
///  opt levels and parameters) while being transparent.
/// - The bottleneck becomes memory loads and stores, which we can't sadly force
///   to be faster

pub fn dequantize_and_idct_avx2(vector: &[i16], qt_table: &Aligned32<[i32; 64]>, stride: usize, samp_factors: usize) -> Vec<i16>
{
    unsafe {
        // We don't call this method directly because we need to flag the code function
        // with #[target_feature] so that the compiler does do weird stuff with
        // it
        dequantize_and_idct_int_avx2(vector, qt_table, stride, samp_factors)
    }
}

#[target_feature(enable = "avx2")]
#[allow(
clippy::too_many_lines,
clippy::cast_possible_truncation,
clippy::similar_names,
unused_assignments
)]
unsafe fn dequantize_and_idct_int_avx2(coeff: &[i16], qt_table: &Aligned32<[i32; 64]>, stride: usize, samp_factors: usize) -> Vec<i16>
{
    let mut tmp_vector = align_alloc::<i16, 16>(coeff.len());

    // calculate position
    // inside This is still slow because cache misses
    let qt_row0 = _mm256_load_si256(qt_table.0[0..=7].as_ptr().cast());

    let qt_row1 = _mm256_load_si256(qt_table.0[8..=15].as_ptr().cast());

    let qt_row2 = _mm256_load_si256(qt_table.0[16..=23].as_ptr().cast());

    let qt_row3 = _mm256_load_si256(qt_table.0[24..=32].as_ptr().cast());

    let qt_row4 = _mm256_load_si256(qt_table.0[32..=39].as_ptr().cast());

    let qt_row5 = _mm256_load_si256(qt_table.0[40..=47].as_ptr().cast());

    let qt_row6 = _mm256_load_si256(qt_table.0[48..=55].as_ptr().cast());

    let qt_row7 = _mm256_load_si256(qt_table.0[56..=63].as_ptr().cast());

    let chunks = coeff.len() / samp_factors;

    // calculate position
    for (in_vector, out_vector) in coeff.chunks_exact(chunks).zip(tmp_vector.chunks_exact_mut(chunks))
    {
        let mut pos = 0;
        let mut x = 0;
        // Iterate in chunks of 64

        for vector in in_vector.chunks_exact(64)
        {
            // load into registers
            //
            // We sign extend i16's to i32's and calculate them with extended precision and
            // later reduce them to i16's when we are done carrying out IDCT

            let rw0 = _mm_load_si128(vector[0..=7].as_ptr().cast());

            let rw1 = _mm_load_si128(vector[8..=15].as_ptr().cast());

            let rw2 = _mm_load_si128(vector[16..=23].as_ptr().cast());

            let rw3 = _mm_load_si128(vector[24..=31].as_ptr().cast());

            let rw4 = _mm_load_si128(vector[32..=39].as_ptr().cast());

            let rw5 = _mm_load_si128(vector[40..=47].as_ptr().cast());

            let rw6 = _mm_load_si128(vector[48..=55].as_ptr().cast());

            let rw7 = _mm_loadu_si128(vector[56..=63].as_ptr().cast());

            {
                // Forward DCT and quantization may cause all the AC terms to be zero, for such
                // cases we can try to accelerate it

                // Basically the poop is that whenever the array has 63 zeroes, its idct is
                // (arr[0]>>3)or (arr[0]/8) propagated to all the elements.
                // We first test to see if the array contains zero elements and if it does, we go the
                // short way.
                //
                // This reduces IDCT overhead from about 39% to 18 %, almost half

                // Do another load for the first row, we don't want to check DC value, because
                // we only care about AC terms
                let tmp_load = _mm_loadu_si128(vector[1..8].as_ptr().cast());

                // To test for zeroes, we use bitwise OR,operations, a| a => 0 if a is zero, so
                // if all items are zero, the resulting value of X should be zero
                let mut zero_test = _mm_or_si128(tmp_load, tmp_load);

                zero_test = _mm_or_si128(rw1, zero_test);

                zero_test = _mm_or_si128(rw2, zero_test);

                zero_test = _mm_or_si128(rw3, zero_test);

                zero_test = _mm_or_si128(rw4, zero_test);

                zero_test = _mm_or_si128(rw5, zero_test);

                zero_test = _mm_or_si128(rw6, zero_test);

                zero_test = _mm_or_si128(rw7, zero_test);

                //compare with ourselves, if the value of v  is 1 all AC terms are zero for
                // this block
                let v = _mm_testz_si128(zero_test, zero_test);

                if v == 1
                {
                    // AC terms all zero, idct of the block is  is (coeff[0] *qt[0])/8 + bias(128)
                    // (and clamped to 255)
                    let idct_value = _mm_set1_epi16(
                        (((vector[0] * qt_table.0[0] as i16) >> 3) + 128)
                            .max(0)
                            .min(255),
                    );
                    macro_rules! store {
                    ($pos:tt,$value:tt) => {
                    // store
                    _mm_store_si128(out_vector.get_unchecked_mut($pos..$pos+8).as_mut_ptr().cast(),$value);
                //    println!("{}",$pos);
                    $pos+=stride;
                    };
                }
                    store!(pos,idct_value);
                    store!(pos,idct_value);
                    store!(pos,idct_value);
                    store!(pos,idct_value);

                    store!(pos,idct_value);
                    store!(pos,idct_value);
                    store!(pos,idct_value);
                    store!(pos,idct_value);

                    x += 8;
                    pos = x;


                    // reset pos

                    // Go to the next coefficient block
                    continue;
                }
            }

            let mut row0 = YmmRegister {
                mm256: _mm256_cvtepi16_epi32(rw0),

            };

            let mut row1 = YmmRegister {
                mm256: _mm256_cvtepi16_epi32(rw1),
            };

            let mut row2 = YmmRegister {
                mm256: _mm256_cvtepi16_epi32(rw2),
            };

            let mut row3 = YmmRegister {
                mm256: _mm256_cvtepi16_epi32(rw3),
            };

            let mut row4 = YmmRegister {
                mm256: _mm256_cvtepi16_epi32(rw4),
            };

            let mut row5 = YmmRegister {
                mm256: _mm256_cvtepi16_epi32(rw5),
            };

            let mut row6 = YmmRegister {
                mm256: _mm256_cvtepi16_epi32(rw6),
            };

            let mut row7 = YmmRegister {
                mm256: _mm256_cvtepi16_epi32(rw7),
            };

            // multiply with qt tables
            row0 *= qt_row0;

            row1 *= qt_row1;

            row2 *= qt_row2;

            row3 *= qt_row3;

            row4 *= qt_row4;

            row5 *= qt_row5;

            row6 *= qt_row6;

            row7 *= qt_row7;

            macro_rules! dct_pass {
            ($SCALE_BITS:tt,$scale:tt) => {
                // There are a lot of ways to do this
                // but to keep it simple(and beautiful), ill make a direct translation of the
                // above to also make this code fully transparent(this version and the non
                // avx one should produce identical code.)

                // even part
                let p1 = (row2 + row6) * 2217;

                let mut t2 = p1 + row6 * -7567;

                let mut t3 = p1 + row2 * 3135;

                let mut t0 = YmmRegister {
                    mm256: _mm256_slli_epi32((row0 + row4).mm256, 12),
                };

                let mut t1 = YmmRegister {
                    mm256: _mm256_slli_epi32((row0 - row4).mm256, 12),
                };

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

            transpose(
                &mut row0, &mut row1, &mut row2, &mut row3, &mut row4, &mut row5, &mut row6, &mut row7,
            );

            // Process rows
            dct_pass!(512, 10);

            transpose(
                &mut row0, &mut row1, &mut row2, &mut row3, &mut row4, &mut row5, &mut row6, &mut row7,
            );

            // process columns
            dct_pass!(SCALE_BITS, 17);

            // transpose to original

            // Pack i32 to i16's,
            // clamp them to be between 0-255
            // Undo shuffling
            // Store back to array
            macro_rules! permute_store {
            ($x:tt,$y:tt,$index:tt,$out:tt) => {
                let a = _mm256_packs_epi32($x, $y);

                // Clamp the values after packing, 3535we can clamp more values at once
                let b = clamp_avx(a);

                // /Undo shuffling
                let c = _mm256_permute4x64_epi64(b, shuffle(3, 1, 2, 0));

                // store first vector
                 _mm_store_si128(($out).get_unchecked_mut($index..$index+8).as_mut_ptr().cast(), _mm256_extractf128_si256::<0>(c));
                 $index += stride;

                // second vector
                 _mm_store_si128(($out).get_unchecked_mut($index..$index+8).as_mut_ptr().cast(),_mm256_extractf128_si256::<1>(c));
                $index += stride;
                
            };
        }

            // Pack and write the values back to the array
            permute_store!((row0.mm256), (row1.mm256), pos, out_vector);
            permute_store!((row2.mm256), (row3.mm256), pos, out_vector);
            permute_store!((row4.mm256), (row5.mm256), pos, out_vector);
            permute_store!((row6.mm256), (row7.mm256), pos, out_vector);

            x += 8;
            pos = x;
        }
    }
    return tmp_vector;
}

#[inline]
#[target_feature(enable = "avx2")]
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

type Reg = YmmRegister;

/// Transpose an array of 8 by 8 i32's using avx intrinsics
///
/// This was translated from [here](https://newbedev.com/transpose-an-8x8-float-using-avx-avx2)
#[allow(unused_parens, clippy::too_many_arguments)]
#[target_feature(enable = "avx2")]
unsafe fn transpose(
    v0: &mut Reg, v1: &mut Reg, v2: &mut Reg, v3: &mut Reg, v4: &mut Reg, v5: &mut Reg,
    v6: &mut Reg, v7: &mut Reg,
)
{
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
const fn shuffle(z: i32, y: i32, x: i32, w: i32) -> i32
{
    ((z << 6) | (y << 4) | (x << 2) | w) as i32
}
