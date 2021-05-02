//! This file contains an implementation of decoding scan data in a file
use crate::bitstream::BitStreamReader;
use crate::threads::parse_threads_ycbcr;
use crate::JPEG;
use ndarray::{arr1, Array1, Array2, ArrayBase, OwnedRepr};
use std::cmp::min;
use zune_traits::image::Image;

impl JPEG {
    /// Decode the scan data
    ///
    /// So when you decode a stream of bits from the image in the JPG file, you'll do:
    ///
    ///-----------------------------------------------------------------------------------
    ///
    /// Init DC with 0.
    ///
    /// 1) First the DC coefficient decode :
    ///
    /// 	 a) Fetch a valid Huffman code (you check if it exists in the Huffman DC table)
    ///
    ///      b) See at what category this Huffman code corresponds
    ///
    ///      c) Fetch N = category bits  , and determine what value is represented
    ///
    ///             by (category, the N bits fetched) = Diff
    ///
    ///      d) DC + = Diff
    ///
    ///     e) write DC in the 64 vector :      `vector[0]=DC`
    ///
    /// -------------------------------------------------------------------------------
    /// 2) The 63 AC coefficients decode :
    ///
    /// FOR every AC coefficient UNTIL (EOB_encountered OR AC_counter=64)
    ///
    ///         a) Fetch a valid Huffman code (check in the AC Huffman table)
    ///
    ///         b) Decode that Huffman code : The Huffman code corresponds to
    ///
    ///                    (nr_of_previous_0,category)
    ///
    ///                  [Remember: EOB_encountered = TRUE if (nr_of_previous_0,category) = (0,0) ]
    ///
    ///        c) Fetch N = category bits, and determine what value is represented by
    ///
    ///               (category,the N bits fetched) = AC_coefficient
    ///
    ///        d) Write in the 64 vector, a number of zeroes = nr_of_previous_zero
    ///
    ///        e) increment the AC_counter with nr_of_previous_0
    ///
    ///        f) Write AC_coefficient in the vector:
    ///
    /// > >                   vector[AC_counter]=AC_coefficient
    ///-----------------------------------------
    /// Checkout  [CRYX's notes on decoding the JPEG image](https://www.opennet.ru/docs/formats/jpeg.txt)
    ///
    #[allow(
        clippy::similar_names,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    pub(crate) fn decode_scan_data_ycbcr(&mut self, scan_data: &[u8]) -> zune_traits::image::Image {
        let mut bits = BitStreamReader::from(scan_data);
        // Number of Minimum coded units
        let mcu_count = (u32::from(self.info.height) * u32::from(self.info.width)) / 64;
        debug!("Minimum Coded Units:{}", mcu_count);
        //let mut pixels:Vec<u8> = Vec::with_capacity((self.dimensions() * 3) as usize);

        // Bits scanned from each bit, putting this here to prevent re-initialization for every mcu
        // count
        let mut dc_bits_scanned = String::with_capacity(20);
        let mut ac_bits_scanned = String::with_capacity(20);

        let mut all_channels = Vec::with_capacity(mcu_count as usize);

        // Prevent re-initialization every time
        let pre_channel = Array1::zeros(64);

        for _ in 0..mcu_count {
            // Check whether we use luminance table or chrominance table
            let mut chanellzz = [arr1(&[]), arr1(&[]), arr1(&[])];
            let mut luminance = true;
            for channel in 0..3 {
                let mut array = pre_channel.clone();
                // Cb and Cr tables use same table while luminance has it's own
                let (dc_table, ac_table) = {
                    if luminance {
                        luminance = false;
                        (&self.dc_huffman_tables[0], &self.ac_huffman_tables[0])
                    } else {
                        (&self.dc_huffman_tables[1], &self.ac_huffman_tables[1])
                    }
                };
                // Get DC Coefficient
                'dc: while let Some(bit) = bits.read() {
                    dc_bits_scanned.push_str(bit.as_str());
                    if let Some(value) = dc_table.lookup(&dc_bits_scanned) {
                        let value = *value as usize;

                        let dc_bits = bits.read_n(value);

                        let number = get_signed_number_from_bits(dc_bits, value);
                        // First coefficient is a DC coefficient
                        array[0] = f64::from(number);

                        // clear dc bits
                        dc_bits_scanned.clear();

                        break 'dc;
                    }
                }
                let mut counter: usize = 1;

                // FOR every AC coefficient UNTIL (EOB_encountered OR AC_counter=64)
                'ac: while counter < 64 {
                    if let Some(bit) = bits.read() {
                        ac_bits_scanned.push_str(&bit);
                        //a) Fetch a valid Huffman code (check in the AC Huffman table)
                        if let Some(next_code) = ac_table.lookup(&ac_bits_scanned) {
                            let value = *next_code as usize;
                            //[Remember: EOB_encountered = TRUE if (nr_of_previous_0,category) = (0,0) ]

                            if value == 0x00 {
                                // Fill rest of block with zeroes
                                // Since its already zero, we can break early
                                ac_bits_scanned.clear();
                                break 'ac;
                            }

                            //b) Decode that Huffman code : The Huffman code corresponds to
                            //     (previous_zeroes[4 first bits],category[4 last bits])

                            // Get the first 4 bits
                            let mut previous_zeros = ((next_code & 0xf0) >> 4) as usize;

                            // 15 is a special number,means 16 zeroes
                            if previous_zeros == 15 {
                                previous_zeros = 16
                            }

                            // Get last 4 bits which represent number of bits to read
                            let num_bits = (next_code & 0xf) as usize;

                            //c) Fetch N = category bits, and determine what value is represented by
                            //            (category,the N bits fetched) = AC_coefficient
                            let ac_bits = bits.read_n(num_bits);

                            let num = get_signed_number_from_bits(ac_bits, num_bits);
                            //e) increment the AC_counter with previous_zeroes
                            counter += min(previous_zeros, 64 - counter - 1);

                            //f) Write AC_coefficient in the vector:
                            array[counter] = f64::from(num);
                            //g) Increment counter
                            counter += 1;
                            // clear the bits
                            ac_bits_scanned.clear()
                        }
                    }
                }
                chanellzz[channel] = array;
            }
            all_channels.push(chanellzz);
        }
        // go to thread.rs
        let small_mcu = parse_threads_ycbcr(all_channels, &self.qt_tables[0], &self.qt_tables[1]);

        let arrays = create_large_array(small_mcu,self.info.width,self.info.height);
        Image::from(arrays)
    }
}
fn create_large_array(mcu: Vec<[Array2<u8>; 3]>, width: u16, height: u16) -> [Array2<u8>; 3] {
    let mut red_channel = Array2::zeros((width as usize, height as usize));
    let mut green_channel = red_channel.clone();
    let mut blue_channel = red_channel.clone();
    // width and height of each block
    let mcu_width = (width / 8) as usize;
    let mcu_height = (height / 8) as usize;
    let mut pos = 0;
    // we need to move left until we reach the end of the row
    // then move one column down and repeat
    for i in 0..mcu_width {
        let p = i * 8;
        for j in 0..mcu_height {
            let q = j * 8;
            let mut r_slice = red_channel.slice_mut(s![p..p + 8, q..q + 8]);
            r_slice.assign( &mcu[pos][0]);

            let mut g_slice = green_channel.slice_mut(s![p..p + 8, q..q + 8]);
            g_slice.assign(&mcu[pos][1]);

            let mut b_slice = blue_channel.slice_mut(s![p..p + 8, q..q + 8]);
            b_slice.assign(&mcu[pos][2]);
            pos+=1;
        }
    }
    [red_channel,green_channel,blue_channel]
}
/// Get the signed representation from bits
///
/// Bits may look like `[0,0,0,1,0,0,0]` and to get it's signed representation
/// we check first for the first bit if it's zero, if it is means we flip all `0`'s to `1`s and `1`'s
/// to zeroes so the above bits will be processed as follows
///
/// - Check first bit , `first_bit` = `0`,
/// - Flip zeros to ones , `array` = `[1,1,1,0,1,1,1]`
/// - Create an i32 number from the bits  = `119`
/// - Negate number by multiplying it by `-1` , number =`-119`
///
/// But under the hood we do it differently
///
/// # Options
///    * bits - `&[u8]` references of `0'`s and `1'`s
///    * value - length of the above  `bits` argument
///
/// Example
/// ```
/// let bits = get_signed_number_from_bits(&[1,0,1],3);
/// assert_eq!(bits,5);
/// ```
#[inline]
#[allow(clippy::if_same_then_else)]
fn get_signed_number_from_bits(bits: &[u8], len: usize) -> i32 {
    let mut number = 0;
    let negative = {
        // If first bit is 0 we multiply result by -1 otherwise use 1
        bits.first()
            .map_or(1, |first_bit| if first_bit == &0 { -1 } else { 1 })
    };
    bits.iter().enumerate().for_each(|(pos, a)| {
        // Hello bit-shifts...

        // If number is zero and we expect a negative number, flip 0 to 1(two's complement)
        // eg (0) 000 000 would become 111 111
        if a == &0 && negative == -1 {
            number |= 1 << (len - pos - 1);
        }
        //If number is 1 and we expect a positive number, leave as is
        // eg (1) 001 001 remains 001 001
        else if a == &1 && negative == 1 {
            number |= 1 << (len - pos - 1);
        }
    });

    // multiply by -1 or 1 depending on the first digit
    number *= negative;

    return number;
}
