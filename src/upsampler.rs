//! Up-sampling routines
//!
//! The main upsampling method is a bi-linear interpolation or a "triangle
//! filter " or libjpeg turbo `fancy_upsampling` which is a good compromise
//! between speed and visual quality

#[cfg(feature = "x86")]
pub use sse::upsample_horizontal_sse;

mod sse;

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

/// Upsample horizontally
///
/// The up-sampling algorithm used is libjpeg-turbo `fancy_upsampling` which is
/// a linear interpolation or triangle filter, see module docs for explanation

pub fn upsample_horizontal(input: &[i16], output_len: usize) -> Vec<i16>
{
    let mut out = vec![128; output_len];

    out[0] = input[0];

    out[1] = (input[0] * 3 + input[1] + 2) >> 2;

    let input_len = input.len();

    // duplicate, number of MCU's are 2
    for i in 1..input_len - 1
    {
        let sample = 3 * input[i] + 2;

        out[i * 2] = (sample + input[i - 1]) >> 2;

        out[i * 2 + 1] = (sample + input[i + 1]) >> 2;
    }

    // write last mcu
    out[(input_len - 1) * 2] = (input[input_len - 2] * 3 + input[input_len - 1] + 2) >> 2;

    out[(input_len - 1) * 2 + 1] = input[input_len - 1];

    // out[0..input.len()].copy_from_slice(input);

    //out[input.len()..(input.len()*2)].copy_from_slice(input);
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
