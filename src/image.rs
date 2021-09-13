#![allow(clippy::doc_markdown)]

use std::cmp::max;
use std::fs::read;
use std::io::{BufRead, Cursor, Read};
use std::path::Path;

#[cfg(feature = "x86")]
use crate::color_convert::{ycbcr_to_rgb_avx2, ycbcr_to_rgb_sse, ycbcr_to_rgb_sse_16, ycbcr_to_rgba,
                           ycbcr_to_rgba_sse_16, ycbcr_to_rgbx};
use crate::color_convert::{ycbcr_to_rgb, ycbcr_to_rgb_16};
use crate::components::Components;
use crate::errors::{DecodeErrors, UnsupportedSchemes};
use crate::headers::{parse_app, parse_dqt, parse_huffman, parse_sos, parse_start_of_frame};
use crate::huffman::HuffmanTable;
#[cfg(feature = "x86")]
use crate::idct::dequantize_and_idct_avx2;
use crate::idct::dequantize_and_idct_int;
use crate::marker::Marker;
use crate::misc::{Aligned32, ColorSpace, read_u16_be, read_u8, SOFMarkers};
use crate::upsampler::{upsample_horizontal, upsample_horizontal_sse, upsample_horizontal_vertical, upsample_vertical};

// avx optimized color IDCT

/// Maximum components
pub(crate) const MAX_COMPONENTS: usize = 3;

/// Color conversion function that can convert YcbCr colorspace to RGB(A/X) for 16 values
///
/// The following are guarantees to the following functions
///
/// 1. The `&[i16]` slices passed contain 8 items
///
/// 2. The slices passed are in the following order
///     `y,y,cb,cb,cr,cr`
///
/// 3. `&mut [u8]` is zero initialized
///
/// 4. `&mut usize` points to the position in the array where new values should be used
///
/// The pointer should
/// 1. Carry out color conversion
/// 2. Update `&mut usize` with the new position
pub type ColorConvert16Ptr = fn(&[i16], &[i16], &[i16], &[i16], &[i16], &[i16], &mut [u8], &mut usize);

/// Color convert function  that can convert YCbCr colorspace to RGB(A/X) for 8 values
///
/// The following are guarantees to the values passed by the following functions
///
/// 1. The `&[i16]` slices passed contain 8 items
/// 2. Slices are passed in the following order
///     'y,cb,cr'
///
/// The other guarantees are the same as `ColorConvert16Ptr`
pub type ColorConvertPtr = fn(&[i16], &[i16], &[i16], &mut [u8], &mut usize);
/// IDCT  function prototype
///
/// This encapsulates a dequantize and IDCT function which will carry out the following functions
///
/// Multiply each 64 element block of `&mut [i16]` with `&Aligned32<[i32;64]>`
/// Carry out IDCT (type 3 dct) on ach block of 64 i16's
pub type IDCTPtr = fn(&mut [i16], &Aligned32<[i32; 64]>);

/// A Decoder Instance
#[allow(clippy::upper_case_acronyms)]
pub struct Decoder {
    /// Struct to hold image information from SOI
    pub(crate) info: ImageInfo,
    ///  Quantization tables, will be set to none and the tables will
    /// be moved to `components` field
    pub(crate) qt_tables: [Option<[i32; 64]>; MAX_COMPONENTS],
    /// DC Huffman Tables with a maximum of 4 tables for each  component
    pub(crate) dc_huffman_tables: [Option<HuffmanTable>; MAX_COMPONENTS],
    /// AC Huffman Tables with a maximum of 4 tables for each component
    pub(crate) ac_huffman_tables: [Option<HuffmanTable>; MAX_COMPONENTS],
    /// Image components, holds information like DC prediction and quantization tables of a component
    pub(crate) components: Vec<Components>,

