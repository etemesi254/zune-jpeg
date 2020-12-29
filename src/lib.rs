#[macro_use]
extern crate log;

pub use zune_traits::sync::images::Image;

pub use crate::image::JPEG;

mod bitstream;
pub mod errors;
mod huffman;
mod idct;
pub mod image;
mod markers;
mod misc;
mod mcu;
mod threads;
mod yuv_to_rgb;

fn decode_jpeg() {
    use zune_traits::sync::images::Image;
    let image =
        JPEG::decode_file("/home/caleb/git/simple-jpeg-decoder/samples/lenna.jpg".to_string())
            .unwrap();
    image.pretty_print();
}
