#![allow(
    clippy::if_not_else,
    clippy::similar_names,
    clippy::inline_always,
    clippy::doc_markdown
)]
//! This file exposes a single struct that can decode a huffman encoded Bitstream in a JPEG file
use std::io::Cursor;

use crate::huffman::{HuffmanTable, HUFF_LOOKAHEAD};
use crate::marker::Marker;
use crate::misc::UN_ZIGZAG;

// PS: Read this
// This code is optimised for speed, like it's heavily optimised
// I know 95% of whats goes on, (the merits of borrowing someone's code
// but there is extensive use of macros and a lot of inlining
//  A lot of things may not make sense(both to me and you), that's why there a lot of comments
// and WTF! and ohh! moments
// Enjoy.

/// A `BitStream` struct, capable of decoding compressed data from the data from image
///
///
pub(crate) struct BitStream {
    /// A MSB type buffer that is used for some certain operations
    buffer: u64,
    /// A LSB type buffer that is used to accelerate some operations like
    /// peek_bits and get_bits.
    lsb_buffer: u64,
    /// Tell us the bits left the two buffer
    bits_left: u8,
    /// Did we find a marker(RST/EOF) during decoding?
    pub marker: Option<Marker>,
}

impl BitStream {
    /// Create a new BitStream
    ///
    /// The buffer and bits_left values are initialized to 0
    /// (it would be crazy to initialize to another value :>) )
    pub(crate) const fn new() -> BitStream {
        BitStream {
            buffer: 0,
            lsb_buffer: 0,
            bits_left: 0,
            marker: None,
        }
    }
    /// Refill the bit buffer by (a maximum of) 48 bits (4 bytes)
    ///
    /// # Arguments
    ///  - `reader`:`&mut BufReader<R>`: A mutable reference to an underlying File/Memory buffer
    ///containing a valid JPEG image
    ///
    /// # Performance
    /// The code here is a lot(but hidden by macros) we move in a linear manner avoiding loops
    /// (which suffer from branch mis-prediction) , we can only refill 32 bits safely without overriding
    /// important bits in the buffer
    ///
    /// Because the code generated here is a lot, we might affect the instruction cache( yes it's not
    /// data that is only cached)
    ///
    /// This function will only refill if `self.count` is less than 32
    #[inline(always)]
    fn refill(&mut self, reader: &mut Cursor<Vec<u8>>) -> bool {
        // Ps i know inline[always] is frowned upon

        /// Macro version of a single byte refill.
        /// Arguments
        /// buffer-> our io buffer, because rust macros cannot get values from the surrounding environment
        /// bits_left-> number of bits left to full refill
        macro_rules! refill {
            ($buffer:expr,$byte:expr,$bits_left:expr) => {
                // read a byte from the stream
                $byte = read_u8(reader);

                // append to the buffer
                // JPEG is a MSB type buffer so that means we append this
                // to the lower end (0..8) of the buffer and push the rest bits above..
                $buffer = ($buffer << 8) | $byte;

                //$lsb_byte |= $byte << ( 56 - $bits_left);
                //println!("{:b},{}",$byte,56-$bits_left);

                // Increment bits left
                $bits_left += 8;

                // Check for special case  of OxFF, to see if it's a stream or a marker
                if $byte == 0xff {
                    // read next byte
                    let mut next_byte = read_u8(reader);
                    // Byte snuffing, if we encounter byte snuff, we skip the byte
                    if next_byte != 0x00 {
                        // skip that byte we read
                        while next_byte == 0xFF {
                            next_byte = read_u8(reader);
                        }
                        if next_byte != 0x00 {
                            // Undo the byte append and return
                            $buffer &= !0xf;
                            //  $lsb_byte <<= 8;
                            $bits_left -= 8;
                            self.marker = Some(Marker::from_u8(next_byte as u8).unwrap());
                            // if we get a marker, we return immediately, and hope
                            // that the bits stored are enough to finish MCU decoding with no hassle
                            return false;
                        }
                    };
                }
            };
        }
        // 32 bits is enough for a decode(16 bits) and receive_extend(max 16 bits)
        // If we have less than 32 bits we refill
        if self.bits_left <= 32 {
            // This serves two reasons,
            // 1: Make clippy shut up
            // 2: Favour register reuse
            let mut byte;
            // 4 refills, if all succeed the stream should contain enough bits to decode a value
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
    /// Decode a Minimum Code Unit(MCU) as quickly as possible
    ///
    /// # Arguments
    /// - reader: The bitstream from where we read more bits.
    /// - dc_table: The Huffman table used to decode the DC coefficient
    /// - ac_table: The Huffman table used to decode AC values
    /// - block: A memory region where we will write out the decoded values
    /// - DC prediction: Last DC value for this component
    ///
    ///# Performance
    ///- We are using a modified version of the Huffman Decoder from `libjpeg-turbo/[jdhuff.h+jdhuff.c]`
    /// and `stb_image`
    /// which is pretty fast(jokes its `super fast`...).
    /// - The code is heavily inlined(this function itself is inlined)(I would not like to be that guy viewing disassembly info of this
    /// function) and makes use of macros, so the resulting code the compiler receives probably spans too many lines
    /// - The main problems are memory bottlenecks
    /// # Expectations
    /// The `block` should be initialized to zero
    #[allow(
        clippy::many_single_char_names,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    #[inline(always)]
    pub fn decode_fast(
        &mut self,
        reader: &mut Cursor<Vec<u8>>,
        dc_table: &HuffmanTable,
        ac_table: &HuffmanTable,
        block: &mut [i16; 64],
        dc_prediction: &mut i32,
    ) -> bool {

        let (mut s, mut l, mut r);

        if !self.refill(reader) {
            return false;
        };
        s = self.peek_bits::<HUFF_LOOKAHEAD>();
        s = dc_table.lookup[s as usize];
        l = s >> HUFF_LOOKAHEAD;
        self.drop_bits(l as u8);
        s &= (1 << HUFF_LOOKAHEAD) - 1;
        if l > i32::from(HUFF_LOOKAHEAD) {
            s = ((self.buffer >> self.bits_left) & ((1 << (l)) - 1)) as i32;
            while s > dc_table.maxcode[l as usize] {
                s <<= 1;
                s |= self.get_bits(1);
                l += 1;
            }
            if l > 16 {
                s = 0;
            } else {
                s = i32::from(
                    dc_table.values[((s + dc_table.offset[l as usize]) & 0xFF) as usize],
                );
            }
        }
        if s != 0 {
            r = self.get_bits(s as u8);
            s = huff_extend(r, s);
        }
        // Update DC prediction
        *dc_prediction += s;
        block[0] = *dc_prediction as i16;
        // Decode AC coefficients
        let mut k: usize = 1;
        // Get fast AC table as a reference before we enter the hot path
        let ac_lookup = ac_table.ac_lookup.as_ref().unwrap();
        while k < 64 {
            // Found RST marker?
            if !self.refill(reader) {
                // found a marker , stop processing
                // The caller handles restart markers for us, it will call the necessary
                // routines
                return false;
            };
            s = self.peek_bits::<HUFF_LOOKAHEAD>();
            // Safety:
            //     S can never go past 1<<HUFF_LOOKAHEAD and lookup table is 1<<HUFF_LOOKAHEAD
            let v =  ac_lookup[s as usize] ;
            if v != 0 {
                //  FAST AC path
                k += ((v >> 4) & 15) as usize; // run
                self.drop_bits((v & 15) as u8); // combined length
                // Safety:
                //  UN_ZIGZAG cannot go past 63 and k can never go past 85
                    block[UN_ZIGZAG[k]] = v >> 8;
                k += 1;
            } else {
                s = ac_table.lookup[s as usize];
                l = s >> HUFF_LOOKAHEAD;
                self.drop_bits(l as u8);
                s &= (1 << HUFF_LOOKAHEAD) - 1;
                if l > i32::from(HUFF_LOOKAHEAD) {
                    s = ((self.buffer >> self.bits_left) & ((1 << (l)) - 1)) as i32;
                    while s > ac_table.maxcode[l as usize] {
                        s <<= 1;
                        s |= self.get_bits(1);
                        l += 1;
                    }
                    if l > 16 {
                        s = 0;
                    } else {
                        s = i32::from(
                            ac_table.values[((s + ac_table.offset[l as usize]) & 0xFF) as usize],
                        );
                    }
                };
                r = s >> 4;
                s &= 15;
                if s != 0 {
                    k += r as usize;
                    r = self.get_bits(s as u8);
                    s = huff_extend(r, s);
                    // Safety
                    // 1: K can never go beyond 85 (everything that increments k keeps that guarantee)
                    block[UN_ZIGZAG[k as usize]] = s as i16;

                    k += 1;
                } else {
                    if r != 15 {
                        return true;
                    }
                    k += 15;
                }
            }
        }
        return true;
    }

    /// Peek `look_ahead` bits ahead without discarding them from the buffer
    #[inline(always)]
    #[allow(clippy::cast_possible_truncation)]
    const fn peek_bits<const LOOKAHEAD: u8>(&self) -> i32 {
        // for the LSB buffer peek bits doesn't require an and to remove/zero out top bits
        (self.lsb_buffer >> (64 - LOOKAHEAD)) as i32
    }
    /// Discard the next `N` bits without checking
    #[inline]
    fn drop_bits(&mut self, n: u8) {
        self.bits_left -= n;
        // remove top n bits  in lsb buffer
        self.lsb_buffer <<= n;
    }
    //noinspection ALL
    /// Read `n_bits` from the buffer  and discard them
    #[inline(always)]
    #[allow(clippy::cast_possible_truncation)]
    fn get_bits(&mut self, n_bits: u8) -> i32 {
        let t = (self.lsb_buffer >> (64 - n_bits)) as i32;
        // Reduce the bits left, this influences the MSB buffer
        self.bits_left -= n_bits;
        // shift out bits read in the LSB buffer
        self.lsb_buffer <<= n_bits;
        t
    }
    /// Reset the stream if we have a restart marker
    ///
    /// Restart markers indicate drop those bits in the stream and zero out everything
    #[cold]
    pub fn reset(&mut self) {
        self.bits_left = 0;
        self.marker = None;
        self.buffer = 0;
        self.lsb_buffer = 0;
    }
}
/// Do the equivalent of JPEG HUFF_EXTEND
#[inline(always)]
fn huff_extend(x: i32, s: i32) -> i32 {
    // if x<s return x else return x+offset[s] where offset[s] = ( (-1<<s)+1)

    (x) + ((((x) - (1 << ((s) - 1))) >> 31) & (((-1) << (s)) + 1))
}

/// Read a byte from underlying file
///
/// Function is inlined (as always)
#[inline(always)]
#[allow(clippy::cast_possible_truncation)]
fn read_u8(reader: &mut Cursor<Vec<u8>>) -> u64 {
    let pos = reader.position();
    reader.set_position(pos + 1);
    // if we have nothing left fill buffer with zeroes
    u64::from(*reader.get_ref().get(pos as usize).unwrap_or(&0))
}
