#![allow(dead_code)]

use crate::errors::DecodeErrors;
use std::fmt;
use std::io::{BufReader, Read};

/// Start of baseline DCT Huffman coding
pub const START_OF_FRAME_BASE: u16 = 0xffc0;
/// Start of another frame
pub const START_OF_FRAME_EXT_SEQ: u16 = 0xffc1;
/// Start of progressive DCT encoding
pub const START_OF_FRAME_PROG_DCT: u16 = 0xffc2;

/// Start of Lossless sequential Huffman coding
pub const START_OF_FRAME_LOS_SEQ: u16 = 0xffc3;
/// Start of extended sequential DCT arithmetic coding
pub const START_OF_FRAME_EXT_AR: u16 = 0xffc9;
/// Start of Progressive DCT arithmetic coding
pub const START_OF_FRAME_PROG_DCT_AR: u16 = 0xffca;
/// Start of Lossless sequential Arithmetic coding
pub const START_OF_FRAME_LOS_SEQ_AR: u16 = 0xffcb;

/// Undo run length encoding of coefficients by placing them in natural order
#[rustfmt::skip]
pub const UN_ZIGZAG: [usize; 64] = [
    0,  1,  8,  16, 9,  2,  3, 10,
    17, 24, 32, 25, 18, 11, 4,  5,
    12, 19, 26, 33, 40, 48, 41, 34,
    27, 20, 13, 6,  7,  14, 21, 28,
    35, 42, 49, 56, 57, 50, 43, 36,
    29, 22, 15, 23, 30, 37, 44, 51,
    58, 59, 52, 45, 38, 31, 39, 46,
    53, 60, 61, 54, 47, 55, 62, 63,
];

/// Align data to a 16 byte boundary
#[repr(align(16))]
#[derive(Clone)]
pub struct Aligned16<T: ?Sized>(pub T);

impl<T> Default for Aligned16<T>
where
    T: Default,
{
    fn default() -> Self {
        Aligned16(T::default())
    }
}
/// Align data to a 16 byte boundary
#[repr(align(32))]
#[derive(Clone)]
pub struct Aligned32<T: ?Sized>(pub T);

impl<T> Default for Aligned32<T>
where
    T: Default,
{
    fn default() -> Self {
        Aligned32(T::default())
    }
}
/// Color conversion types
///
/// This enumerates over supported color conversion types the image can decode
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum ColorSpace {
    /// Red,Green,Blue
    RGB,
    /// Monochrome
    GRAYSCALE,
    /// YCbCr, (also known as YUV)
    YCbCr,
    /// C/M/Y/K
    CMYK,
    /// Y/Cb/Cr/K
    YCCK,
    /// Blue/Green/Red
    BGR,
    /// R,G,B,A colorspace, we set the fourth channel as 255 to represent an
    /// opaque alpha channel
    RGBA,
}
impl ColorSpace {
    /// Number of channels (including unused alpha) in this color space
    pub const fn num_components(self) -> i32 {
        match self {
            Self::RGB|Self::BGR| Self::YCbCr => 3,
            Self::CMYK|Self::RGBA|Self::YCCK => 4,
            Self::GRAYSCALE => 1,
        }
    }
}
impl Default for ColorSpace {
    ///Set default colorspace as RGB
    ///
    /// This is the common behaviour for all (sane) Decoder images
    fn default() -> Self {
        ColorSpace::RGB
    }
}
/// Markers that identify different Start of Image markers
/// They identify the type of encoding and whether the file use lossy(DCT) or lossless
/// compression and whether we use Huffman or arithmetic coding schemes
#[derive(Eq, PartialEq, Copy, Clone)]
#[allow(clippy::upper_case_acronyms)]
pub enum SOFMarkers {
    /// Baseline DCT markers
    BaselineDct,
    /// SOF_1 Extended sequential DCT,Huffman coding
    ExtendedSequentialHuffman,
    /// Progressive DCT, Huffman coding
    ProgressiveDctHuffman,
    /// Lossless (sequential), huffman coding,
    LosslessHuffman,
    /// Extended sequential DEC, arithmetic coding
    ExtendedSequentialDctArithmetic,
    /// Progressive DCT, arithmetic coding,
    ProgressiveDctArithmetic,
    /// Lossless ( sequential), arithmetic coding
    LosslessArithmetic,
}
impl Default for SOFMarkers {
    fn default() -> Self {
        Self::BaselineDct
    }
}
impl SOFMarkers {
    /// Check if a certain marker is sequential DCT or not
    pub fn is_sequential_dct(self) -> bool {
        matches!(
            self,
            Self::BaselineDct
                | Self::ExtendedSequentialHuffman
                | Self::ExtendedSequentialDctArithmetic
        )
    }
    /// Check if a marker is a Lossles type or not
    pub fn is_lossless(self) -> bool {
        matches!(self, Self::LosslessHuffman | Self::LosslessArithmetic)
    }
    /// Check whether a marker is a progressive marker or not
    pub fn is_progressive(self) -> bool {
        matches!(
            self,
            Self::ProgressiveDctHuffman | Self::ProgressiveDctArithmetic
        )
    }
    pub fn from_int(int: u16) -> Option<SOFMarkers> {
        match int {
            START_OF_FRAME_BASE => Some(Self::BaselineDct),
            START_OF_FRAME_PROG_DCT => Some(Self::ProgressiveDctHuffman),
            START_OF_FRAME_PROG_DCT_AR => Some(Self::ProgressiveDctArithmetic),
            START_OF_FRAME_LOS_SEQ => Some(Self::LosslessHuffman),
            START_OF_FRAME_LOS_SEQ_AR => Some(Self::LosslessArithmetic),
            START_OF_FRAME_EXT_SEQ => Some(Self::ExtendedSequentialHuffman),
            START_OF_FRAME_EXT_AR => Some(Self::ExtendedSequentialDctArithmetic),
            _ => None,
        }
    }
}
impl fmt::Debug for SOFMarkers {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match &self {
            Self::BaselineDct => write!(f, "Baseline DCT"),
            Self::ExtendedSequentialHuffman => {
                write!(f, "Extended sequential DCT, Huffman Coding")
            }
            Self::ProgressiveDctHuffman => write!(f, "Progressive DCT,Huffman Encoding"),
            Self::LosslessHuffman => write!(f, "Lossless (sequential) Huffman encoding"),
            Self::ExtendedSequentialDctArithmetic => {
                write!(f, "Extended sequential DCT, arithmetic coding")
            }
            Self::ProgressiveDctArithmetic => write!(f, "Progressive DCT, arithmetic coding"),
            Self::LosslessArithmetic => write!(f, "Lossless (sequential) arithmetic coding"),
        }
    }
}

