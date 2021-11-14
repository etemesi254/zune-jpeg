/// Upsample horizontally
///
/// The up-sampling algorithm used is libjpeg-turbo `fancy_upsampling` which is
/// a linear interpolation or triangle filter, see module docs for explanation
#[inline(always)] // for upsample-horizontal-vertical
pub fn upsample_horizontal(input: &[i16], output_len: usize) -> Vec<i16>
{
    let mut out = vec![0; output_len];

    assert!(
        out.len() > 4 && input.len() > 2,
        "Too Short of a vector, cannot upsample"
    );
    out[0] = input[0];

    out[1] = (input[0] * 3 + input[1] + 2) >> 2;

    // This code is written for speed and not readability
    //
    // The readable code is
    //
    //      for i in 1..input.len() - 1{
    //         let sample = 3 * input[i] + 2;
    //         out[i * 2] = (sample + input[i - 1]) >> 2;
    //         out[i * 2 + 1] = (sample + input[i + 1]) >> 2;
    //     }
    //
    // The output of a pixel is determined by it's surrounding neighbours but we attach more weight to it's nearest
    // neighbour (input[i]) than to the next nearest neighbour.

    for (output_window, input_window) in out[2..].chunks_exact_mut(2).zip(input.windows(3))
    {
        // output_window = out[i*2],out[i*2+1].
        // input_window = input[i-1], input[i], input[i+1]

        let input_window: &[i16; 3] = input_window.try_into().unwrap();

        let sample = 3 * input_window[1] + 2;

        output_window[0] = (sample + input_window[0]) >> 2;

        output_window[1] = (sample + input_window[2]) >> 2;
    }
    // handle last two portions (in the most ugliest of ways)

    // Get lengths
    let out_len = out.len() - 2;
    let input_len = input.len() - 2;

    // slice the output vector
    let f_out: &mut [i16; 2] = out.get_mut(out_len..).unwrap().try_into().unwrap();
    // get a slice of the input vector
    let i_last: &[i16; 2] = input.get(input_len..).unwrap().try_into().unwrap();

    // write out manually..
    f_out[0] = (3 * i_last[0] + i_last[1] + 2) >> 2;

    f_out[1] = i_last[1];

    return out;
}
/// Vertical upsampling (which I don't trust but it works anyway)
///
/// The algorithm is still a bi-linear filter with some caveats.
#[inline(always)] // for upsample-horizontal-vertical
pub fn upsample_vertical(input: &[i16], output_len: usize) -> Vec<i16>
{
    // the caveats, row_near and row_far for the first row point to the same
    // array, that boils to a nearest neighbour upsampling?

    // if row_near doesn't have data (next returns None), it uses it's
    // previous row, so this again becomes a nearest neighbour

    // How many pixels we need to skip to the next MCU row.
    let stride = input.len() >> 3;

    // We have 8 rows and we want 16 rows
    let mut row_near = input.chunks_exact(stride);

    // row far should point one row below row_near, if row near is in row 3, row far is in
    // row 4.
    let mut row_far = input.chunks_exact(stride);

    let mut out = vec![0; output_len];
    // nearest row
    let mut rw_n = row_near.next().unwrap();
    // farthest row;
    let mut rw_f = row_far.next().unwrap();
    // previous row, for out of edge cases, e.g if we reach a row and
    // we don't have data for that row, we use the previous row
    let mut previous;

    let mut i = 0;

    let mut next_row = true;

    for _ in 0..8
    {
        // A bit of cheat here.
        //
        // Let me explain, split at mut will return two slices, one containing 0..stride
        // and another containing what remains.
        //
        // The length of out_near => stride, remainder will ALWAYS be greater than or equal to stride.
        // now this is important with the loop below, we are using zip iterators which have an interesting
        // property, they stop if any iterator doesn't have elements
        //
        // So we know remainder will be >= stride but out_near will be stride, therefore the zip will
        // effectively stop when we write two rows.
        //
        // Ideally all of this is done to eliminate bounds check, the compiler will vectorize this
        // better without the bounds check.
        let (out_near, remainder) = out[i..].split_at_mut(stride);

        for (((near, far), on), of) in rw_n
            .iter()
            .zip(rw_f.iter())
            .zip(out_near.iter_mut())
            .zip(remainder.iter_mut())
        {
            // near row
            *on = ((*near) * 3 + (*far) + 2) >> 2;
            // far row
            *of = ((*far) * 3 + (*near) + 2) >> 2;
        }
        // we wrote two adjacent lines update i to point to next position
        // in output buffer.
        i += stride * 2;

        previous = rw_n;

        rw_n = row_near.next().unwrap_or(previous);

        rw_f = row_far.next().unwrap_or(rw_n);
        // okay here I'm lazy. Ideally what i want s simply this.
        // for the first row, keep row_near and row_far to the same, row, but for sub-sequent rows,
        // row_far should point to the next row(1 row further than this row_near).
        if next_row
        {
            rw_f = row_far.next().unwrap_or(rw_n);
            next_row = false;
        }
    }
    return out;
}
pub fn upsample_hv(input: &[i16], output_len: usize) -> Vec<i16>
{
    //  a hv upsample is simply a two pass sample, first sample vertically, then sample horizontally
    // because we spent too much time writing our horizontal and vertical sub sampling  to
    // create outputs twice their inputs, we can just do this and we know they will work
    // Furthermore, we know it will be fast, upsample vertical and horizontal are super optimized
    // to eliminate bounds check.

    // But there is an AVX version obviously.

    // first pass, do vertical sampling
    let first_pass = upsample_vertical(input, input.len() * 2);
    //second pass, do horizontal sampling
    let second_pass = upsample_horizontal(&first_pass, output_len);

    return second_pass;
}