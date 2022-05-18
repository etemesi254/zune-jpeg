#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let mut d = zune_jpeg::Decoder::new();

    let _: Result<Vec<u8>, _> = d.decode_buffer(data);
});
