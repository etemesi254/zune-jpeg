//! Main image logic.
#![allow(clippy::doc_markdown)]

use std::fs::read;
use std::io::{BufRead, Cursor, Read};
use std::path::Path;

use crate::color_convert::choose_ycbcr_to_rgb_convert_func;
use crate::components::{ComponentID, Components, SubSampRatios};
use crate::errors::{DecodeErrors, UnsupportedSchemes};
use crate::headers::{parse_app, parse_dqt, parse_huffman, parse_sos, parse_start_of_frame};
use crate::huffman::HuffmanTable;
use crate::idct::choose_idct_func;
use crate::marker::Marker;
use crate::misc::{read_byte, read_u16_be, Aligned32, ColorSpace, SOFMarkers};
use crate::upsampler::{
    choose_horizontal_samp_function, choose_hv_samp_function, upsample_vertical,
};

/// Maximum components
pub(crate) const MAX_COMPONENTS: usize = 4;

/// Maximum image dimensions supported.
pub(crate) const MAX_DIMENSIONS: usize = 1 << 27;

/// Color conversion function that can convert YcbCr colorspace to RGB(A/X) for
/// 16 values
///
/// The following are guarantees to the following functions
///
/// 1. The `&[i16]` slices passed contain 16 items
///
/// 2. The slices passed are in the following order
///     `y,cb,cr`
///
/// 3. `&mut [u8]` is zero initialized
///
/// 4. `&mut usize` points to the position in the array where new values should
/// be used
///
/// The pointer should
/// 1. Carry out color conversion
/// 2. Update `&mut usize` with the new position

pub type ColorConvert16Ptr = fn(&[i16; 16], &[i16; 16], &[i16; 16], &mut [u8], &mut usize);

/// IDCT  function prototype
///
/// This encapsulates a dequantize and IDCT function which will carry out the
/// following functions
///
/// Multiply each 64 element block of `&mut [i16]` with `&Aligned32<[i32;64]>`
/// Carry out IDCT (type 3 dct) on ach block of 64 i16's
pub type IDCTPtr = fn(&[i16], &Aligned32<[i32; 64]>, usize, usize, usize) -> Vec<i16>;

/// A Decoder Instance
#[allow(clippy::upper_case_acronyms)]
pub struct Decoder
{
    /// Struct to hold image information from SOI
    pub(crate) info: ImageInfo,
    ///  Quantization tables, will be set to none and the tables will
    /// be moved to `components` field
    pub(crate) qt_tables: [Option<[i32; 64]>; MAX_COMPONENTS],
    /// DC Huffman Tables with a maximum of 4 tables for each  component
    pub(crate) dc_huffman_tables: [Option<HuffmanTable>; MAX_COMPONENTS],
    /// AC Huffman Tables with a maximum of 4 tables for each component
    pub(crate) ac_huffman_tables: [Option<HuffmanTable>; MAX_COMPONENTS],
    /// Image components, holds information like DC prediction and quantization
    /// tables of a component
    pub(crate) components: Vec<Components>,

    /// maximum horizontal component of all channels in the image
    pub(crate) h_max: usize,
    // maximum vertical component of all channels in the image
    pub(crate) v_max: usize,
    /// mcu's  width (interleaved scans)
    pub(crate) mcu_width: usize,
    /// MCU height(interleaved scans
    pub(crate) mcu_height: usize,
    /// Number of MCU's in the x plane
    pub(crate) mcu_x: usize,
    /// Number of MCU's in the y plane
    pub(crate) mcu_y: usize,
    /// Is the image interleaved?
    pub(crate) interleaved: bool,

    pub(crate) sub_sample_ratio: SubSampRatios,

    /// Image input colorspace, should be YCbCr for a sane image, might be
    /// grayscale too
    pub(crate) input_colorspace: ColorSpace,
    /// Image output_colorspace, what input colorspace should be converted to
    pub(crate) output_colorspace: ColorSpace,
    // Progressive image details
    /// Is the image progressive?
    pub(crate) is_progressive: bool,

