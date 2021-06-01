use crate::errors::DecodeErrors;
use crate::huffman::HuffmanTable;
use crate::huffman::FAST_BITS;
use crate::marker::Marker;
use crate::misc::{read_u8, UN_ZIGZAG};
use std::io::{BufReader, Read};

const BMASK: [u32; 17] = [
    0, 1, 3, 7, 15, 31, 63, 127, 255, 511, 1023, 2047, 4095, 8191, 16383, 32767, 65535,
];
// Entry n is (-1<<n) + 1
const BIAS: [i32; 16] = [
    0, -1, -3, -7, -15, -31, -63, -127, -255, -511, -1023, -2047, -4095, -8191, -16383, -32767,
];

//Todo: Maybe instead of panicking on a failed refill, we zero out the rest of the MCU's data to create
// a uniform grey in incomplete JPEG

// Total debugging hours:32

// @OPTIMIZE: Use a u64 as our bit buffer, cutting refills by half in the hot path

/// A bitStream reader
pub struct BitStream {
    // our bit buffer
    bits: u32,
    // number of bits remaining
    count: u8,
    // marker encountered when reading bitstream
    #[allow(dead_code)]
    marker: Option<Marker>,
}

impl BitStream {
    /// Create a new bit-stream reader
    pub const fn new() -> BitStream {
        BitStream {
            bits: 0,
            count: 0,
            marker: None,
        }
    }

    /// Refill the buffer
    #[inline]
    fn refill<R: Read>(&mut self, reader: &mut BufReader<R>) {
        // count cannot go more than 56 bits if we were to add one more bit it would
        // be 65,(more than bits can hold)

        while self.count <= 24 {
            // libjpeg-turbo unrolls this loop , see FILL_BUFFER_FAST function in jdhuff.c
            let byte = read_u8(reader);

            // Add 8 bits in MSB order.
            // Big Endian , top bits contain our stream, new bits added at the bottom.

            // pre-execute most common case
            self.bits |= u32::from(byte) << (24 - self.count);
            // increment count
            self.count += 8;

            // need to check if byte is 0xff since that is a special case
            if byte == 0xff {
                // read next byte
                let mut next_byte = read_u8(reader);
                // Byte snuffing, if we encounter byte snuff, we skip the byte
                if next_byte != 0x00 {
                    // Section B.1.1.2
                    // "Any marker may optionally be preceded by any number of fill bytes, which are bytes assigned code X’FF’."
                    while next_byte == 0xFF {
                        // consume fill bytes
                        next_byte = read_u8(reader);
                    }

                    if next_byte != 0x00 {
                        // if its a marker it indicates end of compressed data
                        // back out pre-execution and fill with zero bits
                        // we are removing 1 byte, te FF we added in pre-computation
                        self.bits &= !0xf;
                        self.count -= 8;

                        self.marker = Some(Marker::from_u8(next_byte).unwrap())
                    }
                    // if next_byte is zero we need to panic(jpeg-decoder does that)
                    // but i'll let it slide
                }
            }
        }
    }
    ///Returns the next few bits in the buffer, without discarding them
    #[inline]
    const fn peek_bits(&self, count: u8) -> usize {
        // we don't check if count is less than 16 since this is called after refill
        // we also don't cover zero cases since its called with constant FAST_BITS
        // which is a compile time constant

        // shift right by count and mask top bits by bitwise AND
        ((self.bits >> (32 - count)) & ((1 << count) - 1)) as usize
    }
    // Remove count bits from the stream without looking at them
    #[inline]
    fn consume_bits(&mut self, count: u8) {
        debug_assert!(
            count <= self.count,
            "Cannot consume more bits  than is available in the current buffer"
        );

        // shift up, removing top `count` bits
        self.bits <<= count;
        // remove count for bits consumed
        self.count -= count;
    }
    pub fn decode<R>(
        &mut self,
        huff_tbl: &HuffmanTable,
        stream: &mut BufReader<R>,
    ) -> Result<u8, DecodeErrors>
    where
        R: Read,
    {
        if self.count < 16 {
            self.refill(stream);
        }
        // look at the top FAST_BITS and determine what symbol ID it is,
        // if the code is <= FAST_BITS
        let mut c = self.peek_bits(FAST_BITS as u8);
        let k = huff_tbl.fast[c as usize];
        if k < 255 {
            let s = huff_tbl.size[k as usize];
            if s > self.count {
                // more bits requested than supported
                return Err(DecodeErrors::HuffmanDecode(format!(
                    "Could not decode Huffman value,requested {} bits while buffer has {} bits",
                    s, self.count
                )));
            }
            // consume already read bits
            self.consume_bits(s);

            return Ok(huff_tbl.values[k as usize]);
        }
        // naive test is to shift the code_buffer down so k bits are
        // valid, then test against maxcode. To speed this up, we've
        // pre-shifted maxcode left so that it has (16-k) 0s at the
        // end; in other words, regardless of the number of bits, it
        // wants to be compared against something shifted to have 16;
        // that way we don't need to shift inside the loop.
        let temp = self.bits >> 16;

        let mut k = FAST_BITS + 1;
        loop {
            if temp < huff_tbl.maxcode[k] {
                break;
            }
            k += 1;
        }

        if k == 17 {
            // Error code not found, revert the stream
            self.count -= 16;
            return Err(DecodeErrors::HuffmanDecode(
                "Cannot decode Huffman value more than 16 bits needed to decode value k"
                    .to_string(),
            ));
        }
        if k > self.count as usize {
            return Err(DecodeErrors::HuffmanDecode(
                "Could not decode Huffman value".to_string(),
            ));
        }
        // convert huffman code to symbol ID masking top `K` bits
        c = (((self.bits >> (32 - k)) & BMASK[k]) as i32 + huff_tbl.delta[k]) as usize;
        assert_eq!(
            ((self.bits >> (32 - huff_tbl.size[c])) & BMASK[huff_tbl.size[c] as usize]),
            u32::from(huff_tbl.code[c])
        );

        self.consume_bits(k as u8);

        Ok(huff_tbl.values[c])
    }
    #[inline]
    pub fn extend_receive<R: Read>(&mut self, n: u8, reader: &mut BufReader<R>) -> i16 {
        if self.count < n {
            // If we have less data in the buffer than requested refill

            // Refill may sometimes fail, it will panic
            self.refill::<R>(reader)
        }

        let (n, n_usize) = (i32::from(n), n as usize);
        // F.2.4.3.1.1 Decoding the sign
        // Decode the sign by checking the top MSb bit to see whether it is 1 or 0,
        // 1 => negative, 0 => positive

        // using two's complement bitwise not operator, if 0 it becomes 0 and if 1 becomes -1
        // see https://stackoverflow.com/questions/6719316/can-i-turn-negative-number-to-positive-with-bitwise-operations-in-actionscript-3/6719341
        let sign = !((self.bits >> 31) as i32) + 1;

        //Circular shifts, when a bit falls off one end of the register it fills
        // the vacated position at the other end
        // see https://blog.regehr.org/archives/1063

        let mut k = ((self.bits << n) | self.bits >> (-n & 31)) as i32;

        self.bits = (k & !((BMASK[n_usize]) as i32)) as u32;

        // zero out upper `n` bits
        k &= BMASK[n_usize] as i32;
        // subtract bits read;
        self.count -= n as u8;

        return (k + (BIAS[n_usize] as i32 & !sign)) as i16;
    }

