//! Up-sampling routines
//!
//! The main upsampling method is a bi-linear interpolation or a "triangle  filter " or libjpeg turbo
//! `fancy_upsampling` which is a good compromise between speed and visual quality
//!
#[cfg(feature = "x86")]
pub use sse::upsample_horizontal_sse;

mod sse;

/// Carry out vertical   upsampling
pub fn upsample_vertical(_input: &[i16], _output_len: usize) -> Vec<i16> {
    return Vec::new();
}

/// Upsample horizontally
///
/// The up-sampling algorithm used is libjpeg-turbo `fancy_upsampling` which is a linear
/// interpolation or triangle filter, see module docs for explanation
pub fn upsample_horizontal(input: &[i16], output_len: usize) -> Vec<i16> {
    let mut out = vec![0; output_len];

    out[0] = input[0];
    out[1] = (input[0] * 3 + input[1] + 2) >> 2;
    let input_len = input.len();
    // duplicate, number of MCU's are 2
    for i in 1..input_len - 1 {
        let sample = 3 * input[i] + 2;
        out[i * 2] = (sample + input[i - 1]) >> 2;
        out[i * 2 + 1] = (sample + input[i + 1]) >> 2;
    }
    // write last mcu
    out[(input_len - 1) * 2] = (input[input_len - 2] * 3 + input[input_len - 1] + 2) >> 2;
    out[(input_len - 1) * 2 + 1] = input[input_len - 1];
    return out;
}

pub fn upsample_horizontal_vertical(_input: &[i16], _output_len: usize) -> Vec<i16> {
    return Vec::new();
}

/// Upsample nothing
pub fn upsample_no_op(_: &[i16], _: usize) -> Vec<i16> {
    return Vec::new();
}