    /// Start of spectral scan
    pub(crate) spec_start: u8,
    /// End of spectral scan
    pub(crate) spec_end: u8,
    /// Successive approximation bit position high
    pub(crate) succ_high: u8,
    /// Successive approximation bit position low
    pub(crate) succ_low: u8,
    /// Number of components.
    pub(crate) num_scans: u8,

    // Function pointers, for pointy stuff.
    /// Dequantize and idct function
    // This is determined at runtime which function to run, statically it's
    // initialized to a platform independent one and during initialization
    // of this struct, we check if we can switch to a faster one which
    // depend on certain CPU extensions.
    pub(crate) idct_func: IDCTPtr,
    // Color convert function which acts on 16 YcbCr values
    pub(crate) color_convert_16: ColorConvert16Ptr,

    pub(crate) z_order: [usize; 4],

    /// restart markers
    pub(crate) restart_interval: usize,
    pub(crate) todo: usize,

    // threads
    pub(crate) num_threads: Option<usize>,
}

impl Default for Decoder
{
    fn default() -> Self
    {
        let color_convert = choose_ycbcr_to_rgb_convert_func(ColorSpace::RGB).unwrap();
        Decoder {
            info: ImageInfo::default(),
            qt_tables: [None, None, None, None],
            dc_huffman_tables: [None, None, None, None],
            ac_huffman_tables: [None, None, None, None],
            components: vec![],

            // Interleaved information
            h_max: 1,
            v_max: 1,
            mcu_height: 0,
            mcu_width: 0,
            mcu_x: 0,
            mcu_y: 0,
            interleaved: false,
            sub_sample_ratio: SubSampRatios::None,

            // Progressive information
            is_progressive: false,
            spec_start: 0,
            spec_end: 0,
            succ_high: 0,
            succ_low: 0,
            num_scans: 0,

            // Function pointers
            idct_func: choose_idct_func(),
            color_convert_16: color_convert,

            // Colorspace
            input_colorspace: ColorSpace::YCbCr,
            output_colorspace: ColorSpace::RGB,

            // This should be kept at par with MAX_COMPONENTS, or until the RFC at
            // https://github.com/rust-lang/rfcs/pull/2920 is accepted
            // Store MCU blocks
            // This should probably be changed..
            z_order: [0; 4],

            restart_interval: 0,
            todo: 0x7fff_ffff,
            num_threads: None,
        }
    }
}

impl Decoder
{
    /// Decode a buffer already in memory
    ///
    /// The buffer should be a valid jpeg file, perhaps created by the command
    /// `std:::fs::read()` or a JPEG file downloaded from the internet.
    ///
    /// # Errors
    /// If the image is not a valid jpeg file
    pub fn decode_buffer(&mut self, buf: &[u8]) -> Result<Vec<u8>, DecodeErrors>
    {
        self.decode_internal(Cursor::new(buf.to_vec()))
    }

    /// Create a new Decoder instance
    #[must_use]
    pub fn new() -> Decoder
    {
        Decoder::default()
    }

    /// Decode a valid jpeg file
    ///
    pub fn decode_file<P>(&mut self, file: P) -> Result<Vec<u8>, DecodeErrors>
    where
        P: AsRef<Path> + Clone,
    {
        //Read to an in memory buffer
        let buffer = Cursor::new(read(file)?);
        info!("File size: {} bytes", buffer.get_ref().len());

        self.decode_internal(buffer)
    }

    /// Returns the image information
    ///
    /// This **must** be called after a subsequent call to `decode_file` or
    /// `decode_buffer` otherwise it will return None
    ///
    #[must_use]
    pub fn info(&self) -> Option<ImageInfo>
    {
        // we check for fails to that call by comparing what we have to the default, if
        // it's default we assume that the caller failed to uphold the
        // guarantees. We can be sure that an image cannot be the default since
        // its a hard panic in-case width or height are set to zero.
        if self.info == ImageInfo::default()
        {
            return None;
        }

        return Some(self.info.clone());
    }

