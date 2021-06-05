#![allow(
    clippy::excessive_precision,
    clippy::unreadable_literal,
    clippy::module_name_repetitions
)]
/*---- Tables of constants ----*/

use std::cmp::{max, min};
use std::convert::TryInto;

const A: [f32; 5] = [
    std::f32::consts::FRAC_1_SQRT_2,
    0.541196100146196984399723,
    std::f32::consts::FRAC_1_SQRT_2,
    1.306562964876376527856643,
    0.382683432365089771728460,
];
const S: [f32; 8] = [
    2.82842712474619,
    3.92314112161292,
    3.69551813004514,
    3.32587844921018,
    2.82842712474619,
    2.22228093207840,
    1.53073372946035,
    0.78036128806451,
];

/// The IDCT method to use when carrying out IDCT
#[derive(Copy, Clone)]
pub enum IDCTMethod {
    // Use Floating point IDCT
    Float,
    // Use integer IDCT
    Integer,
}
impl Default for IDCTMethod {
    fn default() -> Self {
        IDCTMethod::Integer
    }
}

/// Compute inverse IDCT of an  f64 array
///
/// # Arguments
///  - vector: An array of 64 elements
///
/// # Returns
/// An array of u8's level shifted after IDCT
pub fn idct_fl(vector: &mut [f32; 64]) -> [i16; 64] {
    let mut i = [0; 64];
    // process rows
    let mut ptr = 0;
    // while is better, avoids bounds check below

    while ptr < 64 {
        // A straightforward inverse of the forward algorithm
        let v15 = vector[ptr] * S[0];
        let v26 = vector[ptr + 1] * S[1];
        let v21 = vector[ptr + 2] * S[2];
        let v28 = vector[ptr + 3] * S[3];
        let v16 = vector[ptr + 4] * S[4];
        let v25 = vector[ptr + 5] * S[5];
        let v22 = vector[ptr + 6] * S[6];
        let v27 = vector[ptr + 7] * S[7];

        let v19 = (v25 - v28) * 0.5;
        let v20 = (v26 - v27) * 0.5;
        let v23 = (v26 + v27) * 0.5;
        let v24 = (v25 + v28) * 0.5;

        let v7 = (v23 + v24) * 0.5;
        let v11 = (v21 + v22) * 0.5;
        let v13 = (v23 - v24) * 0.5;
        let v17 = (v21 - v22) * 0.5;

        let v8 = (v15 + v16) * 0.5;
        let v9 = (v15 - v16) * 0.5;

        let v18 = (v19 - v20) * A[4]; // Different from original
        let v12 = (v19 * A[3] - v18) * -1.0;
        let v14 = (v18 - v20 * A[1]) * -1.0;

        let v6 = v14 - v7;
        let v5 = v13 / A[2] - v6;
        let v4 = -v5 - v12;
        let v10 = v17 / A[0] - v11;

        let v0 = (v8 + v11) * 0.5;
        let v1 = (v9 + v10) * 0.5;
        let v2 = (v9 - v10) * 0.5;
        let v3 = (v8 - v11) * 0.5;

        vector[ptr] = (v0 + v7) * 0.5;
        vector[ptr + 1] = (v1 + v6) * 0.5;
        vector[ptr + 2] = (v2 + v5) * 0.5;
        vector[ptr + 3] = (v3 + v4) * 0.5;
        vector[ptr + 4] = (v3 - v4) * 0.5;
        vector[ptr + 5] = (v2 - v5) * 0.5;
        vector[ptr + 6] = (v1 - v6) * 0.5;
        vector[ptr + 7] = (v0 - v7) * 0.5;
        ptr += 8;
    }
    // process columns
    for ptr in 0..8 {
        // A straightforward inverse of the forward algorithm
        let v15 = vector[ptr] * S[0];
        let v26 = vector[ptr + 8] * S[1];
        let v21 = vector[ptr + 16] * S[2];
        let v28 = vector[ptr + 24] * S[3];
        let v16 = vector[ptr + 32] * S[4];
        let v25 = vector[ptr + 40] * S[5];
        let v22 = vector[ptr + 48] * S[6];
        let v27 = vector[ptr + 56] * S[7];

        let v19 = (v25 - v28) * 0.5;
        let v20 = (v26 - v27) * 0.5;
        let v23 = (v26 + v27) * 0.5;
        let v24 = (v25 + v28) * 0.5;

        let v7 = (v23 + v24) * 0.5;
        let v11 = (v21 + v22) * 0.5;
        let v13 = (v23 - v24) * 0.5;
        let v17 = (v21 - v22) * 0.5;

        let v8 = (v15 + v16) * 0.5;
        let v9 = (v15 - v16) * 0.5;

        let v18 = (v19 - v20) * A[4]; // Different from original
        let v12 = (v19 * A[3] - v18) * -1.0;
        let v14 = (v18 - v20 * A[1]) * -1.0;

        let v6 = v14 - v7;
        let v5 = v13 / A[2] - v6;
        let v4 = -v5 - v12;
        let v10 = v17 / A[0] - v11;

        let v0 = (v8 + v11) * 0.5;
        let v1 = (v9 + v10) * 0.5;
        let v2 = (v9 - v10) * 0.5;
        let v3 = (v8 - v11) * 0.5;

        i[ptr] = level_shift((v0 + v7) * 0.5);
        i[ptr + 8] = level_shift((v1 + v6) * 0.5);
        i[ptr + 16] = level_shift((v2 + v5) * 0.5);
        i[ptr + 24] = level_shift((v3 + v4) * 0.5);
        i[ptr + 32] = level_shift((v3 - v4) * 0.5);
        i[ptr + 40] = level_shift((v2 - v5) * 0.5);
        i[ptr + 48] = level_shift((v1 - v6) * 0.5);
        i[ptr + 56] = level_shift((v0 - v7) * 0.5);
    }
    i
}
#[inline]
fn level_shift(x: f32) -> i16 {
    let mut p = x as i32;
    p += 128;
    // clamp to 0 and 255

    min(max(p, 0), 255) as i16
}