    pub fn decode_ac<R: Read>(
        &mut self,
        stream: &mut BufReader<R>,
        huff_ac: &HuffmanTable,
        buffer: &mut [i16; 64],
    ) -> Result<(), DecodeErrors> {
        let mut k = 1;

        while k < 64 {
            if self.count < 16 {
                // refill buffer if we are low on bits

                // refill may sometimes fail, this is panicky code
                self.refill::<R>(stream);
            }

            // peek `FAST_COUNT` bits
            let c = self.peek_bits(FAST_BITS as u8);
            // was passing as an array causing move, huge bottleneck
            if let Some(fac) = &huff_ac.fast_ac {
                let r = fac[c];
                if r != 0 {
                    // FAST ac path

                    //Number of zero bits preceding the table
                    k += (r >> 4) & 15;

                    // combined length( the number of bits to read from the stream to decode this value)
                    let s = (r & 15) as u8;

                    self.consume_bits(s);

                    let zig = UN_ZIGZAG[k as usize];

                    buffer[zig] = r >> 8;

                    k += 1;
                } else {
                    let rs = self.decode(huff_ac, stream)?;
                    //  number of previous zeroes
                    let s = rs & 15;
                    // category bits
                    let r = rs >> 4;
                    if s == 0 {
                        // end of block
                        if rs != 0xf0 {
                            break;
                        }
                        k += 16;
                    } else {
                        k += (r) as i16;
                        let zig = UN_ZIGZAG[k as usize];
                        let p = self.extend_receive(s, stream);
                        buffer[zig] = p;
                        k += 1;
                    }
                }
            } else {
                return Err(DecodeErrors::Format("No Fast AC table found, this is a bug, please file an issue in the Github Page".to_string()));
            }
        }
        Ok(())
    }
}
