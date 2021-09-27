//! Decode Decoder markers/segments
//!
//! This file deals with decoding header information in a Decoder file
//!
//! A good guide on markers can be found [here](http://vip.sugovica.hu/Sardi/kepnezo/JPEG%20File%20Layout%20and%20Format.htm)
//!

use std::io::{BufRead, Read};

use crate::components::Components;
use crate::errors::DecodeErrors;
use crate::huffman::HuffmanTable;
use crate::image::ImageInfo;
use crate::marker::Marker;
use crate::misc::{read_u16_be, read_u8, SOFMarkers, UN_ZIGZAG, Aligned32};
use crate::{ColorSpace, Decoder};
use std::cmp::max;

///**B.2.4.2 Huffman table-specification syntax**
/// ----------------------------------------------
///
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
///
/// # Errors
/// `HuffmanDecode` - Encountered errors with excessive length
#[allow(clippy::similar_names)]
pub fn parse_huffman<R>(decoder: &mut Decoder, mut buf: &mut R) -> Result<(), DecodeErrors>
where
    R: Read,
{
    // Read the length of the Huffman table
    let dht_length = read_u16_be(&mut buf).map_err(|_| {
        DecodeErrors::HuffmanDecode("Could not read Huffman length from image".to_string())
    })? - 2;
    // how much have we read
    let mut length_read: u16 = 0;
    // A single DHT table may contain multiple HT's
    while length_read < dht_length {
        // HT information
        let ht_info = read_u8(&mut buf);

        // third bit indicates whether the huffman encoding is DC or AC type
        let dc_or_ac = (ht_info >> 4) & 0x01;
        // Indicate the position of this table, should be less than 4;
        let index = (ht_info & 0x0f) as usize;
        // read the number of symbols
        let mut num_symbols: [u8; 17] = [0; 17];
        buf.read_exact(&mut num_symbols[1..17]).map_err(|_| {
            DecodeErrors::HuffmanDecode("Could not read bytes into the buffer".to_string())
        })?;
        let symbols_sum: u16 = num_symbols.iter().map(|f| u16::from(*f)).sum();
        // the sum should not be above 255
        if symbols_sum > 256 {
            return Err(DecodeErrors::HuffmanDecode(
                "Encountered Huffman table with excessive length in DHT".to_string(),
            ));
        }
        // A table containing symbols in increasing code length
        let mut symbols: Vec<u8> = vec![0; symbols_sum.into()];
        buf.read_exact(&mut symbols).map_err(|x| {
            DecodeErrors::Format(format!("Could not read symbols into the buffer\n{}", x))
        })?;
        length_read += 17 + symbols_sum;
        match dc_or_ac {
            0 => {
                decoder.dc_huffman_tables[index] =
                    Some(HuffmanTable::new(&num_symbols, symbols, true)?);
            }
            _ => {
                decoder.ac_huffman_tables[index] =
                    Some(HuffmanTable::new(&num_symbols, symbols, false)?);
            }
        }
    }
    Ok(())
}

///**B.2.4.1 Quantization table-specification syntax**
/// --------------------------------------------------
///
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
///
/// # Panics
/// The library cannot yet handle 16-bit QT tables.
/// Decoding an image with such tables will cause panic
#[allow(clippy::cast_possible_truncation)]
pub fn parse_dqt<R>(decoder: &mut Decoder, buf: &mut R) -> Result<(), DecodeErrors>
where
    R: Read,
{
    let mut buf = buf;
    // read length
    let qt_length = read_u16_be(&mut buf)
        .map_err(|c| DecodeErrors::Format(format!("Could not read  DQT length {}", c)))?;
    let mut length_read: u16 = 0;
    // we don't un-zig-zag here we do it after dequantization
    while qt_length > length_read {
        let qt_info = read_u8(&mut buf);
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
        // last 4 bits five us position
        let table_position = (qt_info & 0x0f) as usize;
        let precision_value = 64 * (precision + 1);

        let dct_table = match precision {
            0 => {
                let mut qt_values = [0; 64];
                buf.read_exact(&mut qt_values).map_err(|x| {
                    DecodeErrors::Format(format!("Could not read symbols into the buffer\n{}", x))
                })?;

                length_read += 7 + precision_value as u16;
                // carry out un zig-zag here
                un_zig_zag(&qt_values)
            }
            1 => {
                // 16 bit quantization tables
                unimplemented!("Support for 16 bit quantization table is not complete")
            }
            _ => {
                return Err(DecodeErrors::DqtError(format!(
                    "Expected precision value of either 0 or 1, found {:?}",
                    precision
                )));
            }
        };
        decoder.qt_tables[table_position] = Some(dct_table);
        // Add table to DCT Table
    }
    return Ok(());
}

