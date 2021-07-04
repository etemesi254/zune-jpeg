//! # Performance optimizations
//! - One bottleneck is `refill`/`refill_fast`, the fact that we have
//! to check for `OxFF` byte during refills, which  most of the times doesn't exist
//!  but it sometimes does. This makes it slow as it forces byte wise processing and branching during the hot path.
//! There is this handy post [here] that talks about how we can avoid checks in the hot path, basically
//! we remove `0x00` bytes before we reach the hot path, but I'm yet to think of faster ways to do that
//!
//! -
//!
//![here]:https://fgiesen.wordpress.com/2011/11/21/buffer-centric-io/

use crate::huffman::{HuffmanTable, HUFF_LOOKAHEAD};
use crate::marker::Marker;
use crate::misc::UN_ZIGZAG;
use std::io::{Cursor};

// PS: Read this
// This code is optimised for speed, like it's heavily optimised
// I know 95% of whats goes on, (the merits of borrowing someone's code
// but there is extensive use of macros and a lot of inlining
//  A lot of things may not make sense(both to me and you), that's why there a lot of comments
// and WTF! and ohh! moments
// Enjoy.

/// Maximum bits in the buffer
/// This gives us the limit we shouldn't cross when refilling buffer
/// because we use a u64 buffer capable of holding 64 bits we want to stop when we have more than
/// 56 bits , eg if we have 57 and try to add another one,we get 65 which is
/// more than what we can hold
const MAX_BITS: u8 = 56;

