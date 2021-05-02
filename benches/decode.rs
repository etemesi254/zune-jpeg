use criterion::{criterion_group, criterion_main, Criterion};
use std::fs::read;
use std::time::Duration;
use zune_jpeg::Image;
use zune_jpeg::JPEG;

fn decode(buf: &[u8]) -> Vec<u8> {
    let mut jpeg = JPEG::default();
    return jpeg.decode_buffer(buf);
}
fn criterion_benchmark(c: &mut Criterion) {
    let a = String::from(env!("CARGO_MANIFEST_DIR")) + "/test-images/test-baseline.jpg";
    let data = read(a).unwrap();
    c.bench_function("Baseline JPEG Decoding", |b| {
        b.iter(|| decode(data.as_slice()))
    });
}

criterion_group!(name=benches;
      config={
      let c = Criterion::default();
        c.measurement_time(Duration::from_secs(60))
      };
    targets=criterion_benchmark);
criterion_main!(benches);