/// Section:`B.2.2 Frame header syntax`
///--------------------------------------
///
/// Parse a START OF FRAME 0 segment
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
/// # Returns
/// A vector containing components in the scan
/// # Errors
/// - `ZeroError` - Width of the image is 0
/// - `SOFError` - Length of Start of Frame differs from expected
pub(crate) fn parse_start_of_frame<R>(
    buf: &mut R,
    sof: SOFMarkers,
    img: &mut Decoder,
) -> Result<(), DecodeErrors>
where
    R: Read,
{
    let mut buf = buf;
    // Get length of the frame header
    let length = read_u16_be(&mut buf)
        .map_err(|_| DecodeErrors::Format("Cannot read SOF length, exhausted data".to_string()))?;
    // usually 8, but can be 12 and 16
    let dt_precision = read_u8(&mut buf);
    if dt_precision != 8 {
        error!(
            "The library can only parse 8-bit images, the image has {} bits",
            dt_precision
        );
    }

    img.info.set_density(dt_precision);
    // read the image height , maximum is 65,536
    let img_height = read_u16_be(&mut buf).map_err(|_| {
        DecodeErrors::Format("Cannot read image height, exhausted data".to_string())
    })?;

    img.info.set_height(img_height);

    // read image width
    let img_width = read_u16_be(&mut buf)
        .map_err(|_| DecodeErrors::Format("Cannot read image width, exhausted data".to_string()))?;

    img.info.set_width(img_width);
    // Check image width or height is zero
    if img_width == 0 || img_height == 0 {
        return Err(DecodeErrors::ZeroError);
    }

    let num_components = read_u8(&mut buf);
    // length should be equal to num components
    if length != u16::from(8 + 3 * num_components) {
        return Err(DecodeErrors::SofError(format!(
            "Length of start of frame differs from expected {},value is {}",
            u16::from(8 + 3 * num_components),
            length
        )));
    }
    // set number of components
    img.info.components = num_components;

    //    if (sof == SOFMarkers::ProgressiveDctHuffman || sof == SOFMarkers::ProgressiveDctArithmetic)
    //      && num_components > 4
    // {
    //    return Err(DecodeErrors::SofError(
    //       format!("An Image encoded with Progressive DCT cannot have more than 4 components in the frame, image has {}",num_components
    //  )));
    //}
    let mut components = Vec::with_capacity(num_components as usize);
    let mut temp = [0; 3];

    for _ in 0..num_components {
        // read 3 bytes for each component
        buf.read_exact(&mut temp)
            .map_err(|x| DecodeErrors::Format(format!("Could not read component data\n{}", x)))?;
        let component = Components::from(temp)?;

        components.push(component);
    }

    // Set the SOF marker
    img.info.set_sof_marker(sof);
    for component in &mut components{
        // compute interleaved image info
        img.h_max = max(img.h_max, component.horizontal_sample);
        img.v_max = max(img.v_max, component.vertical_sample);
        img.mcu_width = img.v_max * 8;
        img.mcu_height = img.h_max * 8;
        img.mcu_x = (usize::from(img.info.width) + img.mcu_width - 1)
            / img.mcu_width;
        img.mcu_y = (usize::from(img.info.height) + img.mcu_height - 1)
            / img.mcu_height;
        // deal with quantization tables
        if img.h_max != 1 || img.v_max != 1 {
            img.interleaved = true;
        }
        let q = *img.qt_tables
            [component.quantization_table_number as usize]
            .as_ref()
            .ok_or_else(|| {
                DecodeErrors::DqtError(format!(
                    "No quantization table for component {:?}",
                    component.component_id
                ))
            })?;
        component.quantization_table = Aligned32(q);
    }
    // delete quantization tables, we'll extract them from the components when needed
    img.qt_tables = [None, None, None];

    img.components = components;
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
pub fn parse_sos<R>(buf: &mut R, image: &mut Decoder) -> Result<(), DecodeErrors>
where
    R: Read + BufRead,
{
    let mut buf = buf;
    // Scan header length
    let ls = read_u16_be(&mut buf)?;
    dbg!(ls-2);

    // Number of image components in scan
    let ns = read_u8(&mut buf);
    dbg!(ls-3);
    if ls != u16::from(6 + 2 * ns) {
        return Err(DecodeErrors::SosError(
            "Bad SOS length,corrupt jpeg".to_string(),
        ));
    }
    //check if it's between 1 and 3(inclusive)
    if !(1..4).contains(&ns) {
        return Err(DecodeErrors::SosError(format!(
            "Number of components in start of scan should be less than 3 but more than 0. Found {}",
            ns
        )));
    }
    // If ns is 1, means image is grayscale,lets change to that
    if ns == 1 {
        image.input_colorspace = ColorSpace::GRAYSCALE;
    }
    // consume spec parameters
    for i in 0..ns {
        dbg!(ls-3-u16::from((i+1)*2));
        let _ = read_u8(&mut buf);


        let y = read_u8(&mut buf);
        image.components[i as usize].dc_huff_table = usize::from((y >> 4) & 0xF);
        image.components[usize::from(i)].ac_huff_table = usize::from(y & 0xF);
        //println!("{},{}", y>>4, y&0xF);
    }
    // Collect the component spec parameters
    if image.info.sof == SOFMarkers::ProgressiveDctHuffman {
        // Extract progressive information

        // https://www.w3.org/Graphics/JPEG/itu-t81.pdf
        // Page 42

        // Start of spectral / predictor selection. (between 0 and 63)
        image.ss = read_u8(&mut buf) & 63;

        // should be between ss and 63
        // End of spectral selection
        image.se = read_u8(&mut buf) & 63;

        let bit_approx = read_u8(&mut buf);

        if image.se > image.ss {
            return Err(DecodeErrors::SosError(
                "End of spectral section smaller than start of spectral selection".to_string(),
            ));
        }

        // successive approximation bit position high
        // top 4 bits
        image.ah = bit_approx >> 4;
        // successive approximation bit position low
        // bottom 4 bits
        image.al = bit_approx & 0xF;

    } else {
        // ignore three bytes that contain progressive information
        buf.consume(3);
    }
    Ok(())
}

pub fn parse_app<R>(
    mut buf: &mut R,
    marker: Marker,
    info: &mut ImageInfo,
) -> Result<(), DecodeErrors>
where
    R: BufRead + Read,
{
    let length = read_u16_be(buf)? as usize;
    let mut bytes_read = 2;

    match marker {
        Marker::APP(0) => {
            // The only thing we need is the x and y pixel densities here
            // which are found 10 bytes away
            buf.consume(8);
            let x_density = read_u16_be(&mut buf)?;
            info.set_x(x_density);
            let y_density = read_u16_be(&mut buf)?;
            info.set_y(y_density);
        }
        Marker::APP(1) => {
            if length >= 6 {
                let mut buffer = [0_u8; 6];
                buf.read_exact(&mut buffer).map_err(|x| {
                    DecodeErrors::Format(format!("Could not read Exif data\n{}", x))
                })?;

                bytes_read += 6;

                // https://web.archive.org/web/20190624045241if_/http://www.cipa.jp:80/std/documents/e/DC-008-Translation-2019-E.pdf
                // 4.5.4 Basic Structure of Decoder Compressed Data
                if &buffer == b"Exif\x00\x00" {
                    buf.consume(length - bytes_read);
                }
            }
        }

        _ => {}
    }
    Ok(())
}

/// Small utility function to print Un-zig-zagged quantization tables
fn un_zig_zag(a: &[u8]) -> [i32; 64] {
    let mut output = [0; 64];
    for i in 0..64 {
        output[UN_ZIGZAG[i]] = i32::from(a[i]);
    }
    output
}
