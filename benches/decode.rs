use criterion::{black_box, criterion_group, criterion_main, Criterion};
use jpeg_decoder::Decoder;
use std::fs::read;
use std::io::BufReader;
use std::time::Duration;
use zune_jpeg::errors::DecodeErrors;
use zune_jpeg::Image;
use zune_jpeg::JPEG;

fn decode(buf: &[u8]) -> Result<Image, DecodeErrors> {
    let jpeg = JPEG::decode_buffer(buf);
    return jpeg;
}
fn decode_jpeg(buf: &[u8]) -> Vec<u8> {
    let mut d = Decoder::new(BufReader::new(buf));
    let e = d.decode().unwrap();
    return e;
}
fn criterion_benchmark(c: &mut Criterion) {
    let a = String::from(env!("CARGO_MANIFEST_DIR")) + "/test-images/test-baseline.jpg";
    let data = read(a).unwrap();
    c.bench_function("Baseline JPEG Decoding ZUNE_JPEG", |b| {
        b.iter(|| black_box(decode(data.as_slice())))
    });
    c.bench_function("Baseline JPEG Decoding JPEG-DECODER", |b| {
        b.iter(|| black_box(decode_jpeg(data.as_slice())))
    });
}

criterion_group!(name=benches;
      config={
      let c = Criterion::default();
        c.measurement_time(Duration::from_secs(60))
      };
    targets=criterion_benchmark);
criterion_main!(benches);
