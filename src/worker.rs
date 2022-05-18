use std::cmp::min;
use std::convert::TryInto;

use crate::color_convert::{ycbcr_to_grayscale, ycbcr_to_ycbcr};
use crate::components::Components;
use crate::decoder::{ColorConvert16Ptr, ColorConvertPtr, IDCTPtr, MAX_COMPONENTS};
use crate::misc::ColorSpace;
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
    unprocessed: &mut [Vec<i16>; MAX_COMPONENTS],
    component_data: &[Components],
    idct_func: IDCTPtr,
    color_convert_16: ColorConvert16Ptr,
    color_convert: ColorConvertPtr,
    input_colorspace: ColorSpace,
    output_colorspace: ColorSpace,
    output: &mut [u8],
    mcu_len: usize,
    width: usize,
) // so many parameters..
{
    // maximum sampling factors are in Y-channel, no need to pass them.
    let h_samp = component_data[0].horizontal_sample;
    let v_samp = component_data[0].vertical_sample;
    // carry out dequantization and inverse DCT

    // So we want to carry out IDCT and upsampling
    // But assuming we have an RGB image but the user asked for a grey-scale image,
    // we don't need to carry out idct and upsampling or even color conversion.
    // So to do the bare minimum work we take the min
    // I.e
    // RGB -> Grayscale
    // (3) - (1) => Decode 1 channel
    // GrayScale->Grayscale
    // (1)      -> (1) => Decode 1 component
    // RGB -> RGBA
    // (3) -> (4) => Decode 3 channels

    let x = min(
        input_colorspace.num_components(),
        output_colorspace.num_components(),
    );

    (0..x).for_each(|z| {
        // Calculate stride
        // Stride is basically how many pixels must we traverse to write an MCU
        // e.g For an 8*8 MCU in a 16*16 image
        // stride is 16 because move 16 pixels from location, you are below where you are writing

        // carry out IDCT.
        let v_samp_idct = {
            if z == 0 { 1 } else { v_samp }
        };
        unprocessed[z] = idct_func(&unprocessed[z],
                                   &component_data[z].quantization_table,
                                   component_data[z].width_stride,
                                   h_samp * v_samp,
                                   v_samp_idct);
    });

    post_process_inner(unprocessed, component_data, color_convert_16, color_convert,
                       input_colorspace, output_colorspace, output, mcu_len, width);
}

#[rustfmt::skip]
pub(crate) fn post_process_prog(
    block: &[&[i16]; MAX_COMPONENTS], /*The difference with post process*/
    component_data: &[Components],
    idct_func: IDCTPtr,
    color_convert_16: ColorConvert16Ptr,
    color_convert: ColorConvertPtr,
    input_colorspace: ColorSpace,
    output_colorspace: ColorSpace,
    output: &mut [u8],
    mcu_len: usize,
    width: usize,
) // so many parameters..
{
    let mut unprocessed = [vec![], vec![], vec![]];
    // maximum sampling factors are in Y-channel, no need to pass them.
    let h_samp = component_data[0].horizontal_sample;
    let v_samp = component_data[0].vertical_sample;

    let x = min(
        input_colorspace.num_components(),
        output_colorspace.num_components(),
    );
    //
    (0..x).for_each(|z| {

        // carry out IDCT.
        let v_samp_idct = { if z == 0 { 1 } else { v_samp } };

        unprocessed[z] = idct_func(block[z], &component_data[z].quantization_table,
            component_data[z].width_stride, h_samp * v_samp, v_samp_idct);
    });
    post_process_inner(&mut unprocessed, component_data, color_convert_16, color_convert,
        input_colorspace,  output_colorspace, output, mcu_len, width);
}
#[rustfmt::skip]
pub(crate) fn post_process_inner(
    unprocessed: &mut [Vec<i16>; MAX_COMPONENTS], component_data: &[Components],
    color_convert_16: ColorConvert16Ptr, color_convert: ColorConvertPtr,
    input_colorspace: ColorSpace, output_colorspace: ColorSpace, output: &mut [u8], mcu_len: usize,
    width: usize,
) // so many parameters..
{
    let x = min(
        input_colorspace.num_components(),
        output_colorspace.num_components(),
    );
    // maximum sampling factors are in Y-channel, no need to pass them.
    let h_samp = component_data[0].horizontal_sample;

    let v_samp = component_data[0].vertical_sample;

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
        (ColorSpace::YCbCr | ColorSpace::GRAYSCALE, ColorSpace::GRAYSCALE) =>
        {
            ycbcr_to_grayscale(&unprocessed[0], width, output);
        }

        (ColorSpace::YCbCr, ColorSpace::YCbCr) =>
        {
            ycbcr_to_ycbcr(unprocessed, width, h_samp, v_samp, output);
        }

        (ColorSpace::YCbCr, ColorSpace::RGB | ColorSpace::RGBA | ColorSpace::RGBX) =>
        {
            color_convert_ycbcr(unprocessed, width, h_samp, v_samp,
                output_colorspace, color_convert_16, color_convert, output, mcu_len);
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


    let mcu_chunks = mcu_block[0].len() / (h_samp * v_samp);

    // Width of image which takes into account fill bytes(it may be larger than actual width).
    let width_chunk = mcu_chunks >> 3;

    let mut start = 0;

    let stride = width * output_colorspace.num_components();

    let mut end = stride;
    // over allocate to account for fill bytes
    let mut temp_area = vec![0; width_chunk * output_colorspace.num_components() + 128];

    // We need to chunk per width to ensure we can discard extra values at the end of the width.
    // Since the encoder may pad bits to ensure the width is a multiple of 8.
    for ((y_width, cb_width), cr_width) in mcu_block[0].chunks_exact(width_chunk)
        .zip(mcu_block[1].chunks_exact(width_chunk))
        .zip(mcu_block[2].chunks_exact(width_chunk))
    {
        let mut position = 0;

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
        // Write to our output buffer row wise.
        output[start..end].copy_from_slice(&temp_area[0..stride]);

        start += stride;

        end += stride;
    }
}
