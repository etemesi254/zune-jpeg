#![allow(
    clippy::if_not_else,
    clippy::similar_names,
    clippy::inline_always,
    clippy::doc_markdown
)]
#![allow(dead_code)]
//! This file exposes a single struct that can decode a huffman encoded
//! Bitstream in a JPEG file
//!
//! This code is optimized for speed.
//! It's meant to be super duper super fast, because everyone else depends on this being fast.
//! It's (annoyingly) serial hence we cant use parallel bitstreams(it's variable length coding..)
//!
//! Furthermore, on the case of refills, we have to do bytewise processing because the standard decided
//! that we want to support markers in the middle of streams(seriously few people use RST markers).
//!
//! So we pull in all optimization steps, use `inline[always]`? ✅ ,pre-execute most common cases ✅,
//! add random comments ✅, fast paths ✅.
//!
//! Readability comes as a second priority(I tried with variable names this time, and we are wayy better than libjpeg).
//!
//! Anyway if you are reading this it means your cool and I hope you get whatever part of the code you are looking for
//! (or learn something cool)
//!
//! Knock yourself out.
use std::cmp::min;
use std::io::Cursor;

use crate::errors::DecodeErrors;
use crate::huffman::{HuffmanTable, HUFF_LOOKAHEAD};
use crate::marker::Marker;
use crate::misc::UN_ZIGZAG;

/// A `BitStream` struct, capable of decoding compressed data from the data from
/// image

pub(crate) struct BitStream
{
    /// A MSB type buffer that is used for some certain operations
    buffer: u64,
    /// A LSB type buffer that is used to accelerate some operations like
    /// peek_bits and get_bits.
    lsb_buffer: u64,
    /// Tell us the bits left the two buffer
    bits_left: u8,
    /// Did we find a marker(RST/EOF) during decoding?
    pub marker: Option<Marker>,

    /// Progressive decoding
    pub successive_high: u8,
    pub successive_low: u8,
    _spec_start: u8,
    spec_end: u8,
}

impl BitStream
{
    /// Create a new BitStream
    pub(crate) const fn new() -> BitStream
    {
        BitStream {
            buffer: 0,
            lsb_buffer: 0,
            bits_left: 0,
            marker: None,
            successive_high: 0,
            successive_low: 0,
            _spec_start: 0,
            spec_end: 0,
        }
    }

    /// Create a new Bitstream for progressive decoding

    pub(crate) fn new_progressive(ah: u8, al: u8, spec_start: u8, spec_end: u8) -> BitStream
    {
        BitStream {
            buffer: 0,
            lsb_buffer: 0,
            bits_left: 0,
            marker: None,
            successive_high: ah,
            successive_low: al,
            _spec_start: spec_start,
            spec_end,
        }
    }

