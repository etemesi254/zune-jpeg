#![cfg(feature="x86")]

#[cfg(target_arch = "x86")]
use std::arch::x86::*;
#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::*;


use crate::unsafe_utils::align_alloc;
use crate::upsampler::scalar::upsample_hv;

pub fn upsample_hv_simd(input:&[i16],output_len:usize)->Vec<i16>{
    if input.len() < 500 {
        return upsample_hv(input, output_len);
    }
    unsafe {
        upsample_hv_avx(input,output_len)
    }
}
// herein lies 2 days work tread lightly

#[target_feature(enable = "avx")]
//Some things are weird...
#[target_feature(enable = "avx2")]
#[inline]
pub unsafe fn upsample_hv_avx(input: &[i16], output_len: usize) -> Vec<i16> {
    //to understand how it works, we have to see what vertical upsampling + horizontal upsampling do
    // Hence the illustration
    //    | A1 | A2 |
    //    |====|====|
    //    | p1 | p2 |
    //    |----|----|
    //    | p3 | p4 |
    //    |====|====|
    //    | B1 | B2 |
    //
    // -> Okau for two squeezed lines, A and B we generate 2 lines.
    // We do the same for B & C and so on.
    //
    // After our vertical pass, we have p2 and p3 stretching halfway the image
    //
    // After that vertical pass, we want to do another pass.
    // | p1 |q1|q2| p2 |
    // |----|=====|----|
    // | p3 |q3|q4| p4 |
    //
    // now q1 will stretch the whole image, hence it becomes a full upsampling

    // So how do we actually implement.



    let mut output = align_alloc::<i16, 16>(output_len);

    // okay for the next step lets play with AVX

    // Difference in  pixels for a width
    let mut stride = 0;
    //
    let mut modify_stride = true;

    let mut pos = 0;
    let mut output_position = 0;

    let mut prev = (3 * input[0] + input[stride] + 2) >> 2;
    let mut pixel_far = (3 * input[pos + 16] + input[pos + stride + 16] + 2) >> 2;

    let three = _mm256_set1_epi16(3);
    let two = _mm256_set1_epi16(2);

    macro_rules! pack_shuffle {
        ($x:tt,$y:tt,$value:tt) => {
            let v = _mm256_permute2x128_si256::<$value>($x, $x);

            let rwn_hi = _mm256_unpackhi_epi16(v, v);
            let rwn_lo = _mm256_unpacklo_epi16(v, v);

            $y = _mm256_permute2x128_si256::<0b0011_0000>(rwn_lo, rwn_hi);
        };
    }

    macro_rules! upsample_horizontal {
        ($row:tt,$stride:tt) => {
            // AND THE implementation of srli is 2 128 bit shuffles.
            // Mahn intel really??
            // Therefore we need to emulate that instruction
            // See this stack overflow post: https://stackoverflow.com/questions/25248766/emulating-shifts-on-32-bytes-with-avx

            // shift the whole data in the register left by 16 to remove zero-th element
            // array looks like B0,[B1,B2,B3,B4,B5,B6,B7,0]
            // _mm256_srli_si256(A, N) (0<N<16)
            let mut next_arr = _mm256_alignr_epi8::<2>(
                _mm256_permute2x128_si256::<{ shuffle(2, 0, 0, 1) }>($row, $row),
                $row,
            );
            // shift the whole data in the array down by(right) by 16, to remove n+1.
            // Now our register looks like  [0,B1,B2,B3,B4,B5,B6],B7.
            // _mm256_slli_si256(A, N) ( 0< N < 16).
            let mut prev_arr = _mm256_alignr_epi8::<{ 16 - 2 }>($row,
                                                                _mm256_permute2x128_si256::<{ shuffle(0, 0, 2, 0) }>($row, $row),
            );


            // insert values

            // prev_array => [X0,B1,B2,B3,B4,B5,B6,B7]
            prev_arr = _mm256_insert_epi16::<0>(prev_arr, prev);
            // NEXT_ARR => [B0,B1,B2,B3,B4,B5,B6,X1]
            next_arr = _mm256_insert_epi16::<15>(next_arr, pixel_far);

            // Okay at this point we have: (the arrows just show how prev_arr & next_arr differ from row_near)
            //              |                                                  |
            //             \/                                                 \/
            //-> prev_arr [B0,B1,B2,B3,B4,B5,B6,B7,B8,B9,B10,B11,B12,B13,B14,B15]
            //
            //
            //-> row_near [B1,B2,B3,B4,B5,B6,B7,B8,B9,B10,B11,B2,B13,B14,B15,B16]
            //
            //              |                                                |
            //             \/                                               \/
            //-> next_row [B2,B3,B4,B5,B6,B7,B8,B9,B10,B11,B12,B13,B14,B15,B16]


            // Next we need to get values in Avx register and stretch them.
            // The reason for this is simple, currently our avx register contain vertical upsampling values.
            // packed together, for a packing B0,B1 , we need to generate two values
            //illustrated by
            // |  |           |  |
            // |B0| |p1| |p2| |B1|
            // |  |           |  |
            // Now a cool way to stretch them is to half our three avx registers,  into 6 upper or lower registers
            // and duplicate elements of either the half register to the other half

            // Eg for near:
            // near->|B0,B1,B2,B3]
            // near_hi ->[B0,B0,B1,B1]
            // near_lo ->[B2,B2,B3,B3]

            // Duplicate top elements to low elements
            let (near_lo, near_hi, prev_lo, prev_hi, next_lo, next_hi);

            pack_shuffle!($row,near_lo,0);
            pack_shuffle!($row,near_hi,0b011_0011);

            pack_shuffle!(prev_arr,prev_lo,0);
            pack_shuffle!(prev_arr,prev_hi,0b011_0011);

            pack_shuffle!(next_arr,next_lo,0);
            pack_shuffle!(next_arr,next_hi,0b011_0011);

            // FINALLY START CARRYING OUT THE CALCULATIONS.

            // everything above was set up for this
            // it plays out soo nicely it makes me want to cry at this marvel.


            // prev-lo->[BO,B0,B1,B1,B2,B2,B3,B3]
            // next-lo->[B2,B2,B3,B3,B4,B4,B5,B5]
            // NN    -> [B0,B2,B1,B3,B2,B4,B3,B5]

            // notice how prettily NN contains  a weird shift of elements.

            // for i in 1..input.len() - 1{
            //         let sample = 3 * input[i] + 2;
            //
            //         out[i * 2] = (sample + input[i - 1]) >> 2;
            //         out[i * 2 + 1] = (sample + input[i + 1]) >> 2;
            // }

            // Look at the ABOVE loop, notice how even numbers get i-1, look at the even indices
            // in nn, LOOK AT THEM.
            // Look at the ODD INDICES while at it.

            // And that's the miracle.
            // I rest my case, may you understand the rest of the code.

            let nn = _mm256_blend_epi16::<0b1010_1010>(prev_lo, next_lo);
            // input[x]*3
            let an = _mm256_mullo_epi16(near_lo, three);

            let bn = _mm256_add_epi16(nn, two);

            let cn = _mm256_srai_epi16::<2>(_mm256_add_epi16(an, bn));

            _mm256_storeu_si256(output.get_unchecked_mut(output_position+$stride..).as_mut_ptr().cast(), cn);


            let nn = _mm256_blend_epi16::<0b1010_1010>(prev_hi, next_hi);

            let an = _mm256_mullo_epi16(near_hi, three);

            let bn = _mm256_add_epi16(nn, two);

            let cn = _mm256_srai_epi16::<2>(_mm256_add_epi16(an, bn));
            // hi is stored one output width lower.
            _mm256_storeu_si256(output.get_unchecked_mut(output_position+$stride+16..).as_mut_ptr().cast(), cn);


        };
    }

    macro_rules! upsample_vertical {
        ($load_near:tt,$load_far:tt,$row_near:tt,$row_far:tt) => {
            let t2 = _mm256_add_epi16($load_far, two);
            let t1 = _mm256_mullo_epi16($load_near, three);

            // (near * 3 + far + 2 ) >> 2
            $row_near = _mm256_srai_epi16::<2>(_mm256_add_epi16(t1, t2));

            let t3 = _mm256_add_epi16($load_near, two);
            let t4 = _mm256_mullo_epi16($load_far, three);

            // (far * 3 + near + 2 ) >> 2
            $row_far = _mm256_srai_epi16::<2>(_mm256_add_epi16(t3, t4));
        };
    }

    let (mut row_near, mut row_far);

    let len = output_len / 16;
    let end = (input.len() >> 7) - 1;
    // some calculations to determine how much will not be written per MCU.
    let v = (output.len() / 16) - (end * 32);

    // let mut scalar_temp = vec![];

    // we need to iterate row wise
    // we initially have 8 rows we want to make 32, new rows,
    for j in 0..8 {
        // loop row wise
        // we are feed it our stuff with 16 values, therefore we can loop the number of times we can
        // chunk data. into 16
        for _ in 0..end {
            let load_near = _mm256_loadu_si256(input[pos..].as_ptr().cast());
            let load_far = _mm256_loadu_si256(input[pos + stride..].as_ptr().cast());
            // ==========================================================================
            // -> Pass 1:Vertical calculation.
            // --------------------------------------------------------------------------
            upsample_vertical!(load_near, load_far, row_near, row_far);

            // --------------------------------------------------------------------------
            // Pass 1 : Complete
            // ==========================================================================

            // ==========================================================================
            // Pass 2: Horizontal upsampling
            //---------------------------------------------------------------------------

            upsample_horizontal!(row_near, 0);

            upsample_horizontal!(row_far, (len));

            output_position += 32;
            // ---------------------------------------------------------------------------
            // pass 2 complete
            // ===========================================================================
            pos += 16;
            // update previous and next values
            assert!(input.len() >= pos + stride + 16);
            // Safety , the assert statement above ensures none of these goes out of bounds.
            prev =
                (3 * (*input.get_unchecked(pos)) + (*input.get_unchecked(pos + stride)) + 2) >> 2;
            pixel_far = (3 * (*input.get_unchecked(pos + 16))
                + (*input.get_unchecked(pos + stride + 16))
                + 2)
                >> 2;
        }
        // now there are some more data left, since we actually don't write
        // to the end of the array.
        // those whe handle using scalar code.

        let remainder = &input[pos..pos + (v / 2)];
        let remainder_stride = &input[pos + stride..pos + (v / 2) + stride];

        // simple horizontal filter
        for (window_near, window_far) in remainder.windows(2).zip(remainder_stride.windows(2)) {
            output[output_position] = (3 * window_near[0] + window_near[1] + 2) >> 2;
            output[output_position + 1] = (3 * window_near[1] + window_near[0] + 2) >> 2;
            // do the same for stride
            output[output_position + len] = (3 * window_far[0] + window_far[1] + 2) >> 2;
            output[output_position + len + 1] = (3 * window_far[1] + window_far[0] + 2) >> 2;

            pos += 1;
            output_position += 2;
        }

        output_position += len + 2;
        if modify_stride {
            stride = input.len() / 8;
            modify_stride = false;
        }
        if j == 6 {
            // when j is 6 it means we are at position 14, and 15, here we
            // need to revert stride to be zero to get a nearest neighbour

            // since there is no more row to look ahead using stride.
            stride = 0;
        }
        //  pos+=1;

        output[output_position - len] = output[output_position - len + 1];

        output[output_position - len - 2] = output[output_position - len - 4];

        output[output_position - len - 1] = output[output_position - len - 3];

        output[output_position - 2] = output[output_position - 4];

        output[output_position - 1] = output[output_position - 3];
    }

    return output;
}

#[inline]
const fn shuffle(z: i32, y: i32, x: i32, w: i32) -> i32 {
    ((z << 6) | (y << 4) | (x << 2) | w) as i32
}

