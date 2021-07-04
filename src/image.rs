use std::fs::read;
use std::io::{BufRead, Cursor, Read};
use std::path::Path;

use crate::errors::{DecodeErrors, UnsupportedSchemes};
use crate::headers::{
    parse_app, parse_com, parse_dqt, parse_huffman, parse_sos, parse_start_of_frame,
};
use crate::huffman::HuffmanTable;
use crate::marker::Marker;
use crate::misc::{read_u16_be, read_u8, ColorSpace, SOFMarkers};

use crate::components::Components;

use crate::color_convert::ycbcr_to_rgb;
use crate::idct::dequantize_and_idct_int;
use std::cell::Cell;

/// A Decoder Instance
#[allow(clippy::upper_case_acronyms)]
pub struct Decoder {
    // Struct to hold image information from SOI
    pub(crate) info: ImageInfo,
    //  Quantization tables
    pub(crate) qt_tables: [Option<[i32; 64]>; 4],
    // DC Huffman Tables
    pub(crate) dc_huffman_tables: [Option<HuffmanTable>; 4],
    // AC Huffman Tables
    pub(crate) ac_huffman_tables: [Option<HuffmanTable>; 4],
    // Image component
    pub(crate) components: Cell<Vec<Components>>,

    // These two values are here for static initialization, these functions are found in the hot path
    // and choosing the best function while dispatching is foolish(branch mis-predictions)

    // Dequantize and idct function
    // pass it through function pointer, to allow runtime dispatching for best method case
    pub(crate) idct_func: Box<dyn FnMut(&mut [i32; 64], &[i32; 64])>,
    // Color convert function will act on 8 values of YCbCr blocks
    // This allows us to do AVX and SSE versions more easily( with some weird interleaving code)
    pub(crate) color_convert_func: Box<dyn FnMut(&[i32], &[i32], &[i32], &mut [u8], usize)>,
}
impl Default for Decoder {
    fn default() -> Self {
        Decoder {
            info: Default::default(),
            qt_tables: [None, None, None, None],
            dc_huffman_tables: [None, None, None, None],
            ac_huffman_tables: [None, None, None, None],
            components: Cell::new(vec![]),
            idct_func: Box::new(dequantize_and_idct_int),
            color_convert_func: Box::new(ycbcr_to_rgb),
        }
    }
}
impl Decoder {
    /// Get a mutable reference to the image info class
    fn get_mut_info(&mut self) -> &mut ImageInfo {
        &mut self.info
    }
    /// Add quantization tables to the struct
    fn add_qt(&mut self, table: Vec<([i32; 64], usize)>) {
        for (pos, value) in table.into_iter() {
            // Once  a  quantization  table  has  been  defined  for  a  particular  destination,
            // it  replaces  the  previous  tables  stored  in
            // that destination  and  shall  be  used,  when  referenced,
            self.qt_tables[value] = Some(pos);
        }
    }
    /// Add Huffman Tables to the  decoder
    fn add_huffman_tables(
        &mut self,
        table: (Vec<(HuffmanTable, usize)>, Vec<(HuffmanTable, usize)>),
    ) {
        table.0.into_iter().for_each(|(table, pos)| {
            self.dc_huffman_tables[pos] = Some(table);
        });
        table.1.into_iter().for_each(|(table, pos)| {
            self.ac_huffman_tables[pos] = Some(table);
        });
    }
    /// Decode a buffer already in memory
    ///
    /// The buffer should be a valid Decoder file, perhaps created by the command
    /// `std:::vec::read()`
    ///
    /// # Errors
    /// If the image is not a valid Decoder file
    pub fn decode_buffer(buf: &[u8]) -> Result<Vec<u8>, DecodeErrors> {
        let mut image = Decoder::default();
        image.decode_internal(Cursor::new(buf.to_vec()))
    }
    /// Create a new Decoder instance
    pub fn new() -> Decoder {
        Decoder::default()
    }
    /// Decode a Decoder file
    ///
    /// # Errors
    ///  - `IllegalMagicBytes` - The first two bytes of the image are not `0xffd8`
    ///  - `UnsupportedImage`  - The image encoding scheme is not yet supported, for now we only support
    /// Baseline DCT which is suitable for most images out there
    pub fn decode_file<P>(file: P) -> Result<Vec<u8>, DecodeErrors>
    where
        P: AsRef<Path> + Clone,
    {
        //Read to an in memory buffer
        let buffer = Cursor::new(read(file).expect("Could not open file"));
        let mut decoder = Decoder::default();
        decoder.decode_internal(buffer)
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
                            let mut p =
                                parse_start_of_frame(&mut buf, marker, self.get_mut_info())?;
                            // place the quantization tables in components
                            for i in p.iter_mut() {
                                let q = self.qt_tables[i.quantization_table_number as usize]
                                    .as_ref()
                                    .unwrap_or_else(|| {
                                        panic!(
                                            "No quantization table for component {:?}",
                                            i.component_id
                                        )
                                    })
                                    .clone();
                                i.quantization_table = q;
                            }

                            // delete quantization tables, we'll extract them from the components when needed
                            self.qt_tables = [None, None, None, None];

                            self.components = Cell::new(p);
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
                            debug!("Extracting Quantization tables");
                            let values = parse_dqt(&mut buf)?;

                            self.add_qt(values);
                            debug!("Quantization tables extracted");
                        }
                        // Huffman tables
                        Marker::DHT => {
                            debug!("Extracting Huffman table(s)");
                            self.add_huffman_tables(parse_huffman(&mut buf)?);
                            debug!("Finished extracting Huffman Table(s)")
                        }
                        // Start of Scan Data
                        Marker::SOS => {
                            debug!("Parsing start of scan");
                            parse_sos(&mut buf, &self.info)?;
                            debug!("Finished parsing start of scan");
                            // break after reading the start of scan.
                            // what follows is the image data
                            break;
                        }

                        Marker::DAC | Marker::DNL => {
                            return Err(DecodeErrors::Format(format!(
                                "Parsing of the following header `{:?}` is not supported,\
                                cannot continue",
                                m
                            )))
                        }
                        _ => {
                            warn!(
                                "Capabilities for processing marker `{:?} not implemented",
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
    fn decode_internal(&mut self, buf: Cursor<Vec<u8>>) -> Result<Vec<u8>, DecodeErrors>
    {
        let mut buf = buf;
        self.decode_headers(&mut buf)?;
        self.decode_mcu_ycbcr(&mut buf)
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
    pub fn set_width(&mut self, width: u16) {
        self.width = width;
    }
    /// Set height of the image
    ///
    /// Found in the start of frame
    pub fn set_height(&mut self, height: u16) {
        self.height = height
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
