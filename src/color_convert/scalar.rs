use std::cmp::{max, min};
use std::convert::TryInto;

/// Limit values to 0 and 255
#[inline]
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, dead_code)]
fn clamp(a: i16) -> u8
{
    min(max(a, 0), 255) as u8
}

pub fn ycbcr_to_rgb_scalar(
    y: &[i16; 8], cb: &[i16; 8], cr: &[i16; 8], output: &mut [u8], pos: &mut usize,
)
{
    let mut p = 0;

    //Okay Rust sucks with this bound's checking.
    // To get it to vectorize this  we need the below lines of code

    let (_, output_position) = output.split_at_mut(*pos);

    // Convert into a slice with 24 elements for Rust to FINALLY SEE WE WON'T GO OUT
    // OF BOUNDS
    let opt: &mut [u8; 24] = output_position
        .get_mut(0..24)
        .expect("Slice to small cannot write")
        .try_into()
        .unwrap();

    for (y, (cb, cr)) in y.iter().zip(cb.iter().zip(cr.iter()))
    {
        let cr = cr - 128;

        let cb = cb - 128;

        let r = y + ((45 * cr) >> 5);

        let g = y - ((11 * cb + 23 * cr) >> 5);

        let b = y + ((113 * cb) >> 6);

        opt[p] = clamp(r);

        opt[p + 1] = clamp(g);

        opt[p + 2] = clamp(b);

        p += 3;
    }

    // Increment pos
    *pos += 24;
}

pub fn ycbcr_to_rgba_scalar(
    y: &[i16; 8], cb: &[i16; 8], cr: &[i16; 8], output: &mut [u8], pos: &mut usize,
)
{
    let mut p = 0;

    //Okay Rust sucks with this bound's checking.
    // To get it to vectorize this  we need the below lines of code

    let (_, output_position) = output.split_at_mut(*pos);

    // Convert into a slice with 32 elements for Rust to FINALLY SEE WE WON'T GO OUT
    // OF BOUNDS
    let opt: &mut [u8; 32] = output_position
        .get_mut(0..32)
        .expect("Slice to small cannot write")
        .try_into()
        .unwrap();

    for (y, (cb, cr)) in y.iter().zip(cb.iter().zip(cr.iter()))
    {
        let cr = cr - 128;

        let cb = cb - 128;

        let r = y + ((45 * cr) >> 5);

        let g = y - ((11 * cb + 23 * cr) >> 5);

        let b = y + ((113 * cb) >> 6);

        opt[p] = clamp(r);

        opt[p + 1] = clamp(g);

        opt[p + 2] = clamp(b);

        opt[p + 3] = 255;

        p += 4;
    }

    // Increment pos
    *pos += 32;
}

/// YcbCr to RGBA color conversion

pub fn ycbcr_to_rgba_16_scalar(
    y: &[i16; 16], cb: &[i16; 16], cr: &[i16; 16], output: &mut [u8], pos: &mut usize,
)
{
    // first mcu
    ycbcr_to_rgba_scalar(
        y[0..8].try_into().unwrap(),
        cb[0..8].try_into().unwrap(),
        cr[0..8].try_into().unwrap(),
        output,
        pos,
    );

    // second MCU
    ycbcr_to_rgba_scalar(
        y[8..16].try_into().unwrap(),
        cb[8..16].try_into().unwrap(),
        cr[8..16].try_into().unwrap(),
        output,
        pos,
    );
}

pub fn ycbcr_to_rgb_16_scalar(
    y: &[i16; 16], cb: &[i16; 16], cr: &[i16; 16], output: &mut [u8], pos: &mut usize,
)
{
    // first mcu
    ycbcr_to_rgb_scalar(
        y[0..8].try_into().unwrap(),
        cb[0..8].try_into().unwrap(),
        cr[0..8].try_into().unwrap(),
        output,
        pos,
    );

    // second MCU
    ycbcr_to_rgb_scalar(
        y[8..16].try_into().unwrap(),
        cb[8..16].try_into().unwrap(),
        cr[8..16].try_into().unwrap(),
        output,
        pos,
    );
}
