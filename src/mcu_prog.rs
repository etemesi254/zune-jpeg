use std::io::Cursor;

use crate::errors::DecodeErrors;
use crate::Decoder;

impl Decoder
{
    /// Decode a progressive image with no interleaving.

    pub(crate) fn decode_mcu_ycbcr_non_interleaved_prog(
        &mut self, _reader: &mut Cursor<Vec<u8>>,
    ) -> Result<Vec<u8>, DecodeErrors>
    {
        return Ok(Vec::new());
    }
}
