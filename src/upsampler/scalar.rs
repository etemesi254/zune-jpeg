/// Upsample horizontally
///
/// The up-sampling algorithm used is libjpeg-turbo `fancy_upsampling` which is
/// a linear interpolation or triangle filter, see module docs for explanation
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
