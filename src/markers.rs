//! Decode JPEG markers/segments
//!
//! This file deals with decoding header information in a JPEG file
//!
//! A good guide on markers can be found [here](http://vip.sugovica.hu/Sardi/kepnezo/JPEG%20File%20Layout%20and%20Format.htm)
//!
use std::io::{BufRead, BufReader, Read};

use byteorder::{BigEndian, ReadBytesExt};
use ndarray::Array1;
use zune_traits::sync::ColorProfile;

use crate::errors::DecodeErrors;
use crate::huffman::HuffmanTable;
use crate::image::ImageInfo;
use crate::misc::{SOFMarkers, UN_ZIGZAG};

/// Parse a Huffman Tree
///
/// |Field                      |Size          |Description
/// ----------------------------|--------------|-------------------------------------------------
/// |Marker Identifier          |2 bytes       |0xff, 0xc4 to identify DHT marker
/// |Length                     |2 bytes       |This specify length of Huffman table
/// |HT information             |1 byte        | bit 0..3 : number of HT (0..3, otherwise error)
/// |                           |              | bit 4    : type of HT, 0 = DC table, 1 = AC table
/// |                           |              |bit 5..7 : not used, must be 0
/// |Number of Symbols          |16 bytes      |Number of symbols with codes of length 1..16,
/// |                           |              |the sum(n) of these bytes is the total number of codes,
/// |                           |              |which must be <= 256
/// |Symbols                    |n bytes       |Table containing the symbols in order of increasing
/// |                           |              |code length ( n = total number of codes ).
#[allow(clippy::similar_names)]
pub fn parse_huffman<R>(
    buf: &mut BufReader<R>,
) -> Result<(Vec<HuffmanTable>, Vec<HuffmanTable>), DecodeErrors>
where
    R: Read,
{
    // This should be the first step in decoding

    // Read the length of the Huffman table
    let dht_length = buf
        .read_u16::<BigEndian>()
        .expect("Could not read Huffman length from image")
        - 2;
    let mut length_read: u16 = 0;
    let mut dc_tables = vec![];
    let mut ac_tables = vec![];
    // A single DHT table may contain multiple HT's
    while length_read < dht_length {
        // HT information
        let mut ht_info: [u8; 1] = [0];
        buf.read_exact(&mut ht_info)
            .expect("Could not read ht info to a buffer");

        // third bit indicates whether the huffman encoding is DC or AC type
        let dc_or_ac = (ht_info[0] >> 4) & 0x01;
        let _index = (ht_info[0] & 0x0f) as usize;
        // read the number of symbols
        let mut num_symbols: [u8; 16] = [0; 16];
        buf.read_exact(&mut num_symbols)
            .expect("Could not read bytes to the buffer");
        // Soo this will panic if it overflows, which is nice since we were to already check if it does.
        // It should not go above 255
        let symbols_sum: u16 = num_symbols.iter().map(|f| u16::from(*f)).sum();
        if symbols_sum > 256 {
            return Err(DecodeErrors::HuffmanDecode(
                "Encountered Huffman table with excessive length in DHT".to_string(),
            ));
        }
        // A table containing symbols in increasing code length
        let mut symbols: Vec<u8> = vec![0; symbols_sum.into()];
        buf.read_exact(&mut symbols)
            .expect("Could not read symbols to the buffer \n");
        length_read += 17 + symbols_sum as u16;
        match dc_or_ac {
            0 => dc_tables.push(HuffmanTable::from(num_symbols, &symbols)),
            _ => ac_tables.push(HuffmanTable::from(num_symbols, &symbols)),
        }
    }
    Ok((dc_tables, ac_tables))
}
/// Parse a DQT tree and carry out unzig-zaging to get the initial
/// matrix after quantization
///
/// |Field               |Size                   |Description
/// ---------------------|-----------------------|-------------------------
/// |Marker Identifier   |2 bytes                |0xff, 0xdb identifies DQT
/// |Length              |2 bytes                |This gives the length of QT.
/// | QT information     |1 byte                 |bit 0..3: number of QT (0..3, otherwise error)
/// |                    |                       |bit 4..7: precision of QT, 0 = 8 bit, otherwise 16 bit
/// | Bytes              |n bytes                |This gives QT values, n = 64*(precision+1)
///
/// Remarks:
///> * A single DQT segment may contain multiple QTs, each with its own information byte.
///> * For precision=1 (16 bit), the order is high-low for each of the 64 words.
///
/// # Errors
/// - `PrecisionError` - Precision value is not zero or 1.
pub fn parse_dqt<R>(buf: &mut BufReader<R>) -> Result<Vec<Array1<f64>>, DecodeErrors>
where
    R: Read,
{
    let qt_length = buf.read_u16::<BigEndian>().expect("Could not read length");
    let mut length_read: u16 = 0;
    // there may be more than one qt table
    let mut qt_tables = Vec::with_capacity(3);
    while qt_length > length_read {
        let qt_info = buf.read_u8().expect("Could not read QT  information");
        // If the first bit is set,panic
        if ((qt_info >> 1) & 0x01) != 0 {
            // bit mathematics
            let second_bit = 2 * ((qt_info >> 2) & 0x01);

            let third_bit = (qt_info >> 3) & 0x01;
            return Err(DecodeErrors::DqtError(format!(
                "Wrong QT bit set,expected value between 0 and 3,found {:?}\n",
                4 + second_bit + third_bit
            )));
        };
        // 0 = 8 bit otherwise 16 bit
        let precision = (qt_info >> 4) as usize;
        let precision_value = 64 * (precision + 1);

        let dct_table = match precision {
            0 => {
                let mut qt_values: Vec<u8> = vec![0; 64];
                buf.read_exact(&mut qt_values)
                    .expect("Could not read symbols to the buffer \n");

                length_read += 7 + precision_value as u16;

                // Map the array to floats for IDCT
                let mut un_zig_zag = Array1::zeros(64);
                (0..qt_values.len())
                    // Okay move value in qt_values[len] to position UN_ZIG_ZAG[len] in the new array
                    .for_each(|len| un_zig_zag[UN_ZIGZAG[len]] = f64::from(qt_values[len]));

                // Return array
                un_zig_zag
            }
            1 => {
                // 16 bit quantization tables
                let mut qt_values: Vec<u16> = vec![0; 64];
                buf.read_u16_into::<BigEndian>(&mut qt_values)
                    .expect("Could not read 16 bit QT table to buffer\n");

                length_read += 7 + precision_value as u16;

                // Map array to floats for IDCT and un zig zag
                let mut un_zig_zag = Array1::zeros(64);
                (0..64)
                    // move item at qt_values[len] to UNZIG_ZAG[len]
                    .for_each(|len| un_zig_zag[UN_ZIGZAG[len]] = f64::from(qt_values[len]));
                // Return array
                un_zig_zag
            }
            _ => {
                return Err(DecodeErrors::DqtError(format!(
                    "Expected precision value of either 0 or 1, found {:?}",
                    precision
                )));
            }
        };
        qt_tables.push(dct_table);
        // Add table to DCT Table
    }
    return Ok(qt_tables);
}

