use std::convert::TryInto;

use crate::misc::Aligned32;

/// Carry out dequantization and IDCT for component data
///
/// # Arguments
/// - component :`mut &[i32]`  A mutable reference to a component data. Results after dequantization
///  and IDCT will be written back in the source allowing us to reuse data
/// - QT table: A quantization table for this component
/// - func: A mutable function that can carry out dequantization and IDCT, this allows us to change
/// the function carrying out IDCT to faster(or slower) implementations depending on the target machine
#[inline(always)]
pub fn dequantize_idct_component<F>(component: &mut [i32], qt_table: &[i32; 64], func: &mut F)
where
    F: FnMut(&mut [i32; 64], &[i32; 64]),
{
    for i in component.chunks_exact_mut(64) {
        // reuse memory by overwriting the old values with dequantized values
        func(i.try_into().unwrap(), qt_table);
    }
}
/// Carry out up-sampling and color conversion functions
///
/// # Arguments
///  - y: Y color component data
///  - cb: Cb color component data
///  - cr: Cr component data
///  - convert_function: A function capable of converting from YCBCr to RGB colorspace
///  - position: The offset from where to write our data
///  - output: The area to write data on the colorspace
#[inline(always)]
pub fn upsample_color_convert_ycbcr<F>(
    y: &Aligned32<&[i32]>,
    cb: &Aligned32<&[i32]>,
    cr: &Aligned32<&[i32]>,
    convert_function: &mut F,
    position: usize,
    output: &mut [u8],
) where
    F: FnMut(&[i32], &[i32], &[i32], &mut [u8], usize),
{
    // The logic here is a bit hard
    // The reason is how MCUs are designed, in the input every 64 step represents a MCU
    // but MCU's traverse rows  so we have to do weird skipping and slicing( which is bad cache wise)

    // iterate 8 times
    let mut pos = 0;
    // How many MCU's do we have?
    let mcu_count = y.0.len() / 64;
    let mut position = position;
    for i in 0..8 {
        // depending on the value of i chunk data
        for _ in 0..mcu_count {
            #[cfg(feature = "perf")]
            {
                // SAFETY: Position can never go out of bounds
                unsafe {
                    let y_c = y.0.get_unchecked(pos..pos + 8);
                    let cb_c = cb.0.get_unchecked(pos..pos + 8);
                    let cr_c = cr.0.get_unchecked(pos..pos + 8);
                    convert_function(y_c, cb_c, cr_c, output, position);
                }
            }
            #[cfg(not(feature = "perf"))]
                {
                    let y_c = &y.0[pos..pos + 8];
                    let cb_c = &cb.0[pos..pos + 8];
                    let cr_c = &cr.0[pos..pos + 8];
                    convert_function(y_c, cb_c, cr_c, output, position);
                }
            // increase position by 24, because we wrote 24 RGB values to array
            position += 24;
            // increase pos by 64, we skip an mcu
            pos += 64;
        }
        // reset pos to i*8
        pos = (i + 1) * 8;
    }
}