    /// Decode Decoder headers
    ///
    /// This routine takes care of parsing supported headers from a Decoder
    /// image
    ///
    /// # Supported Headers
    ///  - APP(0)
    ///  - SOF(O)
    ///  - DQT -> Quantization tables
    ///  - DHT -> Huffman tables
    ///  - SOS -> Start of Scan
    /// # Unsupported Headers
    ///  - SOF(n) -> Decoder images which are not baseline/progressive
    ///  - DAC -> Images using Arithmetic tables
    ///  - JPG(n)
    fn decode_headers_internal<R>(&mut self, buf: &mut R) -> Result<(), DecodeErrors>
    where
        R: Read + BufRead,
    {
        let mut buf = buf;

        // First two bytes should be jpeg soi marker
        let magic_bytes = read_u16_be(&mut buf)?;
        if magic_bytes != 0xffd8
        {
            return Err(DecodeErrors::IllegalMagicBytes(magic_bytes));
        }
        let mut last_byte = 0;

        loop
        {
            // read a byte
            let m = read_byte(&mut buf)?;
            // Last byte should be 0xFF to confirm existence of a marker since markers look
            // like OxFF(some marker data)
            if last_byte == 0xFF
            {
                let marker = Marker::from_u8(m);
                if let Some(n) = marker
                {
                    self.parse_marker_inner(n, buf)?;

                    if n == Marker::SOS
                    {
                        return Ok(());
                    }
                }
                else
                {
                    error!("Marker 0xFF{:X} not known", m);
                    let length = read_u16_be(buf)?;

                    if length < 2
                    {
                        return Err(DecodeErrors::Format(format!(
                            "Found a marker with invalid length : {}",
                            length
                        )));
                    }
                    warn!("Skipping {} bytes", length - 2);
                    buf.consume((length - 2) as usize);
                }
            }
            last_byte = m;
        }
    }
    pub(crate) fn parse_marker_inner<R: Read + BufRead>(
        &mut self, m: Marker, buf: &mut R,
    ) -> Result<(), DecodeErrors>
    {
        match m
        {
            Marker::SOF(0 | 2) =>
            {
                let marker = {
                    // choose marker
                    if m == Marker::SOF(0)
                    {
                        SOFMarkers::BaselineDct
                    }
                    else
                    {
                        self.is_progressive = true;

                        SOFMarkers::ProgressiveDctHuffman
                    }
                };
                info!("Image encoding scheme =`{:?}`", marker);

                // get components
                parse_start_of_frame(buf, marker, self)?;
            }
            // Start of Frame Segments not supported
            Marker::SOF(v) =>
            {
                let feature = UnsupportedSchemes::from_int(v);

                if let Some(feature) = feature
                {
                    return Err(DecodeErrors::Unsupported(feature));
                }

                return Err(DecodeErrors::Format("Unsupported image format".to_string()));
            }
            // APP(0) segment
            Marker::APP(0 | 1) =>
            {
                parse_app(buf, m, &mut self.info)?;
            }
            // Quantization tables
            Marker::DQT =>
            {
                parse_dqt(self, buf)?;
            }
            // Huffman tables
            Marker::DHT =>
            {
                parse_huffman(self, buf)?;
            }
            // Start of Scan Data
            Marker::SOS =>
            {
                parse_sos(buf, self)?;

                // break after reading the start of scan.
                // what follows is the image data
                return Ok(());
            }
            Marker::EOI => return Err(DecodeErrors::Format("Premature End of image".to_string())),

            Marker::DAC | Marker::DNL =>
            {
                return Err(DecodeErrors::Format(format!(
                    "Parsing of the following header `{:?}` is not supported,\
                                cannot continue",
                    m
                )));
            }
            Marker::DRI =>
            {
                info!("DRI marker present");
                if read_u16_be(buf)? != 4
                {
                    return Err(DecodeErrors::Format(
                        "Bad DRI length, Corrupt JPEG".to_string(),
                    ));
                }
                self.restart_interval = usize::from(read_u16_be(buf)?);
                self.todo = self.restart_interval;
            }
            _ =>
            {
                warn!(
                    "Capabilities for processing marker \"{:?}\" not implemented",
                    m
                );
                let length = read_u16_be(buf)?;

                if length < 2
                {
                    return Err(DecodeErrors::Format(format!(
                        "Found a marker with invalid length:{}\n",
                        length
                    )));
                }
                warn!("Skipping {} bytes", length - 2);
                buf.consume((length - 2) as usize);
            }
        }
        Ok(())
    }