/// Parse a START OF FRAME 0 segment
///
/// See [here](https://www.w3.org/Graphics/JPEG/itu-t81.pdf) page 40
///
/// | Field              |Size        |Description
/// ---------------------|------------|-----------------
/// | Marker Identifier  |2 bytes     |0xff, 0xc0 to identify SOF0 marker
/// | Length             |2 bytes     |This value equals to 8 + components*3 value
/// | Data precision     |1 byte      |This is in bits/sample, usually 8
/// |                    |            |(12 and 16 not supported by most software).
/// |Image height        |2 bytes     |This must be > 0
/// |Image Width         |2 bytes     |This must be > 0
/// |Number of components|1 byte      |Usually 1 = grey scaled, 3 = color `YcbCr` or `YIQ` 4 = color `CMYK`
/// |Each component      |3 bytes     | Read each component data of 3 bytes. It contains,
/// |                    |            | (component Id(1byte)(1 = Y, 2 = Cb, 3 = Cr, 4 = I, 5 = Q),
/// |                    |            | sampling factors (1byte) (bit 0-3 vertical., 4-7 horizontal.),
/// |                    |            | quantization table number (1 byte)).
///
/// # Errors
/// - `ZeroWidthError` - Width of the image is 0
/// - `SOFError` - Length of Start of Frame differs from expected
pub fn parse_start_of_frame<R>(
    buf: &mut BufReader<R>,
    sof: SOFMarkers,
    info: &mut ImageInfo,
) -> Result<(), DecodeErrors>
where
    R: Read,
{
    // Get length of the frame header
    let length = buf.read_u16::<BigEndian>().unwrap();
    // usually 8, but can be 12 and 16
    let dt_precision = buf.read_u8().unwrap();
    info.set_density(dt_precision);
    // read the image height , maximum is 65,536
    let img_height = buf.read_u16::<BigEndian>().unwrap();
    info.set_height(img_height);
    // read image width
    let img_width = buf.read_u16::<BigEndian>().unwrap();

    info.set_width(img_width);
    // Check image width is zero
    if img_width == 0 {
        return Err(DecodeErrors::ZeroWidthError);
    }

    let num_components = buf.read_u8().unwrap();
    // length should be equal to num components
    if length != u16::from(8 + 3 * num_components) {
        return Err(DecodeErrors::SofError(format!(
            "Length of start of frame differs from expected {},value is {}",
            u16::from(8 + 3 * num_components),
            length
        )));
    }
    info.set_profile(ColorProfile::set(num_components));
    // set number of components
    info.components = num_components;
    if (sof == SOFMarkers::ProgressiveDctHuffman || sof == SOFMarkers::ProgressiveDctArithmetic)
        && num_components > 4
    {
        return Err(DecodeErrors::SofError(
            format!("An Image encoded with Progressive DCT cannot have more than 4 components in the frame, image has {}",num_components
        )));
    }

    buf.consume((3 * num_components) as usize);

    // Set the SOF marker
    info.set_sof_marker(sof);
    Ok(())
}
/// Parse a start of scan data
///
/// |Field                       |Size      |Description
/// -----------------------------|-----------|-------------
/// Marker Identifier            |2  bytes   |0xff, 0xda identify SOS marker
/// Length                       |2 bytes    |This must be equal to 6+2*(number of components in scan).
/// Number of components in scan |1 byte     |This must be >= 1 and <=4 (otherwise error), usually 1 or 3
/// Each component               |2 bytes    |For each component, read 2 bytes. It contains,
/// |                            |            |- 1 byte   Component Id (1=Y, 2=Cb, 3=Cr, 4=I, 5=Q),
/// |                            |            |- 1 byte   Huffman table to use :
/// |                            |            | > bit 0..3 : AC table (0..3)
/// |                            |            | bit 4..7 : DC table (0..3)
/// | Ignorable Bytes            |3 bytes     |We have to skip 3 bytes.
pub fn parse_sos<R>(buf: &mut BufReader<R>, _image_info: &ImageInfo) -> Result<(), DecodeErrors>
where
    R: Read,
{
    // Scan header length
    let _ls = buf
        .read_u16::<BigEndian>()
        .expect("Could not read start of scan length");
    // Number of components
    let ns = buf.read_u8().unwrap();
    if !(1..5).contains(&ns) {
        return Err(DecodeErrors::SosError(format!(
            "Number of components in start of scan should be less than 4 but more than 0. Found {}",
            ns
        )));
    }
    for _ in 0..ns {
        let _component_id = buf.read_u8();
        let _huffman_tbl = buf.read_u8();
    }
    buf.read_u24::<BigEndian>().unwrap();
    Ok(())
}
