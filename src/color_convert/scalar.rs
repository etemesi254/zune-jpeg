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
    let (_, output_position) = output.split_at_mut(*pos);

    // Convert into a slice with 32 elements for Rust to FINALLY SEE WE WON'T GO OUT
    // OF BOUNDS
    let opt: &mut [u8; 64] = output_position
        .get_mut(0..64)
        .expect("Slice to small cannot write")
        .try_into()
        .unwrap();
    let mut p = 0;
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
    *pos += 64;
}

pub fn ycbcr_to_rgb_16_scalar(
    y: &[i16; 16], cb: &[i16; 16], cr: &[i16; 16], output: &mut [u8], pos: &mut usize,
)
{
    let mut p = 0;
    let (_, output_position) = output.split_at_mut(*pos);

    // Convert into a slice with 48 elements
    let opt: &mut [u8; 48] = output_position
        .get_mut(0..48)
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
    *pos += 48;
}

pub fn ycbcr_to_grayscale(y: &[i16], width: usize, output: &mut [u8])
{
    // Convert i16's to u8's
    let temp_output = y.iter().map(|x| *x as u8).collect::<Vec<u8>>();
    // chunk according to width.

    let width_mcu = y.len() / width;

    let width_chunk = y.len() / width_mcu;

    let mut start = 0;

    let mut end = width;

    for chunk in temp_output.chunks_exact(width_chunk)
    {
        // copy data, row wise, we do it row wise to discard fill bits if the
        // image has an uneven width not divisible by 8.

        output[start..end].copy_from_slice(&chunk[0..width]);
        start += width;
        end += width;
    }
}

/// Convert YcbCr to YCbCr
///
/// Basically all we do is remove fill bytes (if there) in the edges
pub fn ycbcr_to_ycbcr(
    channels: &[Vec<i16>; 3], width: usize, h_samp: usize, v_samp: usize, output: &mut [u8],
)
{
    // copy to a temporary vector.

    let mcu_chunks = channels[0].len() / (h_samp * v_samp);

    // pixels we write per width. since this is YcbCr we write
    // width times color components.
    let stride = width * 3;

    let mut start = 0;

    let mut end = width * 3;

    let addition = width * 3;

    // width which accounts number of fill bytes
    let width_chunk = mcu_chunks >> 3;
    // vector for temporary storage.
    let mut temp_output = vec![0; width_chunk * 3];

    for ((y_chunk, cb_chunk), cr_chunk) in channels[0]
        .chunks_exact(width_chunk)
        .zip(channels[1].chunks_exact(width_chunk))
        .zip(channels[2].chunks_exact(width_chunk))
    {
        // OPTIMIZE-TIP: Don't do loops in Rust, use iterators in such manners to ensure super
        // powers on optimization.
        // Using indexing will cause Rust to do bounds checking and prevent some cool optimization
        // options. See this  compiler-explorer link https://godbolt.org/z/Kh3M43hYr for what I mean.

        for (((y, cb), cr), out) in y_chunk
            .iter()
            .zip(cb_chunk.iter())
            .zip(cr_chunk.iter())
            .zip(temp_output.chunks_exact_mut(3))
        {
            out[0] = *y as u8;
            out[1] = *cb as u8;
            out[2] = *cr as u8;
        }

        output[start..end].copy_from_slice(&temp_output[0..stride]);

        start += addition;

        end += addition;
    }
}
