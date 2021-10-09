use std::cmp::min;
use std::convert::TryInto;
use std::sync::{Arc, Mutex};

use crate::components::Components;
use crate::{ColorConvert16Ptr, ColorConvertPtr, ColorSpace, IDCTPtr, MAX_COMPONENTS};

// In case data isn't there
const BASE_ARRAY: [i16; 8] = [128; 8];

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
#[allow(clippy::too_many_arguments, clippy::cast_sign_loss, clippy::cast_possible_truncation, clippy::doc_markdown, clippy::single_match)]
#[rustfmt::skip]
pub(crate) fn post_process(unprocessed: &mut [Vec<i16>; MAX_COMPONENTS],
                                       component_data: &[Components], h_samp: usize, v_samp: usize,
                                       idct_func: IDCTPtr, color_convert_16: ColorConvert16Ptr,
                                       color_convert: ColorConvertPtr, input_colorspace: ColorSpace,
                                       output_colorspace: ColorSpace, output: Arc<Mutex<Vec<u8>>>,
                                       mcu_len: usize, width: usize, mut position: usize) // so many parameters..
{
    // carry out dequantization and inverse DCT

    // for the reason for the below line, see post_process_no_interleaved
    let x = min(input_colorspace.num_components(), output_colorspace.num_components());

    (0..x).for_each(|z|
        {
            idct_func(&mut unprocessed[z], &component_data[z].quantization_table);
        }
    );

    if h_samp != 1 || v_samp != 1 {
        // carry out upsampling , the return vector overwrites the original vector
        for i in 1..x
        {
            unprocessed[i] = (component_data[i].up_sampler)(&unprocessed[i], unprocessed[0].len());
        }
    }
    // color convert
    match (input_colorspace, output_colorspace)
    {
        // YCBCR to RGB(A) colorspace conversion.

        // Match keyword is amazing..
        (ColorSpace::YCbCr, ColorSpace::RGB | ColorSpace::RGBA | ColorSpace::RGBX | ColorSpace::GRAYSCALE) => {
            color_convert_ycbcr(&unprocessed, width, h_samp, v_samp,
                                output_colorspace, color_convert_16, color_convert,
                                output, &mut position, mcu_len);
        }
        // For the other components we do nothing(currently)
        _ => {}
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
    h_max: usize,
    v_max: usize,
    output_colorspace: ColorSpace,
    color_convert_16: ColorConvert16Ptr,
    color_convert: ColorConvertPtr,
    output: Arc<Mutex<Vec<u8>>>,
    position_0: &mut usize,
    mcu_len: usize,
)
{

    let mut pos;


    let mcu_count = mcu_len >> 1;

    // check if we have an MCU remaining, i.e there are odd mcu's
    let remainder = ((mcu_len) % 2) != 0;

    let mcu_width = width * output_colorspace.num_components();

    let mut expected_pos = mcu_width;

    // Create a temporary area to hold our color converted data
    let temp_size = width * 8 * output_colorspace.num_components() * h_max * v_max;

    let mut temp_area = vec![0; temp_size + 64]; // over allocate

    let mut position = 0;

    // chunk output according to an MCU, allows us to write more than one MCU(for interleaved):
    let mcu_chunks = width * 8;
    for ((y_chunk, cb_chunk), cr_chunk) in mcu_block[0]
        .chunks(mcu_chunks)
        .zip(mcu_block[1].chunks(mcu_chunks))
        .zip(mcu_block[2].chunks(mcu_chunks))
    {

        {

            for i in 0..8
            {

                pos = i * 8;

                // Process MCU's in batches of 2
                for _ in 0..mcu_count
                {

                    //This isn't cache efficient as it hops around too much
                    let y_c = y_chunk.get(pos..pos + 8)
                        .unwrap_or(&BASE_ARRAY)
                        .try_into()
                        .unwrap();

                    let cb_c = cb_chunk.get(pos..pos + 8)
                        .unwrap_or(&BASE_ARRAY)
                        .try_into()
                        .unwrap();

                    let cr_c = cr_chunk.get(pos..pos + 8)
                        .unwrap_or(&BASE_ARRAY)
                        .try_into()
                        .unwrap();

                    //  8 pixels of the second MCU
                    let y1_c = y_chunk.get(pos + 64..pos + 72)
                        .unwrap_or(&BASE_ARRAY)
                        .try_into()
                        .unwrap();

                    let cb2_c = cb_chunk.get(pos + 64..pos + 72)
                        .unwrap_or(&BASE_ARRAY)
                        .try_into()
                        .unwrap();

                    let cr2_c = cr_chunk.get(pos + 64..pos + 72)
                        .unwrap_or(&BASE_ARRAY)
                        .try_into()
                        .unwrap();

                    (color_convert_16)(y_c, y1_c,
                        cb_c, cb2_c,
                        cr_c, cr2_c,
                        &mut temp_area, &mut position,
                    );

                    pos += 128;

                    if remainder
                    {

                        // last odd MCU in the column
                        let y_c = y_chunk[pos..pos + 8].try_into().unwrap();

                        let cb_c = cb_chunk[pos..pos + 8].try_into().unwrap();

                        let cr_c = cr_chunk[pos..pos + 8].try_into().unwrap();

                        (color_convert)(y_c, cb_c, cr_c, &mut temp_area, &mut position);
                    }

                }

                // Sometimes Color convert may overshoot, IE if the image is not
                // divisible by 8 it may have to pad the last MCU with extra pixels
                // The decoder is supposed to discard these extra bits
                position = expected_pos;

                expected_pos += mcu_width;
            }
        }
    }

    // update output with the values
    output.lock().expect("Poisoned mutex")[*position_0..*position_0 + temp_size]
        .copy_from_slice(&temp_area[0..temp_size]);
}
