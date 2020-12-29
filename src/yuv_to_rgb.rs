//! YUV to RGB Conversion
//!
//! Conversion equations can be implemented are
//! ```text
//! R = Y + 1.40200 * Cr
//! G = Y - 0.34414 * Cb - 0.71414 * Cr
//! B = Y + 1.77200 * Cb
//! ```
//!  To avoid floating point arithmetic, (which is expensive see [here](http://justinparrtech.com/JustinParr-Tech/programming-tip-turn-floating-point-operations-in-to-integer-operations/)))
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
//!
use std::cmp::{max, min};

/// Generate tables
#[inline]
const fn yuv_to_rgb_tables(u: i32, v: i32) -> (i32, i32, i32, i32) {
    let u = u -128;
    let v = v -128;
    // Minimum Supported Rust Version just went up to 1.46.0
    // where saturating integers were implemented
    let coeff_rv = (91881 * v + 32768 >> 16);
    let coeff_gu =  -22554 * u;
    let coeff_gv = -46802 * v;
    let coeff_bu =  116130 * u;
    //let r = (y + (91881 * v + 32768 >> 16)) as u8;
    //let g = (y + (-22554 * u - 46802 * v + 32768 >> 16)) as u8;
    // let b = (y + 116130 * u+ 32768 >> 16) as u8;
    (coeff_rv, coeff_gu, coeff_gv, coeff_bu)
}
const fn generate_tables() -> ([i32; 255], [i32; 255], [i32; 255], [i32; 255]) {
    let mut rv_table:[i32;255] = [0;255];
    let mut gu_table:[i32;255] = [0;255];
    let mut gv_table:[i32;255] = [0;255];
    let mut bu_table: [i32; 255] = [0;255];
    let mut f = 0;
    while f != 255 {
        let pos = f as usize;
        let v = yuv_to_rgb_tables(f,f);
        rv_table[pos] = v.0 ;
        gu_table[pos] = v.1;
        gv_table[pos] = v.2;
        bu_table[pos] = v.3;
        f+=1;
    };
    (rv_table,gu_table,gv_table,bu_table)
}
const ALL_TABLES: ([i32; 255], [i32; 255], [i32; 255], [i32; 255]) = generate_tables();
/// 1.40200 * V for all values from 1 to 255
const RV_TABLE: [i32; 255] = ALL_TABLES.0;
/// 0.34414 * u for all values from 1 to 255
const GU_TABLE: [i32; 255] = ALL_TABLES.1;
/// 0.71414 * v for all values from 1 to 255
const GV_TABLE: [i32; 255] = ALL_TABLES.2;
/// 1.77200 * u for all values from 1 to 255
const BU_TABLE :[i32;255]=   ALL_TABLES.3;
#[inline(always)]
fn yuv_to_rgb(y:u8, u:u8, v:u8) -> (u8, u8, u8) {
    let (y,u,v) = (y as usize, u as usize,v as usize);
    let r = (y as i32+RV_TABLE[v]) ;
    let g = (y as i32+GU_TABLE[u]+GV_TABLE[v]+32768 >> 16) ;
    let b = (y as i32+BU_TABLE[u]+32768 >> 16) ;

    return (clamp(r),clamp(g),clamp(b));

}
/// Fastest clamp I could find
fn clamp(a:i32)->u8{
    (min(max(0,a),255)) as u8
}