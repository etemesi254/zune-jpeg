use std::cmp::min;
use std::convert::TryInto;

use crate::{ColorConvert16Ptr, ColorConvertPtr, ColorSpace, IDCTPtr, MAX_COMPONENTS};
use crate::components::Components;

/// Handle everything else in jpeg processing that doesn't involve bitstream decoding
///
/// This handles routines for images which are interleaved for non-interleaved use post_process_non_interleaved
///
/// # Arguments
/// - unprocessed- Contains Y,Cb,Cr components straight from the bitstream decoder
/// - component_data - Contains metadata for unprocessed values, e.g QT tables and such
/// - idct_func - IDCT function pointer
/// - color_convert_16 - Carry out color conversion on 2 mcu's
/// - color_convert - Carry out color conversion on a single MCU
/// - input_colorspace - The colorspace the image is in
/// - output_colorspace: Colorspace to change the value to
/// - output - Where to write the converted data
/// - mcu_len - Number of MCU's per width
/// - width - Width of the image.
/// - position: Offset from which to write the pixels
#[allow(
clippy::too_many_arguments,
clippy::cast_sign_loss,
clippy::cast_possible_truncation,
clippy::doc_markdown,
clippy::single_match
)]
#[rustfmt::skip]
pub(crate) fn post_process(
    unprocessed: &mut [Vec<i16>; MAX_COMPONENTS], component_data: &[Components], h_samp: usize,
    v_samp: usize, idct_func: IDCTPtr, color_convert_16: ColorConvert16Ptr,
    color_convert: ColorConvertPtr, input_colorspace: ColorSpace, output_colorspace: ColorSpace,
    output: &mut [u8], mcu_len: usize, width: usize,
) // so many parameters..
{
    // carry out dequantization and inverse DCT

    // for the reason for the below line, see post_process_no_interleaved
    let x = min(
        input_colorspace.num_components(),
        output_colorspace.num_components(),
    );

    (0..x).for_each(|z| {
        // Calculate stride
        // Stride is basically how many pixels must we traverse to write an MCU
        // e.g For an 8*8 MCU in a 16*16 image
        // stride is 16 because move 16 pixels from location, you are below where you are writing

        // Shift by 3 is divide by 8 for those wondering...
        let stride = (unprocessed[z].len() / (h_samp * v_samp)) >> 3;

        // carry out IDCT.
        unprocessed[z] = idct_func(&unprocessed[z], &component_data[z].quantization_table, stride, h_samp * v_samp);
    });
    if h_samp != 1 || v_samp != 1
    {
        // carry out upsampling , the return vector overwrites the original vector
        for i in 1..x
        {
            unprocessed[i] = (component_data[i].up_sampler)(&unprocessed[i], unprocessed[0].len());
        }
    }

    // color convert
    match (input_colorspace, output_colorspace)
    {
        (ColorSpace::YCbCr, ColorSpace::GRAYSCALE) =>
            {

                // Convert i16's to u8's
                let temp_output = unprocessed[0].iter().map(|x| *x as u8).collect::<Vec<u8>>();
                // chunk according to width.

                let width_mcu = unprocessed[0].len() / width;

                let width_chunk = unprocessed[0].len() / width_mcu;

                let mut start = 0;
                
                let mut end = width;

                for chunk in temp_output.chunks_exact(width_chunk) {
                    // copy data, row wise, we do it row wise to discard fill bits if the
                    // image has an uneven width not divisible by 8.

                    output[start..end].copy_from_slice(&chunk[0..width]);

                    start += width;
                    end += width;
                }
            }

        (ColorSpace::YCbCr, ColorSpace::YCbCr) =>
            {
                // copy to a temporary vector.

                let mcu_chunks = unprocessed[0].len() / (h_samp * v_samp);


                // pixels we write per width. since this is YcbCr we write
                // width times color components.
                let stride = width * 3;

                // Allocate vector for temporary storage
                let temp_size = stride * 8;

                let mut temp_output = vec![0; temp_size + (128 * output_colorspace.num_components() * h_samp * v_samp)];


                let mut start = 0;

                let mut end = width * 3;

                let addition = width * 3;

                // width which accounts number of fill bytes
                let width_chunk = mcu_chunks >> 3;


                for ((y_chunk, cb_chunk), cr_chunk) in unprocessed[0]
                    .chunks_exact(width_chunk)
                    .zip(unprocessed[1].chunks_exact(width_chunk))
                    .zip(unprocessed[2].chunks_exact(width_chunk))
                {
                    // OPTIMIZE-TIP: Don't do loops in Rust, use iterators in such manners to ensure super
                    // powers on optimization.
                    // Using indexing will cause Rust to do bounds checking and prevent some cool optimization
                    // options. See this  compiler-explorer link https://godbolt.org/z/Kh3M43hYr for what I mean.

                    for (((y, cb), cr), out) in y_chunk.iter()
                        .zip(cb_chunk.iter())
                        .zip(cr_chunk.iter())
                        .zip(temp_output.chunks_exact_mut(3))
                    {
                        out[0] = *y as u8;
                        out[1] = *cb as u8;
                        out[2] = *cr as u8;
                    }

                    //output.lock().unwrap()[pos..pos + stride].copy_from_slice(&temp_output[0..stride]);
                    output[start..end].copy_from_slice(&temp_output[0..stride]);

                    start += addition;

                    end += addition;
                }
            }

        (
            ColorSpace::YCbCr,
            ColorSpace::RGB | ColorSpace::RGBA | ColorSpace::RGBX,
        ) =>
            {
                color_convert_ycbcr(
                    unprocessed, width, h_samp, v_samp, output_colorspace, color_convert_16,
                    color_convert, output, mcu_len,
                );
            }
        // For the other components we do nothing(currently)
        _ =>
            {}
    }
}

