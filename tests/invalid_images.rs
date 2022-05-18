use zune_jpeg::Decoder;

#[test]
fn eof() {
    let mut decoder = Decoder::new();

    let err = decoder.decode_buffer(&[0xff, 0xd8, 0xa4]).unwrap_err();

    assert!(matches!(err, zune_jpeg::errors::DecodeErrors::ExhaustedData));
}