    /// maximum horizontal component of all channels in the image
    pub(crate) h_max: usize,
    // maximum vertical component of all channels in the image
    pub(crate) v_max: usize,
    // mcu's  width (interleaved scans)
    pub(crate) mcu_width: usize,
    // MCU height(interleaved scans
    pub(crate) mcu_height: usize,
    pub(crate) mcu_x: usize,
    pub(crate) mcu_y: usize,
    /// Is the image interleaved?
    pub(crate) interleaved: bool,
    /// Image input colorspace, should be YCbCr for a sane image, might be grayscale too
    pub(crate) input_colorspace: ColorSpace,
    /// Image output_colorspace, what input colorspace should be converted to
    pub(crate) output_colorspace: ColorSpace,
    /// Area for unprocessed values we encountered during processing
    pub(crate) mcu_block: [Vec<i16>; MAX_COMPONENTS],
    /// Function pointers, for pointy stuff.

    /// Dequantize and idct function
    ///
    /// This is determined at runtime which function to run, statically it's initialized to
    /// a platform independent one and during initialization of this struct, we check if we can
    /// switch to a faster one which depend on certain CPU extensions.
    pub(crate) idct_func: IDCTPtr,
    // Color convert function will act on 8 values of YCbCr blocks
    pub(crate) color_convert: ColorConvertPtr,
    // Color convert function which acts on 16 YcbCr values
    pub(crate) color_convert_16: ColorConvert16Ptr,
}

impl Default for Decoder {
    fn default() -> Self {
        let mut d = Decoder {
            info: ImageInfo::default(),
            qt_tables: [None, None, None],
            dc_huffman_tables: [None, None, None],
            ac_huffman_tables: [None, None, None],
            components: vec![],
            h_max: 1,
            v_max: 1,
            mcu_height: 0,
            mcu_width: 0,
            mcu_x: 0,
            mcu_y: 0,
            interleaved: false,
            idct_func: dequantize_and_idct_int,
            color_convert: ycbcr_to_rgb,
            // TODO:Add a platform independent version of this
            color_convert_16: ycbcr_to_rgb_16,
            input_colorspace: ColorSpace::YCbCr,
            output_colorspace: ColorSpace::RGB,
            // This should be kept at par with MAX_COMPONENTS, or until the RFC at
            // https://github.com/rust-lang/rfcs/pull/2920 is accepted
            mcu_block: [vec![], vec![], vec![]],
        };
        d.init();
        return d;
    }
}

