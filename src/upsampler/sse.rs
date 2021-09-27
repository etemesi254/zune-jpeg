#![cfg(feature = "x86")]
#![allow(clippy::module_name_repetitions)]

#[cfg(target_arch = "x86")]
use std::arch::x86::*;
#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::{
    _mm_add_epi16, _mm_insert_epi16, _mm_set1_epi16, _mm_slli_epi16, _mm_srai_epi16,
    _mm_store_si128, _mm_undefined_si128,
};

use crate::unsafe_utils::align_zero_alloc;

pub fn upsample_horizontal_sse(input: &[i16], output_len: usize) -> Vec<i16> {
    unsafe { upsample_horizontal_sse_u(input, output_len) }
}

/// Upsample using SSE to improve speed
///
/// The sampling filter is bi-linear or triangle filter
#[target_feature(enable = "sse2")]
//Some things are weird...
#[target_feature(enable = "sse4.1")]
pub unsafe fn upsample_horizontal_sse_u(input: &[i16], output_len: usize) -> Vec<i16> {
    let mut out = align_zero_alloc::<i16, 32>(output_len);
    //println!("{}",output_len);
    // set first 8 pixels linearly
    // Assert that out has more than 8 elements and input has more than 4
    // Do this before otherwise Rust will bounds check all of these items like some paranoid
    // guy.
    assert!(out.len() > 8 && input.len() > 5);
    // We can do better here, since Rust loads input[y] twice yer it can store it in a register
    // but enough ugly code
    out[0] = input[0];
    out[1] = (input[0] * 3 + input[1] + 2) >> 2;
    out[2] = (input[1] * 3 + input[0] + 2) >> 2;
    out[3] = (input[1] * 3 + input[2] + 2) >> 2;
    out[4] = (input[2] * 3 + input[1] + 2) >> 2;
    out[5] = (input[2] * 3 + input[3] + 2) >> 2;
    out[6] = (input[3] * 3 + input[2] + 2) >> 2;
    out[7] = (input[3] * 3 + input[4] + 2) >> 2;

    // maths
    // The poop here is to calculate how many 8 items we can interleave without reading past the array
    // We can't do the last 8 using SSE, because we will read after, and neither can we read from the first 8 because
    // we'll underflow in the first array so those are handled differently
    let inl = input.len();
    // times we can iterate without going past array;

    // For the rest of the pixels use normal instructions
    // Process using SSE for as many times as we can
    for i in 1..(inl >> 2) - 1 {
        // Safety
        // 1. inl is divided by 8 , basically it tells us how many times we can divide the input into
        // 8 chunks
        // 2. We iterate to 8 chunks before the end of the array because that is the limit of SSE instructions,
        // the last 8 are done manually after this function.
        let i4_4 = *input.get_unchecked((i * 4) + 4);
        let i4_2 = *input.get_unchecked((i * 4) + 2);
        let i4_3 = *input.get_unchecked((i * 4) + 3);
        let i4_1 = *input.get_unchecked((i * 4) + 1);
        let i4 = *input.get_unchecked(i * 4);
        let i4_0 = *input.get_unchecked((i * 4) - 1);
        // again seriously Rust compiler, you choose the worst way to do things
        // sincerely
        // Manually insert values into register, because RUST chooses some crazily in efficient way
        // to do this

        //let nn = _mm_set_epi16(i4_4, i4_2, i4_3, i4_1,
        //                      i4_2, i4, i4_1, i4_0);
        //
        //let yn = _mm_set_epi16(i4_3, i4_3, i4_2, i4_2, i4_1,
        //                       i4_1, i4, i4);
        let mut nn = _mm_undefined_si128();
        let mut yn = _mm_undefined_si128();

        nn = _mm_insert_epi16::<7>(nn, i32::from(i4_4));
        yn = _mm_insert_epi16::<7>(yn, i32::from(i4_3));

        nn = _mm_insert_epi16::<6>(nn, i32::from(i4_2));
        yn = _mm_insert_epi16::<6>(yn, i32::from(i4_3));

        nn = _mm_insert_epi16::<5>(nn, i32::from(i4_3));
        yn = _mm_insert_epi16::<5>(yn, i32::from(i4_2));

        nn = _mm_insert_epi16::<4>(nn, i32::from(i4_1));
        yn = _mm_insert_epi16::<4>(yn, i32::from(i4_2));

        nn = _mm_insert_epi16::<3>(nn, i32::from(i4_2));
        yn = _mm_insert_epi16::<3>(yn, i32::from(i4_1));

        nn = _mm_insert_epi16::<2>(nn, i32::from(i4));
        yn = _mm_insert_epi16::<2>(yn, i32::from(i4_1));

        nn = _mm_insert_epi16::<1>(nn, i32::from(i4_1));
        yn = _mm_insert_epi16::<1>(yn, i32::from(i4));

        nn = _mm_insert_epi16::<0>(nn, i32::from(i4_0));
        yn = _mm_insert_epi16::<0>(yn, i32::from(i4));

        // a multiplication by 3 can be seen as a shift by 1 and add by itself, let's use that
        // to reduce latency

        // input[x]*3
        // Change multiplication by 3 to be a shift left by 1(multiplication by 2) and add, removes latency
        // arising from multiplication, but it seems RUST is straight up ignoring me and my optimization techniques
        // it has converted it to a multiplication  RUST WHY DON'T YOU TRUST ME...
        let an = _mm_add_epi16(_mm_slli_epi16::<1>(yn), yn);
        // hoping this favours ILP because they don't depend on each other?
        let bn = _mm_add_epi16(nn, _mm_set1_epi16(2));
        // (input[x]*3+input[y]+2)>>2;
        let cn = _mm_srai_epi16::<2>(_mm_add_epi16(an, bn));
        // write to array
        _mm_store_si128(out.as_mut_ptr().add(i * 8).cast(), cn);
    }
    // Do the last 8 manually because we can't  do it  with SSE because of boundary filters .
    let ol = output_len - 8;
    let il = input.len() - 4;

    out[ol] = (input[il] * 3 + input[il - 1] + 2) >> 2;
    out[ol + 1] = (input[il] * 3 + input[il + 1] + 2) >> 2;
    out[ol + 2] = (input[il + 1] * 3 + input[il] + 2) >> 2;
    out[ol + 3] = (input[il + 1] * 3 + input[il + 1] + 2) >> 2;
    out[ol + 4] = (input[il + 2] * 3 + input[il + 2] + 2) >> 2;
    out[ol + 5] = (input[il + 2] * 3 + input[il + 1] + 2) >> 2;
    out[ol + 6] = (input[il + 2] * 3 + input[il + 3] + 2) >> 2;
    out[ol + 7] = input[il + 3];
    return out;
}

#[test]
/// Ensure SSE and horizontal are identical bitwise for all inputs
fn upsample_sse_plain() {
    use crate::upsampler::upsample_horizontal;
    let v: Vec<i16> = (0..128).collect();
    assert_eq!(
        upsample_horizontal_sse(&v, v.len() * 2),
        upsample_horizontal(&v, v.len() * 2),
        "Algorithms do not match"
    );
}
