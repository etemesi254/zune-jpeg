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
//!
//! # Horizontal Bi-linear filter
//! For a horizontal bi-linear it's trivial to implement,
//!
//! `A` becomes the input closest to the output.
//!
//! `B` varies depending on output.
//!  - For odd positions, input is the `next` pixel after A
//!  - For even positions, input is the `previous` value before A.
//!
//! We iterate in a classic 1-D sliding window with a window of 3.
//! For our sliding window approach, `A` is the 1st and `B` is either the 0th term or 2nd term
//! depending on position we are writing.(see scalar code).
//!
//! For vector code see see explanation.
//!
//! # Vertical bi-linear.
//! Vertical up-sampling is a bit trickier.
//!
//! ```text
//! +----+----+
//! | A1 | A2 |
//! +----+----+
//! +----+----+
//! | p1 | p2 |
//! +----+-+--+
//! +----+-+--+
//! | p3 | p4 |
//! +----+-+--+
//! +----+----+
//! | B1 | B2 |
//! +----+----+
//! ```
//!
//! For `p1`
//! - `A1` is given a weight of `3` and `B1` is given a weight of 1.
//!
//! For `p3`
//! - `B1` is given a weight of `3` and `A1` is giveb
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

pub fn upsample_vertical(input: &[i16], output_len: usize) -> Vec<i16>
{

    // How many pixels we need to skip to the next MCU row.
    let stride = input.len() >> 3;

    // We have 8 rows and we want 16 rows
    let mut row_near = input.chunks_exact(stride);

    // row far should point one row below row_near, if row near is in row 3, row far is in
    // row 4.
    let mut row_far = input[stride..].chunks_exact(stride);

    let mut out = vec![0; output_len];
    // nearest row
    let mut rw_n = row_near.next().unwrap();
    // farthest row;
    let mut rw_f = row_far.next().unwrap();
    // a remainder if things go wrongly
    let remainder = vec![0; stride];

    let mut i = 0;

    for _ in 0..8
    {
        for (near, far) in rw_n.iter().zip(rw_f.iter())
        {
            // near row
            out[i] = ((*near) * 3 + (*far)) >> 2;
            // far row
            out[i + stride] = ((*far) * 3 + (*near)) >> 2;

            i += 1;
        }
        i += stride;

        rw_n = row_near.next().unwrap_or(&remainder);
        //
        rw_f = row_far.next().unwrap_or(&rw_n);
    }
    //   out.chunks(output_len>>4).for_each(|x|println!("{:?}",x));

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
#[cfg(feature = "x86")]
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
#[cfg(feature = "x86")]
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
