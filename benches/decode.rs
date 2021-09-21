use criterion::{black_box, criterion_group, criterion_main, Criterion};
use std::fs::read;
use std::time::Duration;

use zune_jpeg::{ColorSpace, Decoder};

fn decode_jpeg(buf: &[u8]) -> Vec<u8> {
    let mut d = Decoder::new();
    d.set_output_colorspace(ColorSpace::RGB);

    d.decode_buffer(buf).unwrap()
}

fn decode_jpeg_mozjpeg(buf: &[u8]) -> Vec<[u8; 4]> {
    let p = std::panic::catch_unwind(|| {
        let d = mozjpeg::Decompress::with_markers(mozjpeg::ALL_MARKERS)
            .from_mem(buf)
            .unwrap();

        // rgba() enables conversion
        let mut image = d.rgba().unwrap();

        let pixels: Vec<[u8; 4]> = image.read_scanlines().unwrap();
        assert!(image.finish_decompress());
        pixels
    })
    .unwrap();
    p
}
fn decode_jpeg_image_rs(buf: &[u8]) -> Vec<u8> {
    let mut decoder = jpeg_decoder::Decoder::new(buf);
    decoder.decode().unwrap()
}
fn criterion_benchmark(c: &mut Criterion) {
    let a = String::from("/home/caleb/Pictures/backgrounds/wallpapers/backgrounds/Mr. Lee.jpg");
    let data = read(a).unwrap();

    c.bench_function("Baseline JPEG Decoding zune-jpeg", |b| {
        b.iter(|| black_box(decode_jpeg(data.as_slice())))
    });

    c.bench_function("Baseline JPEG Decoding  mozjpeg", |b| {
        b.iter(|| black_box(decode_jpeg_mozjpeg(data.as_slice())))
    });
    c.bench_function("Baseline JPEG Decoding  imagers/jpeg-decoder", |b| {
        b.iter(|| black_box(decode_jpeg_image_rs(data.as_slice())))
    });

    let x = read("/home/caleb/IMG_6128.JPG").unwrap();

    c.bench_function("Horizontal sampling JPEG Decoding zune-jpeg", |b| {
        b.iter(|| black_box(decode_jpeg(x.as_slice())))
    });

    c.bench_function("Horizontal sampling JPEG Decoding  mozjpeg", |b| {
        b.iter(|| black_box(decode_jpeg_mozjpeg(x.as_slice())))
    });
    c.bench_function(
        "Horizontal sampling JPEG Decoding  imagers/jpeg-decoder",
        |b| b.iter(|| black_box(decode_jpeg_image_rs(x.as_slice()))),
    );
}

criterion_group!(name=benches;
      config={
      let c = Criterion::default();
        c.measurement_time(Duration::from_secs(5))
      };
    targets=criterion_benchmark);
criterion_main!(benches);