const SCALE_BITS: i32 = 512 + 65536 + (128 << 17);
/// Perform Integer IDCT
/// and level shift (by adding 128 to each element)
/// This is a modified version of one in [`stbi_image.h`]
///
/// # Warning
/// This implementation only works for values between 0 and 255 it is not meant to calculate IDCT of any array
/// For that use the above fl implementation
///
/// [`stbi_image.h`]:https://github.com/nothings/stb/blob/c9064e317699d2e495f36ba4f9ac037e88ee371a/stb_image.h#L2356
pub fn idct_int(vector: &mut [i32; 64]) -> [i16; 64] {
    let mut x = [0; 64];
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
        let p2 = vector[ptr + 16];
        let p3 = vector[ptr + 48];

        let p1 = (p2 + p3) * 2217;

        let t2 = p1 + p3 * -7567;
        let t3 = p1 + p2 * 3135;

        let p2 = vector[ptr];
        let p3 = vector[32 + ptr];

        let t0 = fsh(p2 + p3);
        let t1 = fsh(p2 - p3);

        let x0 = t0 + t3 + 512;
        let x3 = t0 - t3 + 512;
        let x1 = t1 + t2 + 512;
        let x2 = t1 - t2 + 512;

        // odd part
        let mut t0 = vector[ptr + 56];
        let mut t1 = vector[ptr + 40];

        let mut t2 = vector[ptr + 24];
        let mut t3 = vector[ptr + 8];

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
        vector[ptr] = (x0 + t3) >> 10;
        vector[ptr + 8] = (x1 + t2) >> 10;
        vector[ptr + 16] = (x2 + t1) >> 10;
        vector[ptr + 24] = (x3 + t0) >> 10;
        vector[ptr + 32] = (x3 - t0) >> 10;
        vector[ptr + 40] = (x2 - t1) >> 10;
        vector[ptr + 48] = (x1 - t2) >> 10;
        vector[ptr + 56] = (x0 - t3) >> 10;
    }

    // This is vectorised in architectures supporting SSE 4.1
    while i < 64 {
        // We won't try to short circuit here because it rarely works

        // Even part
        let p2 = vector[i + 2];
        let p3 = vector[i + 6];

        let p1 = (p2 + p3) * 2217;

        let t2 = p1 + p3 * -7567;
        let t3 = p1 + p2 * 3135;

        let p2 = vector[i];
        let p3 = vector[i + 4];

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
        let mut t0 = vector[i + 7];
        let mut t1 = vector[i + 5];

        let mut t2 = vector[i + 3];
        let mut t3 = vector[i + 1];

        let p3 = t0 + t2;
        let p4 = t1 + t3;
        let p1 = t0 + t3;
        let p2 = t1 + t2;
        let p5 = (p3 + p4) * f2f(1.175875602);

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

        // store store in i32

        x[i] = clamp((x0 + t3) >> 17);
        x[i + 1] = clamp((x1 + t2) >> 17);
        x[i + 2] = clamp((x2 + t1) >> 17);
        x[i + 3] = clamp((x3 + t0) >> 17);
        x[i + 4] = clamp((x3 - t0) >> 17);
        x[i + 5] = clamp((x2 - t1) >> 17);
        x[i + 6] = clamp((x1 + t2) >> 17);
        x[i + 7] = clamp((x0 - t3) >> 17);

        i += 8;
    }
    x
}

