use std::fs::OpenOptions;
use std::io::Write;

use mozjpeg::ColorSpace as OutColorSpace;
use zune_jpeg::{ColorSpace, Decoder};

fn write_output(name: &str, pixels: &[u8], width: usize, height: usize, colorspace: OutColorSpace)
{
    let output: String = env!("CARGO_MANIFEST_DIR").to_string() + "/tests/outputs/large/";
    std::fs::create_dir_all(&output).unwrap();

    std::panic::catch_unwind(|| {
        let mut comp = mozjpeg::Compress::new(colorspace);

        comp.set_size(width, height);
        comp.set_mem_dest();
        comp.start_compress();

        assert!(comp.write_scanlines(pixels));

        comp.finish_compress();

        let jpeg_bytes = comp.data_to_vec().unwrap();

        let mut v = OpenOptions::new()
            .write(true)
            .create(true)
            .open(output.clone() + "/" + name)
            .unwrap();

        v.write_all(&jpeg_bytes).unwrap();

        // write to file, etc.
    })
    .unwrap();
}

/// Decodes a large image
#[test]
fn large_no_sampling_factors_rgb()
{
    //
    let path = env!("CARGO_MANIFEST_DIR").to_string() + "/tests/inputs/large_no_samp_7680_4320.jpg";
    let mut decoder = Decoder::new();
    // RGB
    {
        decoder.set_output_colorspace(ColorSpace::RGB);
        let pixels = decoder.decode_file(&path).expect("Test failed decoding");
        write_output(
            "large_no_samp_rgb_7680_4320.jpg",
            &pixels,
            decoder.width() as usize,
            decoder.height() as usize,
            OutColorSpace::JCS_RGB,
        );
    }
}


/// Decodes a large image
#[test]
fn large_vertical_sampling_factors_rgb()
{
    //
    let path = env!("CARGO_MANIFEST_DIR").to_string() + "/tests/inputs/large_vertical_samp_7680_4320.jpg";
    let mut decoder = Decoder::new();
    // RGB
    {
        decoder.set_output_colorspace(ColorSpace::RGB);
        let pixels = decoder.decode_file(&path).expect("Test failed decoding");
        write_output(
            "large_vertical_samp_rgb_7680_4320.jpg",
            &pixels,
            decoder.width() as usize,
            decoder.height() as usize,
            OutColorSpace::JCS_RGB,
        );
    }
}
#[test]
fn large_no_sampling_factors_grayscale()
{
    let path = env!("CARGO_MANIFEST_DIR").to_string() + "/tests/inputs/large_no_samp_7680_4320.jpg";
    let mut decoder = Decoder::new();

    {
        decoder.set_output_colorspace(ColorSpace::GRAYSCALE);
        let pixels = decoder.decode_file(&path).expect("Test failed decoding");
        write_output(
            "large_no_samp_grayscale_7680_4320.jpg",
            &pixels,
            decoder.width() as usize,
            decoder.height() as usize,
            OutColorSpace::JCS_GRAYSCALE,
        );
    }
}

#[test]
fn large_no_sampling_factors_ycbcr()
{
    let path = env!("CARGO_MANIFEST_DIR").to_string() + "/tests/inputs/large_no_samp_7680_4320.jpg";
    let mut decoder = Decoder::new();

    {
        decoder.set_output_colorspace(ColorSpace::YCbCr);
        let pixels = decoder.decode_file(&path).expect("Test failed decoding");
        write_output(
            "large_no_samp_ycbcr_7680_4320.jpg",
            &pixels,
            decoder.width() as usize,
            decoder.height() as usize,
            OutColorSpace::JCS_YCbCr,
        );
    }
}

#[test]
fn large_horizontal_sampling_rgb()
{
    //
    let path =
        env!("CARGO_MANIFEST_DIR").to_string() + "/tests/inputs/large_horiz_samp_7680_4320.jpg";
    let mut decoder = Decoder::new();
    // RGB
    {
        decoder.set_output_colorspace(ColorSpace::RGB);
        let pixels = decoder.decode_file(&path).expect("Test failed decoding");
        write_output(
            "large_horiz_samp_rgb_7680_4320.jpg",
            &pixels,
            decoder.width() as usize,
            decoder.height() as usize,
            OutColorSpace::JCS_RGB,
        );
    }
}
#[test]
fn large_horizontal_sampling_grayscale()
{
    let path =
        env!("CARGO_MANIFEST_DIR").to_string() + "/tests/inputs/large_horiz_samp_7680_4320.jpg";
    let mut decoder = Decoder::new();
    // Grayscale
    decoder.set_output_colorspace(ColorSpace::GRAYSCALE);
    let pixels = decoder.decode_file(&path).expect("Test failed decoding");
    write_output(
        "large_horiz_samp_grayscale_7680_4320.jpg",
        &pixels,
        decoder.width() as usize,
        decoder.height() as usize,
        OutColorSpace::JCS_GRAYSCALE,
    );
}
#[test]
fn large_horizontal_sampling_ycbcr()
{
    let path =
        env!("CARGO_MANIFEST_DIR").to_string() + "/tests/inputs/large_horiz_samp_7680_4320.jpg";
    let mut decoder = Decoder::new();
    // Grayscale
    decoder.set_output_colorspace(ColorSpace::YCbCr);
    let pixels = decoder.decode_file(&path).expect("Test failed decoding");
    write_output(
        "large_horiz_samp_ycbcr_7680_4320.jpg",
        &pixels,
        decoder.width() as usize,
        decoder.height() as usize,
        OutColorSpace::JCS_YCbCr,
    );
}