/// A BitStream struct, capable of decoding compressed data from the data from image
///
/// # Fields and Meanings
/// - `buffer`   : `u64`-> Stores bits from our compressed stream
/// - `bits_left`: `u8`-> A counter telling us how many valid bits are in the buffer
/// - `marker`   : `Option<Marker>`->Will store a marker if it's encountered in the stream
pub(crate) struct BitStream {
    buffer: u64,
    bits_left: u8,
    marker: Option<Marker>,
}
impl BitStream {
    /// Create a new BitStream
    ///
    /// The buffer and bits_left values are initialized to 0
    /// (it would be crazy to initialize to another value :>) )
    pub(crate) const fn new() -> BitStream {
        BitStream {
            buffer: 0,
            bits_left: 0,
            marker: None,
        }
    }
    /// Refill the buffer up to MAX_BITS
    #[inline(always)]
    fn refill(&mut self, reader: &mut Cursor<Vec<u8>>) {
        // load global variables into local stack
        let mut bits_left = self.bits_left;
        let mut buffer = self.buffer;
        // Attempt to load ar least MAX_BITS into buffer

        while bits_left <= MAX_BITS {
            // attempt to read a byte
            let byte = read_u8(reader);
            // pre-execute most common case , add a byte and increment count
            buffer = (buffer << 8) | u64::from(byte);
            bits_left += 8;

            // IF the byte read is 0xFF, check and discard stuffed zero byte
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
                        buffer &= !0xf;
                        bits_left -= 8;

                        self.marker = Some(Marker::from_u8(next_byte).unwrap())
                        // should we suspend?
                    }
                    // if next_byte is zero we need to panic(jpeg-decoder does that)
                    // but i'll let it slide
                }
            }
        }
        // unload local variables
        self.bits_left = bits_left;
        self.buffer = buffer;
    }
    /// Refill the bit buffer by (a maximum of) 48 bits (4 bytes)
    ///
    /// # Arguments
    ///  - `reader`:`&mut BufReader<R>`: A mutable reference to an underlying File/Memory buffer
    ///containing a valid JPEG image
    ///
    /// # Performance
    /// The code here is a lot(but hidden by macros) we move in a linear manner avoiding loops
    /// (which suffer from branch mis-prediction) , we can only refill 48 bits safely without overriding
    /// important bits in the buffer
    ///
    /// Because the code generated here is a lot, we might affect the instruction cache( yes it's not
    /// data that is only cached)
    ///
    /// This function will only refill if `self.count` is less than 16
    #[inline(always)]
    fn refill_fast(&mut self, reader: &mut Cursor<Vec<u8>>) {
        // Ps i know inline[always] is frowned upon

        // Macro version of refill
        // Arguments
        // buffer-> our io buffer, because rust macros cannot get values from the surrounding environment
        // bits_left-> number of bits left to full refill
        macro_rules! refill {
            ($buffer:expr,$bits_left:expr) => {
                // read a byte from the stream
                let byte = read_u8(reader);
                // append to the buffer
                $buffer = ($buffer << 8) | u64::from(byte);
                // Increment bits left
                $bits_left += 8;

                // Check for special case  of OxFF, to see if it's a stream or a marker
                if byte == 0xff {
                    // read next byte
                    let mut next_byte = read_u8(reader);
                    // Byte snuffing, if we encounter byte snuff, we skip the byte
                    if next_byte != 0x00 {
                        while next_byte == 0xFF {
                            next_byte = read_u8(reader);
                        }
                        if next_byte != 0x00 {
                            $buffer &= !0xf;
                            $bits_left -= 8;
                            // Should we suspend?
                            self.marker = Some(Marker::from_u8(next_byte).unwrap())
                        }
                    };
                }
            };
        }
        // 16 bits is enough in the buffer for any use case, if count is less than that then run a lot of code

        if self.bits_left <= 16 {
            // 6 refills, a lot of code
            refill!(self.buffer, self.bits_left);
            refill!(self.buffer, self.bits_left);
            refill!(self.buffer, self.bits_left);
            refill!(self.buffer, self.bits_left);
            refill!(self.buffer, self.bits_left);
            refill!(self.buffer, self.bits_left);
            // Note, we are not assured that all these macros will append a byte, eg if it finds a marker,
            // it won't append it to the stream.
        }
    }
    /// Decode a Minimum Code Unit(MCU) as quickly as possible
    ///
    ///# Performance
    ///- We are using the same version of the Huffman Decoder from libjpeg-turbo/[jdhuff.h+jdhuff.c]
    /// which is pretty fast(jokes its `super fast`...).
    /// - The code is heavily inlined(this function itself is inlined)(I would not like to be that guy viewing disassembly info of this
    /// function) and makes use of macros, so the resulting code the compiler receives probably spans too many lines
    /// but all in favour of speed
    /// - Also some of the things are `unsafe` but they are heavily tested( but if it breaks hit me up really fast)
    /// # Expectations
    /// The `block` should be initialized to zero
    #[inline(always)]
    pub fn decode_mcu_fast(
        &mut self,
        reader: &mut Cursor<Vec<u8>>,
        dc_table: &HuffmanTable,
        ac_table: &HuffmanTable,
        block: &mut [i32; 64],
        dc_prediction: &mut i32,
    ) {
        // Same macro as HUFF_DECODE_FAST in jdhuff.h 220(as time of writing)
        // Sadly this is slow, about 2x slower than that implementation
        // even though its a word to word implementation.
        macro_rules! huff_decode_fast {
            ($s:expr,$n_bits:expr,$table:expr) => {
                // refill buffer, the check if its less than 16 will be done inside the function
                // but since we use inline(always) we have a branch here which tells us if we
                // refill
                self.refill_fast(reader);
                $s = self.peek_bits(HUFF_LOOKAHEAD);
                // Lookup 8 bits in the stream and see if there is a corresponding byte

                // SAFETY:this can never cause UB,because s cannot be more than 1<<HUFF_LOOKAHEAD
                $s = $table.lookup[$s as usize];

                $n_bits = $s >> HUFF_LOOKAHEAD;
                // Pre-execute common case of nb <= HUFF_LOOKAHEAD
                self.drop_bits($n_bits as u8);
                $s &= ((1 << HUFF_LOOKAHEAD) - 1);
                if $n_bits > i32::from(HUFF_LOOKAHEAD) {
                    // if we don't get a hit in the first HUFF_LOOKAHEAD bits

                    // equivalent of jpeg_huff_decode
                    // don't use self.get_bits() here , we don't want to modify bits left
                    $s = ((self.buffer >> self.bits_left) & ((1 << ($n_bits)) - 1)) as i32;
                    while $s > $table.maxcode[$n_bits as usize] {
                        // Read one bit from the buffer and append to s
                        $s <<= 1;
                        $s |= self.get_bits(1);
                        $n_bits += 1;
                    }
                    // Greater than 16? probably corrupt image
                    if $n_bits > 16 {
                        $s = 0
                    } else {
                        $s = i32::from(
                            $table.values[($s + $table.offset[$n_bits as usize] & 0xFF) as usize],
                        );
                    }
                }
            };
        }
        let (mut s, mut l, mut r);
        huff_decode_fast!(s, l, dc_table);

        if s != 0 {
            self.refill_fast(reader);
            r = self.get_bits(s as u8);
            s = huff_extend(r, s);
        }
        // this should be changed with the upper coeff to deal with previous block
        // its done outside of this loop
        *dc_prediction += s;
        block[0] = *dc_prediction;

        // Decode AC coefficients
        let mut k:usize = 1;
        // Get fast AC table as a reference before we enter the hot path
        let ac_lookup = ac_table.ac_lookup.as_ref().unwrap();
        while k < 64 {
            self.refill_fast(reader);
            s = self.peek_bits(HUFF_LOOKAHEAD);
            // SAFETY: S can never go above (1<<HUFF_LOOKAHEAD)

            let v = {
                #[cfg(feature = "perf")]
                    {
                        unsafe { *ac_lookup.get_unchecked(s as usize) }
                    }
                #[cfg(not(feature = "perf"))]
                    {
                        ac_lookup[s as usize]
                    }
            };
            if v != 0 {
                //  FAST AC path
                k += ((v >> 4) & 15) as usize; // run
                self.drop_bits((v & 15) as u8); // combined length
                {
                    #[cfg(feature = "perf")]
                        {
                            unsafe {
                                *block.get_unchecked_mut(*UN_ZIGZAG.get_unchecked(k )) = v >> 8
                            }
                        }
                    #[cfg(not(feature = "perf"))]
                        {
                            block[UN_ZIGZAG[k ]] = v >> 8;
                        }
                }
                k += 1;
            } else {
                // follow the normal route

                s = {
                    // SAFETY s can never go above 1<<HUFF_LOOKAHEAD
                    #[cfg(feature = "perf")]
                        {
                            unsafe { *ac_table.lookup.get_unchecked(s as usize) }
                        }
                    #[cfg(not(feature = "perf"))]
                        {
                            ac_table.lookup[s as usize]
                        }
                };
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
                        s = 0
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
                    self.refill_fast(reader);
                    r = self.get_bits(s as u8);
                    s = huff_extend(r, s);
                    #[cfg(feature = "perf")]
                        {
                            unsafe {
                                *block.get_unchecked_mut(*UN_ZIGZAG.get_unchecked(k as usize)) = s;
                            }
                        }
                    #[cfg(not(feature = "perf"))]
                        {
                            block[UN_ZIGZAG[k as usize]] = s;
                        }
                    // libjpeg-turbo doesn't do this, so why am I?
                    k += 1;
                } else {
                    if r != 15 {
                        break;
                    }
                    k += 15;
                }
            }
        }
    }

    /// Peek `look_ahead` bits ahead without discarding them from the buffer
    ///
    /// This is used in conjunction with a lookup table hence it is `const` ified
    #[inline(always)]
    const fn peek_bits(&self, look_ahead: u8) -> i32 {
        ((self.buffer >> (self.bits_left - look_ahead)) & ((1 << look_ahead) - 1)) as i32
    }
    /// Discard the next `N` bits without checking
    #[inline(always)]
    fn drop_bits(&mut self, n: u8) {
        self.bits_left -= n;
    }
    /// Read `n_bits` from the buffer  and discard them
    #[inline(always)]
    fn get_bits(&mut self, n_bits: u8) -> i32 {
        let t = ((self.buffer >> (self.bits_left - n_bits)) & ((1 << n_bits) - 1)) as i32;
        self.bits_left -= n_bits;
        t
    }
}
#[inline(always)]
fn huff_extend(x: i32, s: i32) -> i32 {
    // if x<s return x else return x+offset[s] where offset[s] = ( (-1<<s)+1)
    (x) + ((((x) - (1 << ((s) - 1))) >> 31) & (((-1) << (s)) + 1))
}
/// Read a byte from underlying file
///
/// Function is inlined (as always)
#[inline(always)]
fn read_u8(reader: &mut Cursor<Vec<u8>>) -> u8

{
    let pos = reader.position();
    reader.set_position(pos+1);
    // if we have nothing left fill buffer with zeroes
    *reader.get_ref().get(pos as usize).unwrap_or(&0)
}
