//! Compare the two ways of converting YUV colorspace to RGB
//! The int method is faster(about 2.5 B iterations on AMD Ryzen 4200 compared to 2.2 B iterations for ycbcr_to_rgb)
//!
//! Though a fair warning, during testing , the latter will be favoured because the former will be warming
//! the cache for it(I guess) leading to some weird results,so watch out for that pitfall.
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use std::cmp::{max, min};

/// Convert YUV to RGB color space using scaled fractions as integers
/// saving us from maths
fn yuv_to_rgb(y: i32, u: i32, v: i32) -> (u8, u8, u8) {
    let u = u - 128;
    let v = v - 128;
    // Minimum Supported Rust Version just went up to 1.46.0
    // where saturating integers were implemented
    let r = y + (91881 * v + 32768 >> 16);
    let g = y + (-22554 * u - 46802 * v + 32768 >> 16);
    let b = y + 116130 * u + 32768 >> 16;
    (clamp(r), clamp(g), clamp(b))
}

// ITU-R BT.601
// The easiest way about 927M iterations
fn ycbcr_to_rgb(y: u8, cb: u8, cr: u8) -> (u8, u8, u8) {
    let y = y as f32;
    let cb = cb as f32 - 128.0;
    let cr = cr as f32 - 128.0;

    let r = y + 1.40200 * cr;
    let g = y - 0.34414 * cb - 0.71414 * cr;
    let b = y + 1.77200 * cb;
    (
        clamp_to_u8((r + 0.5) as i32) as u8,
        clamp_to_u8((g + 0.5) as i32) as u8,
        clamp_to_u8((b + 0.5) as i32) as u8,
    )
}
fn clamp_to_u8(value: i32) -> i32 {
    let value = std::cmp::max(value, 0);
    std::cmp::min(value, 255)
}
/// Generate tables
#[inline]
const fn yuv_to_rgb_tables(y: i32, u: i32, v: i32) -> (i32, i32, i32, i32, i32) {
    let u = u - 128;
    let v = v - 128;
    // Minimum Supported Rust Version just went up to 1.46.0
    // where saturating integers were implemented
    let coeff_y = y;
    let coeff_rv = 91881 * v + 32768 >> 16;
    let coeff_gu = -22554 * u;
    let coeff_gv = -46802 * v;
    let coeff_bu = 116130 * u;
    (coeff_y, coeff_rv, coeff_gu, coeff_gv, coeff_bu)
}
const fn generate_tables() -> ([i32; 255], [i32; 255], [i32; 255], [i32; 255]) {
    let mut rv_table: [i32; 255] = [0; 255];
    let mut gu_table: [i32; 255] = [0; 255];
    let mut gv_table: [i32; 255] = [0; 255];
    let mut bu_table: [i32; 255] = [0; 255];
    let mut f = 0;
    while f != 255 {
        let pos = f as usize;
        let v = yuv_to_rgb_tables(f, f, f);
        rv_table[pos] = v.1;
        gu_table[pos] = v.2;
        gv_table[pos] = v.3;
        bu_table[pos] = v.4;
        f += 1;
    }
    (rv_table, gu_table, gv_table, bu_table)
}
const ALL_TABLES: ([i32; 255], [i32; 255], [i32; 255], [i32; 255]) = generate_tables();
const RV_TABLE: [i32; 255] = ALL_TABLES.0;
const GU_TABLE: [i32; 255] = ALL_TABLES.1;
const GV_TABLE: [i32; 255] = ALL_TABLES.2;
const BU_TABLE: [i32; 255] = ALL_TABLES.3;

/// YUV to RGB conversion using lookup tables
fn yuv_to_rgb2(y: u8, u: u8, v: u8) -> (u8, u8, u8) {
    let (y, u, v) = (y as usize, u as usize, v as usize);
    unsafe {
        // Okay why?
        // Saves us 4 cpu cycles because we know will never panic as u8 are between 0 and 255 and
        // our tables are between 0 and 255 so it will never be out of bounds( and because this is critical,
        // to decoding, it needs to be as fast as possible)[Leads to 5% improvement though]
        let r = (y as i32 + RV_TABLE.get_unchecked(v));
        let g = (y as i32 + GU_TABLE.get_unchecked(u) + GV_TABLE.get_unchecked(v) + 32768 >> 16);
        let b = (y as i32 + BU_TABLE.get_unchecked(u) + 32768 >> 16);
        return (clamp(r), clamp(g), clamp(b));
    }
}

/// Fastest clamp i could find
fn clamp(a: i32) -> u8 {
    (min(max(0, a), 255)) as u8
}
fn criterion_benchmark(c: &mut Criterion) {
    c.bench_function("YUV(fractions) to RGB", |b| {
        b.iter(|| ycbcr_to_rgb(black_box(69), black_box(69), black_box(76)))
    });
    c.bench_function("YUV(int) to RGB", |b| {
        b.iter(|| yuv_to_rgb(black_box(69), black_box(69), black_box(69)))
    });
    c.bench_function("YUV(tables) to RGB", |b| {
        b.iter(|| yuv_to_rgb2(black_box(69), black_box(69), black_box(69)))
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