impl Decoder {
    /// Get a mutable reference to the image info class
    fn get_mut_info(&mut self) -> &mut ImageInfo {
        &mut self.info
    }
    /// Decode a buffer already in memory
    ///
    /// The buffer should be a valid jpeg file, perhaps created by the command
    /// `std:::fs::read()` or a JPEG file downloaded from the internet.
    ///
    /// # Errors
    /// If the image is not a valid jpeg file
    ///
    /// # Examples
    /// ```
    /// use zune_jpeg::Decoder;
    /// let file = std::fs::read("/a_valid_jpeg.jpg").unwrap();
    /// // Decode the file, panic in case something fails
    /// let decoder= Decoder::new().decode_buffer(file.as_ref())
    /// .expect("Error could not decode the file");
    /// ```
    pub fn decode_buffer(&mut self, buf: &[u8]) -> Result<Vec<u8>, DecodeErrors> {
        self.decode_internal(Cursor::new(buf.to_vec()))
    }
    /// Create a new Decoder instance
    #[must_use]
    pub fn new() -> Decoder {
        Decoder::default()
    }
    /// Decode a Decoder file
    ///
    /// # Errors
    ///  - `IllegalMagicBytes` - The first two bytes of the image are not `0xffd8`
    ///  - `UnsupportedImage`  - The image encoding scheme is not yet supported, for now we only support
    /// Baseline DCT which is suitable for most images out there
    pub fn decode_file<P>(&mut self, file: P) -> Result<Vec<u8>, DecodeErrors>
        where
            P: AsRef<Path> + Clone,
    {
        //Read to an in memory buffer
        let buffer = Cursor::new(read(file).expect("Could not open file"));
        self.decode_internal(buffer)
    }
    /// Returns the image information
    ///
    /// This **must** be called after a subsequent call to `decode_file` or `decode_buffer` otherwise it will return None
    ///
    /// # Example
    /// ```
    ///
    /// use zune_jpeg::Decoder;
    /// let file = std::fs::read("/a_valid_jpeg.jpg").unwrap();
    /// // Decode the file, panic in case something fails
    /// let mut decoder= Decoder::new();
    /// let pixels=decoder.decode_buffer(file.as_ref())
    /// .expect("Error could not decode the file");
    /// // okay now get info
    /// let info = decoder.info().unwrap();
    /// // Print width and height
    /// println!("{},{}",info.width,info.height);
    /// // Assert that all pixels are in the image
    /// assert!(usize::from(info.width)*usize::from(info.height)*decoder.get_output_colorspace().num_components(),pixels.len())
    ///
    /// ```
    #[must_use]
    pub fn info(&self) -> Option<ImageInfo> {
        // we check for fails to that call by comparing what we have to the default, if it's default we
        // assume that the caller failed to uphold the guarantees.
        // We can be sure that an image cannot be the default since its a hard panic in-case width or height are set to zero.
        if self.info == ImageInfo::default() {
            return None;
        }
        return Some(self.info.clone());
    }
    /// Decode Decoder headers
    ///
    /// This routine takes care of parsing supported headers from a Decoder image
    ///
    /// # Supported Headers
    ///  - APP(0)
    ///  - SOF(O)
    ///  - DQT -> Quantization tables
    ///  - DHT -> Huffman tables
    ///  - SOS -> Start of Scan
    /// # Unsupported Headers
    ///  - SOF(n) -> Decoder images which are not baseline
    ///  - DAC -> Images using Arithmetic tables
    fn decode_headers<R>(&mut self, buf: &mut R) -> Result<(), DecodeErrors>
        where
            R: Read + BufRead,
    {
        let mut buf = buf;

        // First two bytes should indicate the image buff
        let magic_bytes = read_u16_be(&mut buf).expect("Could not read the first 2 bytes");
        if magic_bytes != 0xffd8 {
            return Err(DecodeErrors::IllegalMagicBytes(magic_bytes));
        }
        let mut last_byte = 0;
        loop {
            // read a byte
            let m = read_u8(&mut buf);
            // Last byte should be 0xFF to confirm existence of a marker since markers look like OxFF(some marker data)
            if last_byte == 0xFF {
                let marker = Marker::from_u8(m);

                // Check http://www.vip.sugovica.hu/Sardi/kepnezo/JPEG%20File%20Layout%20and%20Format.htm
                // for meanings of the values below
                if let Some(m) = marker {
                    match m {
                        Marker::SOF(0) => {
                            let marker = SOFMarkers::BaselineDct;
                            debug!("Image encoding scheme =`{:?}`", marker);
                            // get components
                            let mut p = parse_start_of_frame(&mut buf, marker, self)?;
                            // place the quantization tables in components
                            for component in &mut p {
                                // compute interleaved image info
                                self.h_max = max(self.h_max, component.horizontal_sample);
                                self.v_max = max(self.v_max, component.vertical_sample);
                                self.mcu_width = self.v_max * 8;
                                self.mcu_height = self.h_max * 8;
                                self.mcu_x = (usize::from(self.info.width) + self.mcu_width - 1)
                                    / self.mcu_width;
                                self.mcu_y = (usize::from(self.info.height) + self.mcu_height - 1)
                                    / self.mcu_height;
                                // deal with quantization tables
                                if self.h_max != 1 || self.v_max != 1 {
                                    self.interleaved = true;
                                }
                                let q = *self.qt_tables
                                    [component.quantization_table_number as usize]
                                    .as_ref()
                                    .unwrap_or_else(|| {
                                        panic!(
                                            "No quantization table for component {:?}",
                                            component.component_id
                                        );
                                    });
                                component.quantization_table = Aligned32(q);
                            }

                            // delete quantization tables, we'll extract them from the components when needed
                            self.qt_tables = [None, None, None];

                            self.components = p;
                        }
                        // Start of Frame Segments not supported
                        Marker::SOF(v) => {
                            let feature = UnsupportedSchemes::from_int(v);
                            if let Some(feature) = feature {
                                return Err(DecodeErrors::Unsupported(feature));
                            }
                            return Err(DecodeErrors::Format(
                                "Unsupported image format".to_string(),
                            ));
                        }
                        // APP(0) segment
                        Marker::APP(_) => {
                            parse_app(&mut buf, m, self.get_mut_info())?;
                        }
                        // Quantization tables
                        Marker::DQT => {
                            parse_dqt(self, &mut buf)?;
                        }
                        // Huffman tables
                        Marker::DHT => {
                            parse_huffman(self, &mut buf)?;
                        }
                        // Start of Scan Data
                        Marker::SOS => {
                            parse_sos(&mut buf, self)?;
                            // break after reading the start of scan.
                            // what follows is the image data
                            break;
                        }

                        Marker::DAC | Marker::DNL => {
                            return Err(DecodeErrors::Format(format!(
                                "Parsing of the following header `{:?}` is not supported,\
                                cannot continue",
                                m
                            )));
                        }
                        _ => {
                            warn!(
                                "Capabilities for processing marker \"{:?}\" not implemented",
                                m
                            );
                        }
                    }
                }
            }
            last_byte = m;
        }
        Ok(())
    }
    /// Get the output colorspace the image pixels will be decoded into
    #[must_use]
    pub fn get_output_colorspace(&self) -> ColorSpace {
        return self.output_colorspace;
    }
    fn decode_internal(&mut self, buf: Cursor<Vec<u8>>) -> Result<Vec<u8>, DecodeErrors> {
        let mut buf = buf;
        self.decode_headers(&mut buf)?;
        // if the image is interleaved
        if self.interleaved {
            self.decode_mcu_ycbcr_interleaved(&mut buf)
        } else {
            Ok(self.decode_mcu_ycbcr_non_interleaved(&mut buf))
        }
    }
    /// Initialize the most appropriate functions for
    ///
    fn init(&mut self) {
        // Set color convert function
        #[cfg(feature = "x86")]
            {
                debug!("Running on x86 ARCH using accelerated decoding routines");
                #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
                    {
                        if is_x86_feature_detected!("avx2") {
                            debug!("Detected AVX code support");

                            debug!("Using AVX optimised IDCT");
                            self.idct_func = dequantize_and_idct_avx2;
                            debug!("Using AVX optimised color conversion functions");
                            match self.output_colorspace {
                                ColorSpace::RGB => {
                                    self.color_convert_16 = ycbcr_to_rgb_avx2;
                                }
                                ColorSpace::RGBA => self.color_convert_16 = ycbcr_to_rgba,
                                ColorSpace::RGBX => self.color_convert_16 = ycbcr_to_rgbx,
                                _ => {}
                            }
                        } else if is_x86_feature_detected!("sse2") {
                            debug!("No support for avx2 switching to sse");
                            debug!("Using sse color convert functions");
                            match self.output_colorspace {
                                ColorSpace::RGB => {
                                    self.color_convert_16 = ycbcr_to_rgb_sse_16;
                                    self.color_convert = ycbcr_to_rgb_sse;
                                }
                                ColorSpace::RGBA | ColorSpace::RGBX => {
                                    // Ideally I make RGBA  and RGBX the same because 255 for an alpha channel is still random...
                                    self.color_convert_16 = ycbcr_to_rgba_sse_16;
                                }

                                _ => {}
                            }
                        }
                    }
                #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
                    {
                        // use the slower version which doesn't depend on  platform specific
                        // intrinsics
                        debug!("Using Table lookup color conversion function");
                        self.color_convert_16 = ycbcr_to_rgb_16;
                        self.color_convert_func = ycbcr_to_rgb;
                    }
            }
        #[cfg(not(feature = "x86"))]
            {
                debug!("Using safer(and slower) color conversion functions");
                self.color_convert = ycbcr_to_rgb;
                self.color_convert_16 = ycbcr_to_rgb_16;
            }
    }
    /// Set the output colorspace
    ///
    /// # Currently Works on x86_64 CPU's with avx2 instruction set
    /// Now that that's cleared,  the following options exist
    ///
    ///- `ColorSpace::RGBX` : Set it to RGB_X where X is anything between 0 and 255
    /// (cool for playing with alpha channels)
    ///
    /// - `ColorSpace::RGBA` : Set is to RGB_A where A is the alpha channel , useful for converting JPEG
    /// images to PNG images
    ///
    /// - `ColorSpace::RGB` : Use the normal color convert function where YCbCr is converted to RGB colorspace.
    pub fn set_output_colorspace(&mut self, colorspace: ColorSpace) {
        self.output_colorspace = colorspace;
        // change color convert function

        #[cfg(all(any(target_arch = "x86", target_arch = "x86_64"), feature = "x86"))]
            {
                // Check for avx2 feature set on x86 cpu's
                if is_x86_feature_detected!("avx2") {
                    // set appropriate color space
                    match self.output_colorspace {
                        ColorSpace::RGB => {
                            self.color_convert_16 = ycbcr_to_rgb_avx2;
                        }
                        ColorSpace::RGBA => self.color_convert_16 = ycbcr_to_rgba,
                        ColorSpace::RGBX => self.color_convert_16 = ycbcr_to_rgbx,
                        _ => {}
                    }
                }
            }
    }
    /// Set upsampling routines in case an image is down sampled
    pub(crate) fn set_upsampling(&mut self) -> Result<(), DecodeErrors> {
        // no sampling, return early
        // check if horizontal max ==1
        if self.h_max == self.v_max && self.h_max == 1 {
            return Ok(());
        }
        // match for other ratios
        match (self.h_max, self.v_max) {
            (2, 1) => {
                // horizontal sub-sampling
                debug!("Horizontal sub-sampling (2,1)");
                // Change all sub sampling to be horizontal. This also changes the Y component which
                // should **NOT** be up-sampled, so it's the responsibility of the caller to ensure that
                if is_x86_feature_detected!("sse2") {
                    self.components.iter_mut().for_each(|x| x.up_sampler = upsample_horizontal_sse);
                } else {
                    self.components.iter_mut().for_each(|x| x.up_sampler = upsample_horizontal);
                }
            }
            (1, 2) => {
                // Vertical sub-sampling
                debug!("Vertical sub-sampling (1,2)");
                self.components.iter_mut().for_each(|x| x.up_sampler = upsample_vertical);
            }
            (2, 2) => {
                // vertical and horizontal sub sampling
                debug!("Vertical and horizontal sub-sampling(2,2)");
                self.components.iter_mut().for_each(|x| x.up_sampler = upsample_horizontal_vertical);
            }
            (_, _) => {
                // no op. Do nothing
                // jokes , panic...
                return Err(DecodeErrors::Format("Unknown down-sampling method, cannot continue".to_string()));
            }
        }
        return Ok(());
    }
    #[must_use]
    /// Get the width of the image as a u16
    ///
    /// The width lies between 0 and 65535
    pub fn width(&self) -> u16 {
        self.info.width
    }

