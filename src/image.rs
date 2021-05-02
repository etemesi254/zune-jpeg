use crate::errors::{DecodeErrors, UnsupportedSchemes};
use crate::huffman::HuffmanTable;
use crate::markers::{parse_dqt, parse_huffman, parse_sos, parse_start_of_frame};
use crate::misc::{
    remove_ff0, SOFMarkers, END_OF_IMAGE, HUFFMAN_TABLE, QUANTIZATION_TABLE, START_OF_APP_MARKER,
    START_OF_FRAME_BASE, START_OF_FRAME_EXT_AR, START_OF_FRAME_EXT_SEQ, START_OF_FRAME_LOS_SEQ,
    START_OF_FRAME_LOS_SEQ_AR, START_OF_FRAME_PROG_DCT, START_OF_FRAME_PROG_DCT_AR, START_OF_IMAGE,
    START_OF_SCAN,
};
use byteorder::{BigEndian, ReadBytesExt};
use ndarray::Array1;
use std::fs::{metadata, File};
use std::io::{BufRead, BufReader, Read};
use zune_traits::sync::{ColorProfile, ImageTrait};
use zune_traits::image::Image;
use std::path::Path;

#[derive(Clone, Default)]
#[allow(clippy::upper_case_acronyms)]
pub struct JPEG {
    pub(crate) info: ImageInfo,
    pub(crate) qt_tables: Vec<Array1<f64>>,
    pub(crate) dc_huffman_tables: Vec<HuffmanTable>,
    pub(crate) ac_huffman_tables: Vec<HuffmanTable>,

    file_name: Path,
}

impl JPEG {
    pub fn new<P>(file: P) -> JPEG where P:AsRef<Path> {
        JPEG {
            file_name: file,
            ..Self::default()
        }
    }
    /// Get a mutable reference to the image info class
    fn get_mut_info(&mut self) -> &mut ImageInfo {
        &mut self.info
    }
    fn add_qt(&mut self, table: Vec<Array1<f64>>) {
        self.qt_tables.extend_from_slice(table.as_slice());
    }
    #[allow(clippy::needless_pass_by_value)]
    fn add_huffman_tables(&mut self, table: (Vec<HuffmanTable>, Vec<HuffmanTable>)) {
        self.dc_huffman_tables.extend_from_slice(table.0.as_slice());
        self.ac_huffman_tables.extend_from_slice(table.1.as_slice());
    }
    /// Decode a JPEG file
    ///
    /// # Errors
    ///  - `IllegalMagicBytes` - The first two bytes of the image are not `0xffd8`
    ///  - `UnsupportedImage`  - The image encoding scheme is not yet supported, for now we only support
    /// Baseline DCT which is suitable for most images out there
    pub fn decode_file<P>(file: P) -> Result<Image, DecodeErrors> where P:AsRef<Path>{
        let buffer = BufReader::new(File::open(file.clone()).expect("Could not open file"));
        let mut decoder = JPEG {
            file_name: file,
            // Let others be default
            ..JPEG::default()
        };
        let pixels = decoder.decode_internal(buffer)?;
        Ok(pixels)
    }
    fn decode_internal<R>(&mut self, buf: R) -> Result<Image, DecodeErrors>
    where
        R: Read,
    {
        let mut buf = BufReader::new(buf);
        // First two bytes should indicate the image buff
        let magic_bytes = buf
            .read_u16::<BigEndian>()
            .expect("Could not read first two magic bytes");
        if magic_bytes != START_OF_IMAGE {
            return Err(DecodeErrors::IllegalMagicBytes(magic_bytes));
        }
        let mut last_byte = 0;
        loop {
            // read a byte
            let m = buf.read_u8().unwrap_or_else(|err| {
                panic!("Could not read values to buffer \n\t\t Reason: {}", err);
            });
            // create a u16 from the read byte and the previous read by
            let marker = ((last_byte as u16) << 8) | m as u16;

            // Check http://www.vip.sugovica.hu/Sardi/kepnezo/JPEG%20File%20Layout%20and%20Format.htm
            // for meanings of the values below
            match marker {
                // There can be different SOF markers which mean different things
                START_OF_FRAME_BASE => {
                    let info = self.get_mut_info();
                    let marker = SOFMarkers::BaselineDct;
                    debug!("Image encoding scheme =`{:?}`", marker);
                    parse_start_of_frame(&mut buf, marker, info)?;
                }
                // Yet to be supported encoding schemes
                START_OF_FRAME_EXT_SEQ
                | START_OF_FRAME_PROG_DCT
                | START_OF_FRAME_PROG_DCT_AR
                | START_OF_FRAME_LOS_SEQ_AR
                | START_OF_FRAME_LOS_SEQ
                | START_OF_FRAME_EXT_AR => {
                    let feature = UnsupportedSchemes::from_int(marker).unwrap();
                    return Err(DecodeErrors::Unsupported(feature));
                }
                START_OF_APP_MARKER => {
                    debug!("Parsing start of APP 0 segment");
                    // The only thing we need is the x and y pixel densities here
                    // which are found 10 bytes away
                    buf.consume(10);
                    let info = self.get_mut_info();
                    let x_density = buf
                        .read_u16::<BigEndian>()
                        .expect("Could not read x-pixel density");
                    info.set_x(x_density);
                    let y_density = buf
                        .read_u16::<BigEndian>()
                        .expect("Could not read y-pixel density");
                    info.set_y(y_density);
                    debug!("Pixel density acquired");
                }
                // Quantization table
                QUANTIZATION_TABLE => {
                    if self.info.sof.is_lossless() {
                        warn!("A quantization table was found in a lossless encoded jpeg");
                        continue;
                    }
                    self.add_qt(parse_dqt(&mut buf)?);
                    debug!("Quantization tables extracted");
                }
                HUFFMAN_TABLE => {
                    self.add_huffman_tables(parse_huffman(&mut buf)?);
                    debug!("Finished extracting Huffman Table(s)")
                }
                // Start of Scan
                START_OF_SCAN => {
                    debug!("Parsing start of scan");
                    parse_sos(&mut buf, &self.info)?;
                    debug!("Finished parsing start of scan");
                    // break after reading the start of scan.
                    // what follows is the image data
                    break;
                }
                _ => (),
            }
            if marker == END_OF_IMAGE {
                break;
            }
            last_byte = marker
        }

        let data = remove_ff0(buf);
        Ok(self.decode_scan_data_ycbcr(&data))

    }
}
impl ImageTrait for JPEG {
    fn decode_buffer(&mut self, buf: &[u8]) -> Vec<u8> {
        todo!()
    }



