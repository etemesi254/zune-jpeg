use zune_jpeg::Decoder;

#[test]
fn eof()
{
    let mut decoder = Decoder::new();

    let err = decoder.decode_buffer(&[0xff, 0xd8, 0xa4]).unwrap_err();

    assert!(matches!(err, zune_jpeg::errors::DecodeErrors::Format(_)));
}

#[test]
fn bad_ff_marker_size()
{
    let mut decoder = Decoder::new();

    let err = decoder
        .decode_buffer(&[0xff, 0xd8, 0xff, 0x00, 0x00, 0x00])
        .unwrap_err();

    assert!(
        matches!(err, zune_jpeg::errors::DecodeErrors::Format(x) if x == "Got marker with invalid raw size 0")
    );
}

#[test]
fn bad_number_of_scans()
{
    let mut decoder = Decoder::new();

    let err = decoder
        .decode_buffer(&[255, 216, 255, 218, 232, 197, 255])
        .unwrap_err();

    assert!(
        matches!(err, zune_jpeg::errors::DecodeErrors::SosError(x) if x == "Bad SOS length,corrupt jpeg")
    );
}

#[test]
fn huffman_length_subtraction_overflow()
{
    let mut decoder = Decoder::new();

    let err = decoder
        .decode_buffer(&[255, 216, 255, 196, 0, 0])
        .unwrap_err();

    assert!(
        matches!(err, zune_jpeg::errors::DecodeErrors::HuffmanDecode(x) if x == "Invalid Huffman length in image")
    );
}
