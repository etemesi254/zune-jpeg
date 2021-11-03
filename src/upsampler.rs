//! Up-sampling routines
//!
//! The main upsampling method is a bi-linear interpolation or a "triangle
//! filter " or libjpeg turbo `fancy_upsampling` which is a good compromise
//! between speed and visual quality
//!
//! # The filter
//! Each output pixel is made from `(3*A+B)/4` where A is the original
//! pixel closer to the output and B is the one further.
//!
//! ```text
//!+---+---+
//! | A | B |
//! +---+---+
//! +-+-+-+-+
//! | |P| | |
//! +-+-+-+-+
//! ```

#[cfg(feature = "x86")]
pub use sse::upsample_horizontal_sse;

use crate::components::UpSampler;

mod sse;

mod scalar;
// choose best possible implementation for this platform
pub fn choose_horizontal_samp_function() -> UpSampler
{
    #[cfg(all(feature = "x86", any(target_arch = "x86_64", target_arch = "x86")))]
    {
        if is_x86_feature_detected!("sse4.1")
        {
            return sse::upsample_horizontal_sse;
        }
    }
    return scalar::upsample_horizontal;
}
/// Carry out vertical   upsampling

pub fn upsample_vertical(input: &[i16], output_len: usize) -> Vec<i16>
{
    // what we know.
    // We have 8 rows of data and we need to make it 16 rows;
    let mut out = vec![0; output_len];
    let inp_row = input.len() >> 4;
    // so we chunk output row wise
    for (position, row_chunk) in out.chunks_exact_mut(output_len >> 4).enumerate()
    {
        // iterate over each row
        row_chunk.iter_mut().enumerate().for_each(|(pos, x)| {
            let row_far = {
                if position % 2 == 0
                {
                    *input.get(inp_row * (position + 1) + pos).unwrap_or(&0)
                }
                else
                {
                    *input.get(inp_row * (position - 1) + pos).unwrap_or(&0)
                }
            };
            let row_near = *input.get(pos).unwrap_or(&0);

            *x = (3 * row_near + row_far + 2) >> 2;
        });
    }
    //println!("{:?}",out);
    return out;
}

pub fn upsample_horizontal_vertical(_input: &[i16], _output_len: usize) -> Vec<i16>
{
    return Vec::new();
}

/// Upsample nothing

pub fn upsample_no_op(_: &[i16], _: usize) -> Vec<i16>
{
    return Vec::new();
}

//---------------------------------------------
// TEST
//----------------------------------------------
#[test]
fn upsample_sse_v1()
{

    let v: Vec<i16> = (0..128).collect();

    assert_eq!(
        upsample_horizontal_sse(&v, v.len() * 2),
        crate::upsampler::scalar::upsample_horizontal(&v, v.len() * 2),
        "Algorithms do not match"
    );
}
#[test]
fn upsample_sse_v2()
{
    use crate::upsampler::scalar::upsample_horizontal;

    let v: Vec<i16> = (0..1280).rev().collect();

    assert_eq!(
        upsample_horizontal_sse(&v, v.len() * 2),
        upsample_horizontal(&v, v.len() * 2),
        "Algorithms do not match"
    );
}
