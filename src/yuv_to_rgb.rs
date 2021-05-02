#![allow(clippy::many_single_char_names, clippy::similar_names)]
//! YUV to RGB Conversion
//!
//! Conversion equations can be implemented are
//! ```text
//! R = Y + 1.40200 * Cr
//! G = Y - 0.34414 * Cb - 0.71414 * Cr
//! B = Y + 1.77200 * Cb
//! ```
//! To avoid floating point arithmetic, (which is expensive a good explanation is found [here])
//! we represent fractional constants as integers scaled up to 2¹⁶( 4 digits precision);
//! we have to divide the products by 2¹⁶ with appropriate rounding to get correct answer.
//!
//! For even more speed, we avoid multiplications by precalculating constants
//! times Cb and Cr for all possible values.
//!
//! This is reasonable for 8 bit samples 256 values for each 4 tables(about 4kb memory needed
//! to store the tables in memory)
//!
//! For benchmarks of different ways to convert YUV/YCbCr colorspace to RGB see
//! `/benches/yuv_to_rgb.rs`
//!
//![here]:http://justinparrtech.com/JustinParr-Tech/programming-tip-turn-floating-point-operations-in-to-integer-operations/
use ndarray::{Array2, Zip};
use std::cmp::{max, min};

/// Generate tables used in const evaluation
const fn uv_to_rgb_tables(u: i32, v: i32) -> (i32, i32, i32, i32) {
    // This was an implementation borrowed from ffmpeg
    // and I forgot where, so ill leave a `TODO` here until i findd it
    let u = u - 128;
    let v = v - 128;
    // A good intro to bitwise operations
    // https://www.codeproject.com/Articles/2247/An-Introduction-to-Bitwise-Operators

    // add and divide by 2^16
    let coeff_rv = 91881 * v;
    let coeff_gu = -22554 * u;
    let coeff_gv = -46802 * v;
    let coeff_bu = 116130 * u;
    //let r = (y + (91881 * v + 32768 >> 16)) as u8;
    //let g = (y + (-22554 * u - 46802 * v + 32768 >> 16)) as u8;
    // let b = (y + 116130 * u+ 32768 >> 16) as u8;
    (coeff_rv, coeff_gu, coeff_gv, coeff_bu)
}
/// Generate lookup tables for the constants
const fn generate_tables() -> ([i32; 255], [i32; 255], [i32; 255], [i32; 255]) {
    // each table contains conversion tables
    let mut rv_table: [i32; 255] = [0; 255];
    let mut gu_table: [i32; 255] = [0; 255];
    let mut gv_table: [i32; 255] = [0; 255];
    let mut bu_table: [i32; 255] = [0; 255];

    // for loop isn't allowed in const functions, which is weird
    let mut pos: usize = 0;
    while pos != 255 {
        let v = uv_to_rgb_tables(pos as i32, pos as i32);
        rv_table[pos] = v.0;
        gu_table[pos] = v.1;
        gv_table[pos] = v.2;
        bu_table[pos] = v.3;
        pos += 1;
    }
    (rv_table, gu_table, gv_table, bu_table)
}
const ALL_TABLES: ([i32; 255], [i32; 255], [i32; 255], [i32; 255]) = generate_tables();
/// `1.40200 * V` bit-shifted to int for all values from 1 to 255
const RV_TABLE: [i32; 255] = ALL_TABLES.0;
/// `0.34414 * u` bit-shifted to int for all values from 1 to 255
const GU_TABLE: [i32; 255] = ALL_TABLES.1;
/// `0.71414 * v` bit-shifted to int for all values from 1 to 255
const GV_TABLE: [i32; 255] = ALL_TABLES.2;
/// `1.77200 * u` bit-shifted to int for all values from 1 to 255
const BU_TABLE: [i32; 255] = ALL_TABLES.3;

/// 8 bit conversion of YUV to RGB conversion using lookup tables
/// and clamping values between 0 and 255
///
/// # Arguments
/// > - `y`:Luma component
/// > - `cb`,`cr`: Chroma components
/// # Returns
/// > `(r:u8,g:u8,b:u8)`: Converted color components from  the inputs
#[allow(
    clippy::module_name_repetitions,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap
)]
pub fn yuv_to_rgb_8_bit(y: u8, cb: u8, cr: u8) -> (u8, u8, u8) {
    let (y_c, u_c, v_c) = ((y as i32) << 16, cb as usize, cr as usize);
    unsafe {
        // Okay why?
        // Saves us some time because we know it will never panic as u8 are between 0 and 255 and
        // our tables are between 0 and 255 so it will never be out of bounds (the u8 would overflow or underflow before it becomes out of bounds)

        // shift down  16 bits to it's original value after calculations
        let r = y_c + RV_TABLE.get_unchecked(v_c) + 32768 >> 16;

        let g = y_c + GU_TABLE.get_unchecked(u_c) + GV_TABLE.get_unchecked(v_c) >> 16;
        let b = y_c + BU_TABLE.get_unchecked(u_c) + 32768 >> 16;

        return (clamp(r), clamp(g), clamp(b));
    }
}
/// Convert YCbCr color space to RGB colorspace for each color space in the MCU
///
/// #  Arguments
/// > - `y`,`cb`,`cr`: 2-Dimension array (8 by 8) representing an MCU in the image
/// # Returns
/// > `[Array2<f64>;3]`:An array containing `R`,`G` and `B` elements, each with 64 elements for the MCU
#[allow(clippy::module_name_repetitions)]
pub fn yuv_to_rgb_mcu(y: &Array2<u8>, cb: &Array2<u8>, cr: &Array2<u8>) -> [Array2<u8>; 3] {
    let mut r = Array2::zeros((8, 8));
    // cloning is faster than constructing
    let mut g = r.clone();
    let mut b = g.clone();
    let mut pos = 0;
    Zip::from(y).and(cb).and(cr).for_each(|y, cb, cr| {
        let values = yuv_to_rgb_8_bit(*y, *cb, *cr);
        // positions to place the new arrays
        let (x_plane, y_plane) = (pos / 8, pos % 8);

        r[[x_plane, y_plane]] = values.0;
        g[[x_plane, y_plane]] = values.1;
        b[[x_plane, y_plane]] = values.2;
        pos += 1;
    });
    [r, g, b]
}
/// Limit values to 0 and 255
fn clamp(a: i32) -> u8 {
    (min(max(0, a), 255)) as u8
}
