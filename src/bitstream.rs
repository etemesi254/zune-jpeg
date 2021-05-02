//! A `BitStream` Reader
//!
//! A Bit-stream reader reads a byte from a stream
//! and then iterates over the byte returns  a bit from the byte
//!
//! One bit at a time
//!

/// Creates a lookup table for bits
///
/// This increases the binary output by 2048 bytes ( 2 kb's)
/// using `mem::size_of::<[[u8;8];256]`
const fn create_lookup() -> [[u8; 8]; 256] {
    let mut c: [[u8; 8]; 256] = [[0; 8]; 256];
    let mut val = 0;
    // for is not allowed in a constant loop
    while val < 256 {
        let mut a = [0; 8];
        let mut bit_position = 0;
        while bit_position < 8 {
            // Get bits from byte stream
            a[bit_position] = (val as u8) >> (7 - bit_position) & 1;
            bit_position += 1;
        }
        c[val] = a;
        val += 1;
    }
    c
}
/// A lookup table that provides binary bits of all digits from 0 to 255(inclusive)
///
/// Increases binary size by 2048 bytes(2 kb)
const LOOKUP: [[u8; 8]; 256] = create_lookup();

/// Reads a bit from a Byte stream
///
/// Returns `Some(1)` or `Some(0)` if the byte exists
/// or `None` if we reached the end of the buffer
///
/// # Example
/// Assuming a buffer contains `255`
/// ```
/// use std::io::Cursor; // Cursor implements read trait for &[u8] buffers
/// use crate::decoders::jpeg::huffman::BitStreamReader;
/// let reader = BitStreamReader::from(&[255]);
/// assert_eq(reader.read(),Some("1"));
///
/// ```
///
///
pub struct BitStreamReader {
    // The underlying buffer containing all bits in the stream
    buffer: Vec<u8>,
    // position
    position: usize,
}
impl BitStreamReader {
    /// Create a bitstream from u8 bytes
    ///
    /// Internally, it uses a lookup table to map a byte to it's corresponding bits hence it's
    /// quite fast
    pub fn from(reader: &[u8]) -> BitStreamReader {
        // Complex logic goes here

        // initialize a buffer to hold u8 values of bits from the stream
        let mut buffer: Vec<u8> = Vec::with_capacity(reader.len() * 8);
        for byte in reader.iter() {
            // lookup bits corresponding to the bytes
            buffer.extend(LOOKUP[*byte as usize].iter())
        }

        BitStreamReader {
            position: 0,
            buffer,
        }
    }
    /// Reads a bit from the current byte stream and returns
    ///
    /// `Some("1")` or `Some("O")` if it exists or `None` if we reached the End of buffer
    pub fn read(&mut self) -> Option<String> {
        if self.position == self.buffer.len() {
            // exhausted bits from BitStream
            return None;
        }
        // Using `usize::to_string()` was taking too long of a time
        let data = Some(
            // Create a string from a utf8 representation of 0 or 1
            // Safety we are sure 0b_.. is a valid utf8 representation of 0
            unsafe {
                match &self.buffer[self.position] {
                    0 => String::from_utf8_unchecked(vec![0b_00110000]),
                    _ => String::from_utf8_unchecked(vec![0b_00110001]),
                }
            },
        );

        // increment position
        self.position += 1;

        return data;
    }
    /// Read `n` bits from the buffer
    ///
    /// # Panics
    /// If `amount` is more than data available
    pub fn read_n(&mut self, amount: usize) -> &[u8] {
        let last = self.position + amount;
        assert!(
            last < self.buffer.len(),
            "Trying to read more data than available"
        );
        let data = &self.buffer[self.position..last];
        self.position = last;
        return data;
    }
}
