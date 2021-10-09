//! Routines for IDCT
#![allow(
    clippy::excessive_precision,
    clippy::unreadable_literal,
    clippy::module_name_repetitions,
    unused_parens,
    clippy::wildcard_imports
)]

#[cfg(feature = "x86")]
mod avx2;

#[cfg(feature = "X86")]
use crate::idct::avx2::dequantize_and_idct_avx2;

use crate::{misc::Aligned32, IDCTPtr};

const SCALE_BITS: i32 = 512 + 65536 + (128 << 17);

/// Perform Integer IDCT
/// and level shift (by adding 128 to each element)
/// This is a modified version of one in [`stbi_image.h`]
///
/// # Arguments
///  - vector: A mutable reference( so that i can reuse memory) to a MCU worth
///    of numbers
///  - `qt_table`: A quantization table fro the MCU
///
/// [`stbi_image.h`]:https://github.com/nothings/stb/blob/c9064e317699d2e495f36ba4f9ac037e88ee371a/stb_image.h#L2356
#[allow(arithmetic_overflow)]

pub fn dequantize_and_idct_int(vector: &mut [i16], qt_table: &Aligned32<[i32; 64]>)
{

    let mut tmp = [0; 64];

    for vector in vector.chunks_exact_mut(64)
    {

        let mut i = 0;

        // Putting this in a separate function makes it really bad
        // because the compiler fails to see that it can be auto_vectorised so i'll
        // leave it here check out [idct_int_slow, and idct_int_1D to get what i mean ] https://godbolt.org/z/8hqW9z9j9
        for ptr in 0..8
        {

            // Due to quantization, we may find that all AC elements are zero, the IDCT of
            // that column Becomes a (scaled) DCT coefficient

            // We could short-circuit
            // but it leads to the below part not being vectorised, which makes it REALLY
            // SLOW

            // and since it rarely is zero, I favour this, and for those cases where it's
            // zero, we'll survive even part
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
        while i < 64
        {

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

fn f2f(x: f32) -> i32
{

    (x * 4096.0 + 0.5) as i32
}

#[inline]
/// Multiply a number by 4096

fn fsh(x: i32) -> i32
{

    x << 12
}

/// Clamp values between 0 and 255
#[inline]
#[allow(clippy::cast_possible_truncation)]

fn clamp(a: i32) -> i16
{

    a.max(0).min(255) as i16
}

#[inline]

fn dequantize(a: i16, b: i32) -> i32
{

    i32::from(a) * b
}

/// Choose an appropriate IDCT function

pub fn choose_idct_func() -> IDCTPtr
{

    #[cfg(feature = "x86")]
    {

        if is_x86_feature_detected!("avx2")
        {

            // use avx one
            return crate::idct::avx2::dequantize_and_idct_avx2;
        }
    }

    // use generic one
    return dequantize_and_idct_int;
}
