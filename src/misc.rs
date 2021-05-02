#![allow(dead_code)]
use byteorder::ReadBytesExt;
use std::fmt;
use std::io::Read;

/// Start of image, these are the first two bytes in the Image
pub const START_OF_IMAGE: u16 = 0xffd8;
/// Start of baseline DCT Huffman coding
pub const START_OF_FRAME_BASE: u16 = 0xffc0;
/// Start of another frame
pub const START_OF_FRAME_EXT_SEQ: u16 = 0xffc1;
/// Start of progressive DCT encoding
pub const START_OF_FRAME_PROG_DCT: u16 = 0xffc2;

/// Start of losless sequential Huffman coding
pub const START_OF_FRAME_LOS_SEQ: u16 = 0xffc3;
/// Start of extended sequential DCT arithmetic coding
pub const START_OF_FRAME_EXT_AR: u16 = 0xffc9;
/// Start of Progressive DCT arithmetic coding
pub const START_OF_FRAME_PROG_DCT_AR: u16 = 0xffca;
/// Start of Lossless sequential Arithmetic coding
pub const START_OF_FRAME_LOS_SEQ_AR: u16 = 0xffcb;
/// Code for Start of app marker
pub const START_OF_APP_MARKER: u16 = 0xffe0;
/// 0xffc2-> 65474
pub const PROGRESSIVE_DCT: u16 = 0xffc2;
///0xffc4 -> 65476
pub const HUFFMAN_TABLE: u16 = 0xffc4;
/// 0xffdb -> 65499
pub const QUANTIZATION_TABLE: u16 = 0xffdb;
/// 0xffdd -> 65501
pub const RESTART_INTERVAL: u16 = 0xffdd;
/// 0xffda - > 65498
pub const START_OF_SCAN: u16 = 0xffda;
/// Signifies end of image
///
/// 0xffd9->65497
pub const END_OF_IMAGE: u16 = 0xffd9;

/// Unzigzag a zig-zagged jpeg
///
/// This is used as an index mechanism
/// Ie calling `UN_ZIGZAG[5]` gives you 2 which means the
/// Value at index 5 should be moved to index 2
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

/// Remove FF0 byte snuffed to prevent conditional checking during decoding
/// Because of such things as byte snuffing
pub fn remove_ff0<T>(mut buf: T) -> Vec<u8>
where
    T: Read,
{
    let mut decoded = Vec::with_capacity(10000);
    let mut last_value = 0;
    while let Ok(data) = buf.read_u8() {
        if !(data == 0x00 && last_value == 0xff) {
            decoded.push(data)
        }
        last_value = data;
    }
    let length = decoded.len();

    // Remove the last 2 elements that contain the EOI marker
    unsafe {
        decoded.set_len(length - 2);
    }

    decoded
}
