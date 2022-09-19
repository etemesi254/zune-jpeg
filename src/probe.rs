use crate::Decoder;

/// Probe a buffer and return a boolean
/// to show if this is a jpeg image.
///
/// If it's a valid image, this parses the header without
/// doing the entropy decoding and post processing stage
#[must_use]
pub fn probe(buffer: &[u8]) -> bool
{
    let mut decoder = Decoder::new();

    decoder.read_headers(buffer).is_ok()
}
