use std::cmp::min;
use std::convert::TryInto;

use std::sync::{Arc, Mutex};

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
    output: Arc<Mutex<Vec<u8>>>, mcu_len: usize, width: usize, mut position: usize,
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
        let length = unprocessed[z].len();

        let stride = length / (h_samp * v_samp) >> 3;
        // carry out IDCT.
        unprocessed[z] = idct_func(&unprocessed[z], &component_data[z].quantization_table, stride);
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
            let num_elements = width * 8 * h_samp * v_samp;

            let x = unprocessed[0].iter().map(|x| *x as u8).collect::<Vec<u8>>();
            // copy data
            output.lock().unwrap()[position..position + num_elements].copy_from_slice(x.get(0..num_elements).unwrap());
        }
        (
            ColorSpace::YCbCr,
            ColorSpace::RGB | ColorSpace::RGBA | ColorSpace::RGBX,
        ) =>
            {
                color_convert_ycbcr(
                    unprocessed, width, h_samp, v_samp, output_colorspace, color_convert_16,
                    color_convert, output, &mut position, mcu_len,
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
    output: Arc<Mutex<Vec<u8>>>,
    position_0: &mut usize,
    mcu_len: usize,
)
{


    let remainder = ((mcu_len) % 2) != 0;

    let mcu_width = width * output_colorspace.num_components();

    let mut expected_pos = mcu_width;

    // Create a temporary area to hold our color converted data
    let temp_size = width * (output_colorspace.num_components() * h_samp) * (v_samp * 8);

    let mut temp_area = vec![0; temp_size + 64];

    let mut position = 0;

    // chunk output according to an MCU, allows us to write more than one MCU(for interleaved) images.
    let mcu_chunks = width * 8;

    for ((y_chunk, cb_chunk), cr_chunk) in mcu_block[0]
        .chunks_exact(mcu_chunks)
        .zip(mcu_block[1].chunks_exact(mcu_chunks))
        .zip(mcu_block[2].chunks_exact(mcu_chunks))
    {

        for (y, (cb, cr)) in y_chunk.chunks_exact(16).zip(cb_chunk.chunks_exact(16).zip(cr_chunk.chunks_exact(16)))
        {
            (color_convert_16)(y.try_into().unwrap(), cb.try_into().unwrap(), cr.try_into().unwrap(), &mut temp_area, &mut position);
        }

        if remainder
        {
            // last odd MCU in the column
            let y_c = y_chunk.rchunks_exact(8).next().unwrap().try_into().unwrap();

            let cb_c = cb_chunk.rchunks_exact(8).next().unwrap().try_into().unwrap();

            let cr_c = cr_chunk.rchunks_exact(8).next().unwrap().try_into().unwrap();

            (color_convert)(y_c, cb_c, cr_c, &mut temp_area, &mut position);
        }

        // Sometimes Color convert may overshoot, I.e if the image width is not
        // divisible by 8 it may have to pad the last MCU with extra pixels.

        // The decoder is supposed to discard these extra bits
        position = expected_pos;

        expected_pos += mcu_width;
    }

    output.lock().expect("Poisoned mutex")[*position_0..*position_0 + temp_size].copy_from_slice(&temp_area[0..temp_size]);

}