    fn width(&self) -> u32 {
        u32::from(self.info.width)
    }
    fn height(&self) -> u32 {
        u32::from(self.info.height)
    }

    fn decode(&mut self) {
        let buffer = BufReader::new(
            File::open(self.file_name.clone())
                .unwrap_or_else(|_| panic!("Could not open file {}", self.file_name)),
        );
        self.decode_internal(buffer)
            .unwrap_or_else(|f| panic!("Error decoding image {}\n{}", self.file_name, f));
    }
    fn color_profile(&self) -> ColorProfile {
        self.info.color_profile
    }

    fn pixel_density(&self) -> (u16, u16) {
        (self.info.x_density as u16, self.info.x_density as u16)
    }

    fn pretty_print(&self) {
        let meta = metadata(self.file_name.clone())
            .unwrap_or_else(|_| panic!("Could not get image data {}", self.file_name.clone()));
        // I override this, because I CAN
        println!("+--------------------------------------------------------+");
        println!("Image File             : {}", self.file_name);
        println!("Image Size             : {:?} bytes", meta.len());
        println!("Image Width            : {:?}", self.width());
        println!("Image Height           : {:?}", self.height());
        println!(
            "Pixel Density          : {:?}x{:?} DPI",
            self.pixel_density().0,
            self.pixel_density().1
        );
        println!("Dimensions             : {:?} pixels", self.dimensions());
        println!("Color Profile          : {:?}", self.color_profile());
        println!("+-------------------------------------------------------+");
    }
}

/// A struct representing Image Information
#[derive(Default, Clone)]
#[allow(clippy::module_name_repetitions)]
pub struct ImageInfo {
    /// Width of the image
    pub width: u16,
    /// Height of image
    pub height: u16,
    /// PixelDensity
    pub pixel_density: u8,
    /// Colour profile
    pub color_profile: ColorProfile,
    /// Start of frame markers
    pub sof: SOFMarkers,
    /// Horizontal sample
    pub x_density: u16,
    /// Vertical sample
    pub y_density: u16,
    pub(crate) components: u8,
}
impl ImageInfo {
    /// Set width of the image
    ///
    /// Found in the start of frame
    pub fn set_width(&mut self, width: u16) {
        self.width = width;
    }
    /// Set height of the image
    ///
    /// Found in the start of frame
    pub fn set_height(&mut self, height: u16) {
        self.height = height
    }
    /// Set the image profile
    ///
    /// Found in the start of Frame data
    pub fn set_profile(&mut self, profile: ColorProfile) {
        self.color_profile = profile
    }
    /// Set the image density
    ///
    /// Found in the start of frame
    pub fn set_density(&mut self, density: u8) {
        self.pixel_density = density
    }
    /// Set image Start of frame marker
    ///
    /// found in the Start of frame header
    pub fn set_sof_marker(&mut self, marker: SOFMarkers) {
        self.sof = marker
    }
    /// Set image x-density(dots per pixel)
    ///
    /// Found in the APP(0) marker
    pub fn set_x(&mut self, sample: u16) {
        self.x_density = sample;
    }
    /// Set image y-density
    ///
    /// Found in the APP(0) marker
    pub fn set_y(&mut self, sample: u16) {
        self.y_density = sample
    }
}