/// Read u8 from a buffer returning the byte
///
/// # Arguments
/// - `reader`: A mutable reference  implementing the `Read` trait
///
/// # Returns
/// The byte read
/// # Errors
/// If the reader cannot read the next byte
///
/// # Caveats
/// Some files may be incomplete and for such files, we'll return zeroes as the place holder
#[inline]
#[allow(clippy::unused_io_amount)]
pub fn read_u8<R>(reader: &mut R) -> u8
where
    R: Read,
{
    let mut tmp = [0; 1];
    // if there is no more data fill with zero
    reader
        .read_exact(&mut tmp)
        .expect("Could not read from underlying buffer");

    tmp[0]
}
/// Read two `u8`'s from a buffer and create a `u16` from the bytes in BigEndian order.
///
/// The first 8 bytes of the u16 are made by the first u8 read, and the second one make the last 8
/// ```text
/// u16 => [first_u8][second_u8]
/// ```
/// # Argument
///  - reader: A mutable reference to anything that implements `Read` trait
///
/// # Returns
/// - the u16 value created from the two u8's
///
/// # Panics
/// When the bytes cannot be read
#[inline]
pub fn read_u16_be<R>(reader: &mut BufReader<R>) -> Result<u16, DecodeErrors>
where
    R: Read,
{
    let mut tmp: [u8; 2] = [0, 0];
    if reader.read(&mut tmp).expect("could not read from data") != 2 {
        return Err(DecodeErrors::ExhaustedData);
    };
    let v = u16::from_be_bytes(tmp);
    Ok(v)
}

/// Read `buf.len()*2` data from the underlying `u8` buffer and convert it into u16, and store it into
/// `buf`
///
/// # Arguments
/// - reader: A mutable reference to the underlying reader.
/// - buf: A mutable reference to a slice containing u16's
#[inline]
pub fn read_u16_into<R>(reader: &mut BufReader<R>, buf: &mut [u16]) -> Result<(), DecodeErrors>
where
    R: Read,
{
    let mut reader = reader;

    for i in buf {
        *i = read_u16_be(&mut reader)?
    }
    Ok(())
}
/// Multiply elements in two vectors
///
/// # Arguments
/// - `a`: A vector containing 64 elements
/// - `b` :Another vector containing 64 elements
///
/// # Returns
/// Another vector containing multiplied coefficients
#[inline]
pub fn dequantize<T>(a: &[i16; 64], b: &[i16; 64]) -> [T; 64]
where
    T: Copy + Default + From<i16>,
{
    // some decoders put this inside IDCT, i prefer it here
    let mut c = [T::default(); 64];
    // using `i32::from` makes the generated assembly first convert before multiplying
    // which does not favour us at all
    for i in 0..c.len() {
        *c.get_mut(i).unwrap() = T::from(a.get(i).unwrap() * b.get(i).unwrap());
    }
    c
}