    /// Refill the bit buffer by (a maximum of) 48 bits (4 bytes)
    ///
    /// # Arguments
    ///  - `reader`:`&mut BufReader<R>`: A mutable reference to an underlying
    ///    File/Memory buffer containing a valid JPEG stream
    ///
    /// This function will only refill if `self.count` is less than 32
    #[inline(always)]
    fn refill(&mut self, reader: &mut Cursor<Vec<u8>>) -> bool
    {
        // Ps i know inline[always] is frowned upon

        /// Macro version of a single byte refill.
        /// Arguments
        /// buffer-> our io buffer, because rust macros cannot get values from
        /// the surrounding environment bits_left-> number of bits left
        /// to full refill

        macro_rules! refill {
            ($buffer:expr,$byte:expr,$bits_left:expr) => {
                // read a byte from the stream
                $byte = read_u8(reader);

                // append to the buffer
                // JPEG is a MSB type buffer so that means we append this
                // to the lower end (0..8) of the buffer and push the rest bits above..
                $buffer = ($buffer << 8) | $byte;

                // Increment bits left
                $bits_left += 8;

                // Check for special case  of OxFF, to see if it's a stream or a marker
                if $byte == 0xff
                {
                    // read next byte
                    let mut next_byte = read_u8(reader);

                    // Byte snuffing, if we encounter byte snuff, we skip the byte
                    if next_byte != 0x00
                    {
                        // skip that byte we read
                        while next_byte == 0xFF
                        {
                            next_byte = read_u8(reader);
                        }

                        if next_byte != 0x00
                        {
                            // Undo the byte append and return
                            $buffer &= !0xf;

                            //  $lsb_byte <<= 8;
                            $bits_left -= 8;

                            self.marker = Some(Marker::from_u8(next_byte as u8).unwrap());

                            // if we get a marker, we return immediately, and hope
                            // that the bits stored are enough to finish MCU decoding with no hassle
                            return false;
                        }
                    }
                }
            };
        }

        // 32 bits is enough for a decode(16 bits) and receive_extend(max 16 bits)
        // If we have less than 32 bits we refill
        if self.bits_left <= 32
        {
            // This serves two reasons,
            // 1: Make clippy shut up
            // 2: Favour register reuse
            let mut byte;

            // 4 refills, if all succeed the stream should contain enough bits to decode a
            // value
            refill!(self.buffer, byte, self.bits_left);

            refill!(self.buffer, byte, self.bits_left);

            refill!(self.buffer, byte, self.bits_left);

            refill!(self.buffer, byte, self.bits_left);

            // Construct an MSB buffer whose top bits are the bitstream we are currently
            // holding.
            self.lsb_buffer = self.buffer << (64 - self.bits_left);
        }

        return true;
    }

    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::unwrap_used
    )]
    #[inline(always)]
    fn decode_dc(
        &mut self, reader: &mut Cursor<Vec<u8>>, dc_table: &HuffmanTable, dc_prediction: &mut i32,
    ) -> bool
    {
        let (mut symbol, mut code_length, r);

        // in the instance that refill returns false,
        // it means a marker was found in the stream, stop execution..
        if !self.refill(reader)
        {
            return false;
        };
        // look a head HUFF_LOOKAHEAD bits into the bitstream
        symbol = self.peek_bits::<HUFF_LOOKAHEAD>();

        symbol = dc_table.lookup[symbol as usize];

        // Extract code length of the DC coefficient
        code_length = symbol >> HUFF_LOOKAHEAD;

        // Drop bits from the bitstream.
        self.drop_bits(code_length as u8);

        // Get symbol for the DC coefficient.
        symbol &= (1 << HUFF_LOOKAHEAD) - 1;

        if code_length > i32::from(HUFF_LOOKAHEAD)
        {
            // If code length is greater than HUFF_LOOKAHEAD, read in bits the hard way way

            // Read the bits we initially discarded when we called drop_bits.
            symbol = ((self.buffer >> self.bits_left) & ((1 << (code_length)) - 1)) as i32;

            while symbol > dc_table.maxcode[code_length as usize]
            {
                symbol <<= 1;

                symbol |= self.get_bits(1);

                code_length += 1;
            }

            if code_length > 16
            {
                // corrupt image?
                symbol = 0;
            }
            else
            {
                symbol = i32::from(
                    dc_table.values
                        [((symbol + dc_table.offset[code_length as usize]) & 0xFF) as usize],
                );
            }
        }
        if symbol != 0
        {
            r = self.get_bits(symbol as u8);

            symbol = huff_extend(r, symbol);
        }

        // Update DC prediction
        *dc_prediction += symbol;

        return true;
    }

    /// Decode a Minimum Code Unit(MCU) as quickly as possible
    ///
    /// # Arguments
    /// - reader: The bitstream from where we read more bits.
    /// - dc_table: The Huffman table used to decode the DC coefficient
    /// - ac_table: The Huffman table used to decode AC values
    /// - block: A memory region where we will write out the decoded values
    /// - DC prediction: Last DC value for this component
    ///
    #[allow(
        clippy::many_single_char_names,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    #[rustfmt::skip]
    #[inline(always)]
    pub fn decode_mcu_block(
        &mut self,
        reader: &mut Cursor<Vec<u8>>,
        dc_table: &HuffmanTable,
        ac_table: &HuffmanTable,
        block: &mut [i16; 64],
        dc_prediction: &mut i32,

    ) -> bool
    {
        // decode DC, dc prediction will contain the value
        if !self.decode_dc(reader, dc_table, dc_prediction)
        {
            return false;
        }
        // set dc to be the dc prediction.
        block[0] = *dc_prediction as i16;

        let (mut symbol, mut code_length, mut r);
        // Decode AC coefficients
        let mut pos: usize = 1;
        // Get fast AC table as a reference before we enter the hot path
        let ac_lookup = ac_table.ac_lookup.as_ref().unwrap();

        while pos < 64
        {
            if !self.refill(reader)
            {
                // found a marker , stop processing
                return false;
            };

            symbol = self.peek_bits::<HUFF_LOOKAHEAD>();

            let fast_ac = ac_lookup[symbol as usize];

            if fast_ac != 0
            {
                //  FAST AC path

                // run
                pos += ((fast_ac >> 4) & 63) as usize;

                // Value

                // The `& 63` is to remove a  branch, i.e keep it between 0 and 63 because Rust can't
                // see that un-zig-zag returns values less than 63
                // See https://godbolt.org/z/zrbe6qcPf
                block[UN_ZIGZAG[min(pos,63)] & 63] = fast_ac >> 10;

                // combined length
                self.drop_bits((fast_ac & 15) as u8);

                pos += 1;
            }
            else
            {
                symbol = ac_table.lookup[symbol as usize];

                code_length = symbol >> HUFF_LOOKAHEAD;

                symbol &= (1 << HUFF_LOOKAHEAD) - 1;

                self.drop_bits(code_length as u8);

                if code_length > i32::from(HUFF_LOOKAHEAD)
                {
                    symbol = ((self.buffer >> self.bits_left) & ((1 << (code_length)) - 1)) as i32;

                    while symbol > ac_table.maxcode[code_length as usize]
                    {
                        symbol <<= 1;

                        symbol |= self.get_bits(1);

                        code_length += 1;
                    }
                    if code_length > 16
                    {
                        symbol = 0;
                    }
                    else
                    {
                        symbol = i32::from(
                            ac_table.values[((symbol + ac_table.offset[code_length as usize])
                                & 0xFF) as usize],
                        );
                    }
                };

                r = symbol >> 4;
                symbol &= 15;

                if symbol != 0
                {
                    pos += r as usize;

                    r = self.get_bits(symbol as u8);

                    symbol = huff_extend(r, symbol);

                    block[UN_ZIGZAG[pos as usize] & 63] = symbol as i16;

                    pos += 1;
                }
                else
                {
                    if r != 15
                    {
                        return true;
                    }
                    pos += 15;
                }
            }
        }
        return true;
    }

    /// Peek `look_ahead` bits ahead without discarding them from the buffer
    #[inline(always)]
    #[allow(clippy::cast_possible_truncation)]
    const fn peek_bits<const LOOKAHEAD: u8>(&self) -> i32
    {
        // for the LSB buffer peek bits doesn't require an and to remove/zero out top
        // bits
        (self.lsb_buffer >> (64 - LOOKAHEAD)) as i32
    }

    /// Discard the next `N` bits without checking
    #[inline]
    fn drop_bits(&mut self, n: u8)
    {
        self.bits_left -= n;

        // remove top n bits  in lsb buffer
        self.lsb_buffer <<= n;
    }

    /// Read `n_bits` from the buffer  and discard them
    #[inline(always)]
    #[allow(clippy::cast_possible_truncation)]
    fn get_bits(&mut self, n_bits: u8) -> i32
    {
        let bits = (self.lsb_buffer >> (64 - n_bits)) as i32;

        // Reduce the bits left, this influences the MSB buffer
        self.bits_left -= n_bits;

        // shift out bits read in the LSB buffer
        self.lsb_buffer <<= n_bits;

        bits
    }

    /// Decode a DC block
    #[allow(clippy::cast_possible_truncation)]
    pub fn decode_block_dc(
        &mut self, reader: &mut Cursor<Vec<u8>>, dc_table: &HuffmanTable, block: &mut [i16; 64],
        dc_prediction: &mut i32,
    ) -> Result<bool, DecodeErrors>
    {
        if self.spec_end == 0
        {
            return Err(DecodeErrors::HuffmanDecode(
                "Can't merge dc and AC corrupt jpeg".to_string(),
            ));
        }

        if self.successive_high == 0
        {
            self.decode_dc(reader, dc_table, dc_prediction);

            block[0] = (*dc_prediction as i16) * (1_i16 << self.successive_low);
        }
        else
        {
            // refinement scan
            self.get_bit(reader);

            block[0] += 1 << self.successive_low;
        }

        return Ok(true);
    }

    /// Get a single bit from the bitstream

    fn get_bit(&mut self, reader: &mut Cursor<Vec<u8>>) -> bool
    {
        if self.bits_left < 1
        {
            return self.refill(reader);
        }

        // discard a bit
        self.bits_left -= 1;

        self.lsb_buffer <<= 1;

        return true;
    }

    /// Reset the stream if we have a restart marker
    ///
    /// Restart markers indicate drop those bits in the stream and zero out
    /// everything
    #[cold]
    pub fn reset(&mut self)
    {
        self.bits_left = 0;

        self.marker = None;

        self.buffer = 0;

        self.lsb_buffer = 0;
    }
}

/// Do the equivalent of JPEG HUFF_EXTEND
#[inline(always)]
fn huff_extend(x: i32, s: i32) -> i32
{
    // if x<s return x else return x+offset[s] where offset[s] = ( (-1<<s)+1)

    (x) + ((((x) - (1 << ((s) - 1))) >> 31) & (((-1) << (s)) + 1))
}

/// Read a byte from underlying file
///
/// Function is inlined (as always)
#[inline(always)]
#[allow(clippy::cast_possible_truncation)]
fn read_u8(reader: &mut Cursor<Vec<u8>>) -> u64
{
    let pos = reader.position();

    reader.set_position(pos + 1);

    // if we have nothing left fill buffer with zeroes
    u64::from(*reader.get_ref().get(pos as usize).unwrap_or(&0))
}