    /// Get the output colorspace the image pixels will be decoded into
    #[must_use]
    pub fn get_output_colorspace(&self) -> ColorSpace
    {
        return self.output_colorspace;
    }

    fn decode_internal(&mut self, buf: Cursor<Vec<u8>>) -> Result<Vec<u8>, DecodeErrors>
    {
        let mut buf = buf;

        self.decode_headers_internal(&mut buf)?;

        if self.is_progressive
        {
            self.decode_mcu_ycbcr_progressive(&mut buf)
        }
        else
        {
            self.decode_mcu_ycbcr_baseline(&mut buf)
        }
    }
    /// Read only headers from a jpeg image buffer
    ///
    /// This allows you to extract important information like
    /// image width and height without decoding the full image
    ///
    /// # Examples
    /// ```no_run
    /// use zune_jpeg::Decoder;
    /// let mut decoder = Decoder::new();
    /// let img_data = std::fs::read("a_valid.jpeg").unwrap();
    /// decoder.read_headers(&img_data).unwrap();
    ///
    /// println!("Total decoder dimensions are : {} pixels",usize::from(decoder.width()) * usize::from(decoder.height()));
    /// println!("Number of components in the image are {}", decoder.info().unwrap().components);
    /// ```
    pub fn read_headers(&mut self, buf: &[u8]) -> Result<(), DecodeErrors>
    {
        let mut cursor = Cursor::new(buf);
        self.decode_headers_internal(&mut cursor)?;
        Ok(())
    }

    /// Set the output colorspace
    ///
    ///# Values which currently work
    ///
    /// - `ColorSpace::RGBX` : Set it to RGB_X where X is anything between 0 and
    ///   255
    /// (cool for playing with alpha channels)
    ///
    /// - `ColorSpace::RGBA` : Set is to RGB_A where A is the alpha channel ,
    ///   useful for converting JPEG
    /// images to PNG images
    ///
    /// - `ColorSpace::RGB` : Use the normal color convert function where YCbCr
    ///   is converted to RGB colorspace.
    ///
    /// - `ColorSpace::GRAYSCALE`:Convert normal image to a black and white
    ///   image(grayscale)
    ///
    /// - `ColorSpace::YCbCr`: Do not do colorspace conversion.
    #[allow(clippy::expect_used)]
    pub fn set_output_colorspace(&mut self, colorspace: ColorSpace)
    {
        if self.output_colorspace == colorspace
        {
            return;
        }
        self.output_colorspace = colorspace;

        match colorspace
        {
            ColorSpace::RGB | ColorSpace::RGBX | ColorSpace::RGBA =>
            {
                let func_ptr = choose_ycbcr_to_rgb_convert_func(colorspace).unwrap();

                self.color_convert_16 = func_ptr;
            }
            // do nothing for others
            _ => (),
        }
    }

    /// Set up-sampling routines in case an image is down sampled
    pub(crate) fn set_upsampling(&mut self) -> Result<(), DecodeErrors>
    {
        // no sampling, return early
        // check if horizontal max ==1
        if self.h_max == self.v_max && self.h_max == 1
        {
            return Ok(());
        }

        // match for other ratios
        match (self.h_max, self.v_max)
        {
            (2, 1) =>
            {
                self.sub_sample_ratio = SubSampRatios::H;
                // horizontal sub-sampling
                info!("Horizontal sub-sampling (2,1)");

                let up_sampler = choose_horizontal_samp_function();
                self.components[1..]
                    .iter_mut()
                    .for_each(|x| x.up_sampler = up_sampler);
            }
            (1, 2) =>
            {
                self.sub_sample_ratio = SubSampRatios::V;
                // Vertical sub-sampling
                info!("Vertical sub-sampling (1,2)");

                self.components[1..]
                    .iter_mut()
                    .for_each(|x| x.up_sampler = upsample_vertical);
            }
            (2, 2) =>
            {
                self.sub_sample_ratio = SubSampRatios::HV;
                // vertical and horizontal sub sampling
                info!("Vertical and horizontal sub-sampling(2,2)");

                self.components[1..]
                    .iter_mut()
                    .for_each(|x| x.up_sampler = choose_hv_samp_function());
            }
            (_, _) =>
            {
                // no op. Do nothing
                // Jokes , panic...
                return Err(DecodeErrors::Format(
                    "Unknown down-sampling method, cannot continue".to_string(),
                ));
            }
        }

        return Ok(());
    }