    /// Get the height of the image as a u16
    ///
    /// The height lies between 0 and 65535
    #[must_use]
    pub fn height(&self) -> u16 {
        self.info.height
    }
}

/// A struct representing Image Information
#[derive(Default, Clone, Eq, PartialEq)]
#[allow(clippy::module_name_repetitions)]
pub struct ImageInfo {
    /// Width of the image
    pub width: u16,
    /// Height of image
    pub height: u16,
    /// PixelDensity
    pub pixel_density: u8,
    /// Start of frame markers
    pub sof: SOFMarkers,
    /// Horizontal sample
    pub x_density: u16,
    /// Vertical sample
    pub y_density: u16,
    /// Number of components
    pub(crate) components: u8,
}

impl ImageInfo {
    /// Set width of the image
    ///
    /// Found in the start of frame
    pub(crate) fn set_width(&mut self, width: u16) {
        self.width = width;
    }
    /// Set height of the image
    ///
    /// Found in the start of frame
    pub(crate) fn set_height(&mut self, height: u16) {
        self.height = height;
    }
    /// Set the image density
    ///
    /// Found in the start of frame
    pub(crate) fn set_density(&mut self, density: u8) {
        self.pixel_density = density;
    }
    /// Set image Start of frame marker
    ///
    /// found in the Start of frame header
    pub(crate) fn set_sof_marker(&mut self, marker: SOFMarkers) {
        self.sof = marker;
    }
    /// Set image x-density(dots per pixel)
    ///
    /// Found in the APP(0) marker
    pub(crate) fn set_x(&mut self, sample: u16) {
        self.x_density = sample;
    }
    /// Set image y-density
    ///
    /// Found in the APP(0) marker
    pub(crate) fn set_y(&mut self, sample: u16) {
        self.y_density = sample;
    }
}
