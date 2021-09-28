use zune_jpeg::*;

const BASELINE: &'static str = "/test-images/test-baseline.jpg";

const ARITHMETIC: &'static str = "/test-images/test-arithmetic-coding.jpg";

const PROGRESSIVE: &'static str = "/test-images/test-progressive.jpg";

const RESTART_MARKERS: &'static str = "/test-images/test-restart-markers.jpg";

#[test]

fn progressive_dct_arithmetic()
{
    let file = String::from(env!("CARGO_MANIFEST_DIR")) + ARITHMETIC;

    let jpeg = JPEG::decode_file(file).expect("Not there yet \n");
}

#[test]

fn baseline_dct()
{
    let file = String::from(env!("CARGO_MANIFEST_DIR")) + BASELINE;

    let jpeg = JPEG::decode_file(file).expect("Not there yet \n");
}

#[test]

fn restart_markers()
{
    let file = String::from(env!("CARGO_MANIFEST_DIR")) + RESTART_MARKERS;

    let jpeg = JPEG::decode_file(file).expect("Not there yet \n");
}

#[test]

fn progressive()
{
    let file = String::from(env!("CARGO_MANIFEST_DIR")) + PROGRESSIVE;

    let jpeg = JPEG::decode_file(file).expect("Not there yet \n");
}
