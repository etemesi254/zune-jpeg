use std::sync::{Arc, Mutex};

use crate::{ColorConvert16Ptr, ColorConvertPtr, ColorSpace, IDCTPtr, MAX_COMPONENTS};
use crate::components::Components;


/// Handle everything else in jpeg processing that doesn't involve bitstream decoding
///
/// This handles routines for images which are not interleaved for interleaved use post_process_interleaved
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
#[allow(clippy::too_many_arguments,clippy::cast_sign_loss,clippy::cast_possible_truncation,clippy::doc_markdown)]
pub (crate) fn post_process_non_interleaved(mut unprocessed: [Vec<i16>; MAX_COMPONENTS],
                    component_data: &[Components],
                    idct_func: IDCTPtr,
                    color_convert_16:ColorConvert16Ptr,
                    color_convert: ColorConvertPtr,
                    input_colorspace: ColorSpace, output_colorspace: ColorSpace,
                    output: Arc<Mutex<Vec<u8>>>,
                    mcu_len: usize,
                    width: usize,
                    mut position: usize) {
    // carry out IDCT

    (0..input_colorspace.num_components()).for_each(|x| {
        (idct_func)(unprocessed[x].as_mut_slice(), &component_data[x].quantization_table);
    });

    // color convert
    match (input_colorspace, output_colorspace) {

        // YCBCR to RGB(A) colorspace conversion.
        (ColorSpace::YCbCr, _) => {
            color_convert_ycbcr(&unprocessed, width, output_colorspace, color_convert_16, color_convert, output, &mut position,  mcu_len);

        }
        (ColorSpace::GRAYSCALE, ColorSpace::GRAYSCALE) => {
            // for grayscale to grayscale we copy first MCU block(which should contain the Y Luminance channel) to the other
            let x: Vec<u8> = unprocessed[0].iter().map(|c| *c as u8).collect();
            output.lock().unwrap().copy_from_slice(x.as_slice());
        }
        // For the other components we do nothing(currently)
        _ => {}
    }
}
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

#[allow(clippy::too_many_arguments,clippy::cast_sign_loss,clippy::cast_possible_truncation,clippy::doc_markdown,unused_variables)]
pub(crate) fn post_process_interleaved(mut unprocessed: [Vec<i16>; MAX_COMPONENTS],
                                       component_data: &[Components],
                                       idct_func: IDCTPtr,
                                       color_convert_16:ColorConvert16Ptr,
                                       color_convert: ColorConvertPtr,
                                       input_colorspace: ColorSpace,
                                       output_colorspace: ColorSpace,
                                       output: Arc<Mutex<Vec<u8>>>,
                                       mcu_len: usize,
                                       width: usize,
                                       mut position: usize){

    // carry out dequantization and inverse DCT
    (0..input_colorspace.num_components()).for_each(|z|{
        idct_func(&mut unprocessed[z],&component_data[z].quantization_table);
    });
    // carry out upsampling , the return vector overwrites the original vector
    for i in 1..input_colorspace.num_components(){
        unprocessed[i]=(component_data[i].up_sampler)(&unprocessed[i],unprocessed[0].len());
    }
    // color convert
    match (input_colorspace, output_colorspace) {
        // YCBCR to RGB(A) colorspace conversion.
        (ColorSpace::YCbCr, _) => {
            color_convert_ycbcr(&unprocessed, width, output_colorspace, color_convert_16, color_convert, output, &mut position,  mcu_len);

        }
        (ColorSpace::GRAYSCALE, ColorSpace::GRAYSCALE) => {
            // for grayscale to grayscale we copy first MCU block(which should contain the Y Luminance channel) to the other
            let x: Vec<u8> = unprocessed[0].iter().map(|c| *c as u8).collect();
            output.lock().unwrap().copy_from_slice(x.as_slice());
        }
        // For the other components we do nothing(currently)
        _ => {}
    }
}
#[allow(clippy::similar_names,clippy::too_many_arguments,clippy::needless_pass_by_value)]
fn color_convert_ycbcr(mcu_block: &[Vec<i16>; MAX_COMPONENTS],
                       width: usize,
                       output_colorspace: ColorSpace,
                       color_convert_16: ColorConvert16Ptr,
                       color_convert: ColorConvertPtr,
                       output: Arc<Mutex<Vec<u8>>>,
                       position: &mut usize,
                       mcu_len: usize) {
    // The logic here is a bit hard
    // The reason is how MCUs are designed, in the input every 64 step represents a MCU
    // but MCU's traverse rows  so we have to do weird skipping and slicing( which is bad cache wise)
    let mut pos = 0;

    // slice into 128(2 mcu's)
    //println!("{}",mcu_len);
    let mcu_count = mcu_len  >> 1;
    //println!("{}",mcu_count);
    // check if we have an MCU remaining
    let remainder = ((mcu_len ) % 2) != 0;
    let mcu_width = width * output_colorspace.num_components();
    let mut expected_pos = *position + mcu_width;
    // Lock here because we do not want to keep locking while writing,
    // this is cheaper actually but it's syntax becomes weird when dereferencing
    let mut x = output.lock().unwrap();
    for i in 0..8 {
        // Process MCU's in batches of 2, this allows us (where applicable) to convert two MCU rows
        // using fewer instructions
        //println!("{},{}",position,pos);
        for _ in 0..mcu_count {
            //This isn't cache efficient as it hops around too much

            // SAFETY
            // 1. mcu_block is initialized, (note not assigned) with zeroes
            // enough to ensure that this is unsafe,
            // The bounds here can never go above the length
            unsafe {
                // remove some cmp instructions that were slowing us down

                let y_c = mcu_block[0].get_unchecked(pos..pos + 8);
                let cb_c = mcu_block[1].get_unchecked(pos..pos + 8);
                let cr_c = mcu_block[2].get_unchecked(pos..pos + 8);
                //  8 pixels of the second MCU
                let y1_c = mcu_block[0].get_unchecked(pos + 64..pos + 72);
                let cb2_c = mcu_block[1].get_unchecked(pos + 64..pos + 72);
                let cr2_c = mcu_block[2].get_unchecked(pos + 64..pos + 72);
                // Call color convert function
                (color_convert_16)(y_c, y1_c, cb_c, cb2_c, cr_c, cr2_c, &mut **x, position);
                // increase pos by 128, skip 2 MCU's
            }
            pos += 128;
        }
        //println!("{}",position);
        if remainder {
            // last odd MCU in the column
            let y_c = &mcu_block[0][pos..pos + 8];
            let cb_c = &mcu_block[1][pos..pos + 8];
            let cr_c = &mcu_block[2][pos..pos + 8];
            // convert function should be able to handle
            // last mcu
            (color_convert)(y_c, cb_c, cr_c, &mut **x, position);
            //*position+=24;
        }

        // Sometimes Color convert may overshoot, IE if the image is not
        // divisible by 8 it may have to pad the last MCU with extra pixels
        // The decoder is supposed to discard these extra bits
        //
        // But instead of discarding those bits, I just tell the color_convert to overwrite them
        // Meaning I have to reset position to the expected position, which is the width
        // of the MCU.

        *position = expected_pos;
        expected_pos += mcu_width;
      //  println!("{}",position);

        // Reset position to start fetching from the next MCU
        pos = (i + 1) * 8;

    }
}