    /// Set output colorspace to be RGBA
    /// equivalent of calling
    /// ```rust
    /// use zune_jpeg::{Decoder, ColorSpace};
    /// Decoder::new().set_output_colorspace(ColorSpace::RGBA);
    /// ```
    pub fn rgba(&mut self)
    {
        // told you so
        self.set_output_colorspace(ColorSpace::RGBA);
    }

    #[must_use]
    /// Get the width of the image as a u16
    ///
    /// The width lies between 0 and 65535
    pub fn width(&self) -> u16
    {
        self.info.width
    }

    /// Get the height of the image as a u16
    ///
    /// The height lies between 0 and 65535
    #[must_use]
    pub fn height(&self) -> u16
    {
        self.info.height
    }

    /// Set the number of threads the decoder should use during decoding
    ///
    /// This allows the end user to manually set decoding threads
    ///
    /// # Errors
    /// Errors when value is `0`
    ///
    /// # Example
    /// ```
    /// use zune_jpeg::Decoder;
    ///
    /// let mut img = Decoder::new();
    /// img.set_num_threads(3).unwrap(); // spawn only three threads
    /// ```
    pub fn set_num_threads(&mut self, threads: usize) -> Result<(), DecodeErrors>
    {
        if threads == 0
        {
            return Err(DecodeErrors::Format(format!(
                "Cannot set zero threads to decode image"
            )));
        }
        self.num_threads = Some(threads);
        Ok(())
    }
    /// Check that all components have the correct width and height
    /// before continuing to decode
    ///
    /// This helps to identify some corrupt images that may have invalid widths and heights and error out
    /// before trying to decode.
    pub(crate) fn check_component_dimensions(&self) -> Result<(), DecodeErrors>
    {
        // find  y component
        let y_comp = self
            .components
            .iter()
            .find(|c| c.component_id == ComponentID::Y)
            .ok_or_else(|| {
                DecodeErrors::Format(format!("Could not find Y component for the image"))
            })?;

        let y_width = y_comp.width_stride;
        let cb_cr_width = y_width / self.h_max;

        for comp in &self.components
        {
            if comp.component_id == ComponentID::Y
            {
                continue;
            }
            if comp.width_stride != cb_cr_width
            {
                return Err(DecodeErrors::Format(format!("Invalid image width and height stride for component {:?}, expected {}, but found {}", comp.component_id, cb_cr_width,comp.width_stride)));
            }
        }

        Ok(())
    }
}

/// A struct representing Image Information
#[derive(Default, Clone, Eq, PartialEq)]
#[allow(clippy::module_name_repetitions)]
pub struct ImageInfo
{
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
    pub components: u8,
}

impl ImageInfo
{
    /// Set width of the image
    ///
    /// Found in the start of frame

    pub(crate) fn set_width(&mut self, width: u16)
    {
        self.width = width;
    }

    /// Set height of the image
    ///
    /// Found in the start of frame

    pub(crate) fn set_height(&mut self, height: u16)
    {
        self.height = height;
    }

    /// Set the image density
    ///
    /// Found in the start of frame

    pub(crate) fn set_density(&mut self, density: u8)
    {
        self.pixel_density = density;
    }

    /// Set image Start of frame marker
    ///
    /// found in the Start of frame header

    pub(crate) fn set_sof_marker(&mut self, marker: SOFMarkers)
    {
        self.sof = marker;
    }

    /// Set image x-density(dots per pixel)
    ///
    /// Found in the APP(0) marker
    #[allow(dead_code)]
    pub(crate) fn set_x(&mut self, sample: u16)
    {
        self.x_density = sample;
    }

    /// Set image y-density
    ///
    /// Found in the APP(0) marker
    #[allow(dead_code)]
    pub(crate) fn set_y(&mut self, sample: u16)
    {
        self.y_density = sample;
    }
}