#[inline]
/// Multiply a number by 4096
fn f2f(x: f32) -> i32 {
    (x * 4096.0 + 0.5) as i32
}
#[inline]
/// Multiply a number by 4096
const fn fsh(x: i32) -> i32 {
    x * 4096
}
#[inline]
fn clamp(a: i32) -> i16 {
    a.max(0).min(255) as i16
}

//--------------------------------------------------
// Testing code
#[test]
fn test_dequantize_and_idct_block_8x8_int_all_zero() {
    // Test
    let mut output = idct_int(&mut [0; 64]);
    assert_eq!(&output[..], &[128; 64]);
}

#[test]
fn test_dequantize_and_idct_block_8x8_f32_all_zero() {
    let mut output = idct_fl(&mut [0.0; 64]);
    assert_eq!(&output[..], &[128; 64]);
}
#[test]
fn test_dequantize_and_idct_fl_block() {
    let results = [
        255, 65, 169, 110, 149, 123, 139, 131, 0, 180, 89, 142, 108, 130, 117, 124, 199, 106, 144,
        122, 136, 127, 132, 129, 69, 145, 115, 132, 121, 128, 124, 127, 160, 118, 135, 125, 131,
        128, 130, 128, 102, 135, 123, 130, 125, 128, 127, 128, 139, 124, 130, 127, 129, 128, 128,
        128, 122, 129, 127, 128, 128, 128, 128, 128,
    ];
    let mut coeff: [f32; 64] = (0..64)
        .map(|a| a as f32)
        .collect::<Vec<f32>>()
        .try_into()
        .unwrap();
    assert_eq!(results, idct_fl(&mut coeff));
}
#[test]
fn test_dequantize_and_idct_int_block() {
    let results = [
        255, 65, 170, 109, 150, 123, 65, 132, 0, 180, 89, 143, 107, 131, 180, 123, 199, 105, 144,
        121, 137, 127, 105, 130, 68, 145, 115, 133, 121, 129, 145, 126, 161, 117, 135, 125, 132,
        127, 117, 129, 102, 135, 122, 130, 125, 128, 135, 127, 139, 124, 131, 127, 129, 128, 124,
        128, 122, 129, 127, 128, 127, 128, 129, 128,
    ];
    let mut coeff: [i32; 64] = (0..64).collect::<Vec<i32>>().try_into().unwrap();
    assert_eq!(results, idct_int(&mut coeff));
}
