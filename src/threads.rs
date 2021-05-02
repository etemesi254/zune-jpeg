use crate::idct::idct;
use crate::misc::UN_ZIGZAG;
use crate::yuv_to_rgb::yuv_to_rgb_mcu;
use ndarray::{arr2, ArcArray1, Array1, Array2};
use std::ops::IndexMut;
use std::sync::{Arc, Mutex};
use threadpool::{Builder, ThreadPool};

/// Create a thread-pool
fn create_pool() -> ThreadPool {
    Builder::new()
        .thread_name("zune-jpeg".to_string())
        .thread_stack_size(1024)
        .build()
}
/// Parse YCbCr MCU's
///
pub fn parse_threads_ycbcr(
    channels: Vec<[Array1<f64>; 3]>,
    dc_qt: &Array1<f64>,
    ac_qt: &Array1<f64>,
) -> Vec<[Array2<u8>; 3]> {
    let pool = create_pool();
    // Store global channels
    let global_parsed_channel = Arc::new(Mutex::new(vec![
        [arr2(&[[]]), arr2(&[[]]), arr2(&[[]])];
        channels.len()
    ]));

    // DC and AC quantization tables
    let dc_arc = ArcArray1::from_vec(dc_qt.to_vec());
    let ac_arc = ArcArray1::from_vec(ac_qt.to_vec());

    // do idct and all those stuff
    for (pos, mcu) in channels.into_iter().enumerate() {
        let a = dc_arc.clone();
        let b = ac_arc.clone();
        let c = global_parsed_channel.clone();
        pool.execute(move || parse_ycbcr_channels(mcu, pos, a, b, c))
    }
    pool.join();

    let lock_for_rgb = &*global_parsed_channel.lock().unwrap();
    let lock_for_rgb = lock_for_rgb.to_vec();

    return lock_for_rgb;
}

/// Parse YCbCr channels
///
/// This is called by parse_threads_ycbcr to carry out individual parsing
///
/// # Arguments
/// > - channels :`[Array1<f64>;3]` - Contains color components the `Y` component
/// is the first element ,the `Cb` component is the second element and the `Cr` component is the third component
///> - position:`usize`: The Position of this MCU in the image
///> - y_qt:The Luma quantization table
///> - cb_cr_qt: The Chrominance and Luminance quantization table
///> - buf : A Mutex guarded `Vec` where the resulting parsed matrix will be placed
///
///  --------------------------------------------------------------------------------
/// This function carries out the following on each color component, in the order below:
///> - Multiplies each channel by its quantization table (Y channel*y_qt,cb and cr channels by cb_cr_qt)
///> - Undoes run length delta encoding
///> - Applies inverse IDCT on the 8 by 8 matrix
///> - Up samples if needed
///> - Level shifts the matrix, by adding `128` to each element
///> - Carries out YUV to RGB for the MCU
///> - Places the RGB MCU in the buffer
fn parse_ycbcr_channels(
    channels: [Array1<f64>; 3],
    position: usize,
    y_qt: ArcArray1<f64>,
    cb_cr_qt: ArcArray1<f64>,
    buf: Arc<Mutex<Vec<[Array2<u8>; 3]>>>,
) {
    // the fact that they can't clone items to spread them makes
    // us not use the `[arr2(&[[]]);3]` syntax
    let mut parsed_channel: [Array2<u8>; 3] = [arr2(&[[]]), arr2(&[[]]), arr2(&[[]])];

    for (pos, channel) in channels.iter().enumerate() {
        // get appropriate qt table
        let quantization_table = {
            match pos {
                0 => y_qt.clone(),
                _ => cb_cr_qt.clone(),
            }
        };
        // multiply
        let dequantized = channel * quantization_table;
        // undo run length encoding
        let mut un_zig_zagged = un_zig_zag(&dequantized);

        // apply inverse DCT
        idct(&mut un_zig_zagged);

        // todo:up sample if needed

        // level shift samples
        un_zig_zagged += 128.0;

        // modify buffer
        parsed_channel[pos] = un_zig_zagged.mapv(|a| a as u8);
    }
    // convert YCbCr to RGB
    let yuv_to_rgb = yuv_to_rgb_mcu(&parsed_channel[0], &parsed_channel[1], &parsed_channel[2]);
    // modify buffer
    *buf.lock().unwrap().index_mut(position) = yuv_to_rgb;
}
/// Undo run length encoding of the array
///
/// This function creates a new array with the elements arranged before run-length encoding was carried out
///
/// Elements are arranged using the values in the array `UN_ZIGZAG` in misc.rs file
///
/// # Panics
/// If array  does not have 64 elements
fn un_zig_zag(array: &ArcArray1<f64>) -> Array2<f64> {
    let mut new_array = vec![0.0; 64];
    array.iter().enumerate().for_each(|(pos, data)| {
        new_array[UN_ZIGZAG[pos]] = *data;
    });
    return Array2::from_shape_vec((8, 8), new_array).unwrap();
}