/// Do color-conversion for interleaved MCU
#[allow(
clippy::similar_names,
clippy::too_many_arguments,
clippy::needless_pass_by_value,
clippy::unwrap_used
)]
#[rustfmt::skip]
fn color_convert_ycbcr(
    mcu_block: &[Vec<i16>; MAX_COMPONENTS],
    width: usize,
    h_samp: usize,
    v_samp: usize,
    output_colorspace: ColorSpace,
    color_convert_16: ColorConvert16Ptr,
    color_convert: ColorConvertPtr,
    output: &mut [u8],
    mcu_len: usize,
)
{
    let remainder = ((mcu_len) % 2) != 0;

    // Create a temporary area to hold our color converted data
    let temp_size = width * (output_colorspace.num_components() * h_samp) * (v_samp * 8);

    let mut temp_area = vec![0; temp_size + (64 * output_colorspace.num_components() * h_samp * v_samp)];

    let mut position = 0;

    let mcu_chunks = mcu_block[0].len() / (h_samp * v_samp);

    let mut mcu_pos = 1;

    // Width of image which takes into account fill bytes(it may be larger
    // than actual width).
    let width_chunk = mcu_chunks >> 3;


    // We need to chunk per width to ensure we can discard extra values at the end of the width.
    // Since the encoder may pad bits to ensure the width is a multiple of 8.
    for ((y_width, cb_width), cr_width) in mcu_block[0].chunks_exact(width_chunk)
        .zip(mcu_block[1].chunks_exact(width_chunk))
        .zip(mcu_block[2].chunks_exact(width_chunk))
    {

        // Chunk in outputs of 16 to pass to color_convert as an array of 16 i16's.
        for ((y, cb), cr) in y_width.chunks_exact(16)
            .zip(cb_width.chunks_exact(16))
            .zip(cr_width.chunks_exact(16))
        {
            // @ OPTIMIZE-TIP, use slices with known sizes, can turn on some optimization,
            // e.g autovectorization.
            (color_convert_16)(y.try_into().unwrap(), cb.try_into().unwrap(), cr.try_into().unwrap(), &mut temp_area, &mut position);
        }
        if remainder
        {
            // last odd MCU in the column
            let y_c = y_width.rchunks_exact(8).next().unwrap().try_into().unwrap();

            let cb_c = cb_width.rchunks_exact(8).next().unwrap().try_into().unwrap();

            let cr_c = cr_width.rchunks_exact(8).next().unwrap().try_into().unwrap();

            (color_convert)(y_c, cb_c, cr_c, &mut temp_area, &mut position);
        }
        // update position to next width.
        position = width * output_colorspace.num_components() * mcu_pos;

        mcu_pos += 1;
    }
    output.copy_from_slice(&temp_area[0..temp_size]);
}
