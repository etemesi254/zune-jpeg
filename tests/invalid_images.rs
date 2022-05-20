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
