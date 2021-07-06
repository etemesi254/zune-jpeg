#![allow(
clippy::excessive_precision,
clippy::unreadable_literal,
clippy::module_name_repetitions,
)]

const SCALE_BITS: i32 = 512 + 65536 + (128 << 17);
/// Perform Integer IDCT
/// and level shift (by adding 128 to each element)
/// This is a modified version of one in [`stbi_image.h`]
///
/// # Arguments
///  - vector: A mutable reference( so that i can reuse memory) to a MCU worth of numbers
///  - qt_table: A quantization table fro the MCU
///
/// [`stbi_image.h`]:https://github.com/nothings/stb/blob/c9064e317699d2e495f36ba4f9ac037e88ee371a/stb_image.h#L2356
#[inline(always)]
#[allow(arithmetic_overflow)]
pub fn dequantize_and_idct_int(vector: &mut [i32; 64], qt_table: &[i32; 64]) {
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
        let p2 = dequantize(vector[ptr + 16], qt_table[ptr + 16]);
        let p3 = dequantize(vector[ptr + 48], qt_table[ptr + 48]);

        let p1 = (p2 + p3) * 2217;

        let t2 = p1 + p3 * -7567;
        let t3 = p1 + p2 * 3135;

        let p2 = dequantize(vector[ptr], qt_table[ptr]);
        let p3 = dequantize(vector[32 + ptr], qt_table[32 + ptr]);

        let t0 = fsh(p2 + p3);
        let t1 = fsh(p2 - p3);

        let x0 = t0 + t3 + 512;
        let x3 = t0 - t3 + 512;
        let x1 = t1 + t2 + 512;
        let x2 = t1 - t2 + 512;

        // odd part
        let mut t0 = dequantize(vector[ptr + 56], qt_table[ptr + 56]);
        let mut t1 = dequantize(vector[ptr + 40], qt_table[ptr + 40]);

        let mut t2 = dequantize(vector[ptr + 24], qt_table[ptr + 24]);
        let mut t3 = dequantize(vector[ptr + 8], qt_table[ptr + 8]);

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

        let p1 = p5.wrapping_add(p1.wrapping_mul(-3865));
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
        vector[i + 6] = clamp((x1 + t2) >> 17);
        vector[i + 7] = clamp((x0 - t3) >> 17);

        i += 8;
    }
}

#[inline]
/// Multiply a number by 4096
fn f2f(x: f32) -> i32 {
    (x * 4096.0 + 0.5) as i32
}

#[inline(always)]
/// Multiply a number by 4096
fn fsh(x: i32) -> i32 {
    x << 12
}

#[inline]
fn clamp(a: i32) -> i32 {
    a.max(0).min(255)
}

#[inline]
fn dequantize(a: i32, b: i32) -> i32 {
    a * b
}

//--------------------------------------------------
// Testing code
#[test]
fn test_dequantize_and_idct_block_8x8_int_all_zero() {
    // Test
    let mut output = [0; 64];
    dequantize_and_idct_int(&mut output, &[1; 64]);
    assert_eq!(&output[..], &[128; 64]);
}


