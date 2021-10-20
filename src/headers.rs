//! Decode Decoder markers/segments
//!
//! This file deals with decoding header information in a Decoder file
//!
//! A good guide on markers can be found [here](http://vip.sugovica.hu/Sardi/kepnezo/JPEG%20File%20Layout%20and%20Format.htm)

use std::cmp::max;
use std::io::{BufRead, Read};

use crate::components::Components;
use crate::errors::DecodeErrors;
use crate::huffman::HuffmanTable;
use crate::image::ImageInfo;
use crate::marker::Marker;
use crate::misc::{read_byte, read_u16_be, Aligned32, SOFMarkers, UN_ZIGZAG};
use crate::{ColorSpace, Decoder, MAX_DIMENSIONS};

///**B.2.4.2 Huffman table-specification syntax**
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

    // A single DHT marker may contain multiple Huffman Tables.
    while length_read < dht_length
    {
        // HT information
        let ht_info = read_byte(&mut buf);

        // third bit indicates whether the huffman encoding is DC or AC type
        let dc_or_ac = (ht_info >> 4) & 0x01;

        // Indicate the position of this table, should be less than 4;
        let index = (ht_info & 3) as usize;

        // read the number of symbols
        let mut num_symbols: [u8; 17] = [0; 17];

        buf.read_exact(&mut num_symbols[1..17]).map_err(|_| {
            DecodeErrors::HuffmanDecode("Could not read bytes into the buffer".to_string())
        })?;

        let symbols_sum: u16 = num_symbols.iter().map(|f| u16::from(*f)).sum();

        // The sum of the number of symbols cannot be greater than 256;
        if symbols_sum > 256
        {
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

        // store
        match dc_or_ac
        {
            0 =>
            {
                decoder.dc_huffman_tables[index] =
                    Some(HuffmanTable::new(&num_symbols, symbols, true)?);
            }
            _ =>
            {
                decoder.ac_huffman_tables[index] =
                    Some(HuffmanTable::new(&num_symbols, symbols, false)?);
            }
        }
    }

    Ok(())
}

///**B.2.4.1 Quantization table-specification syntax**
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
    // A single DQT header may have multiple QT's
    while qt_length > length_read
    {
        let qt_info = read_byte(&mut buf);

        // If the first bit is set,panic
        if ((qt_info >> 1) & 0x01) != 0
        {
            // bit mathematics
            let second_bit = 2 * ((qt_info >> 2) & 0x01);

            let third_bit = (qt_info >> 3) & 0x01;

            return Err(DecodeErrors::DqtError(format!(
                "Wrong QT bit set,expected value between 0 and 3,found {:?}\n",
                4 + second_bit + third_bit
            )));
        };

        // 0 = 8 bit otherwise 16 bit dqt
        let precision = (qt_info >> 4) as usize;

        // last 4 bits give us position
        let table_position = (qt_info & 0x0f) as usize;

        let precision_value = 64 * (precision + 1);

        let dct_table = match precision
        {
            0 =>
            {
                let mut qt_values = [0; 64];

                buf.read_exact(&mut qt_values).map_err(|x| {
                    DecodeErrors::Format(format!("Could not read symbols into the buffer\n{}", x))
                })?;

                length_read += 7 + precision_value as u16;

                // carry out un zig-zag here
                un_zig_zag(&qt_values)
            }
            1 =>
            {
                // 16 bit quantization tables
                return Err(DecodeErrors::DqtError(
                    "Support for 16 bit quantization table is not complete".to_string(),
                ));
            }
            _ =>
            {
                return Err(DecodeErrors::DqtError(format!(
                    "Expected QT precision value of either 0 or 1, found {:?}",
                    precision
                )));
            }
        };

        decoder.qt_tables[table_position] = Some(dct_table);
    }

    return Ok(());
}

/// Section:`B.2.2 Frame header syntax`

pub(crate) fn parse_start_of_frame<R>(
    buf: &mut R, sof: SOFMarkers, img: &mut Decoder,
) -> Result<(), DecodeErrors>
where
    R: Read,
{
    let mut buf = buf;

    // Get length of the frame header
    let length = read_u16_be(&mut buf)
        .map_err(|_| DecodeErrors::Format("Cannot read SOF length, exhausted data".to_string()))?;

    // usually 8, but can be 12 and 16, we currently support only 8
    // so sorry about that 12 bit images
    let dt_precision = read_byte(&mut buf);

    if dt_precision != 8
    {
        return Err(DecodeErrors::SofError(format!(
            "The library can only parse 8-bit images, the image has {} bits of precision",
            dt_precision
        )));
    }

    img.info.set_density(dt_precision);

    // read  and set the image height.
    let img_height = read_u16_be(&mut buf).map_err(|_| {
        DecodeErrors::Format("Cannot read image height, exhausted data".to_string())
    })?;

    img.info.set_height(img_height);

    // read and set the image width
    let img_width = read_u16_be(&mut buf)
        .map_err(|_| DecodeErrors::Format("Cannot read image width, exhausted data".to_string()))?;

    img.info.set_width(img_width);
    let dimensions = usize::from(img_width) * usize::from(img_height);
    if dimensions > MAX_DIMENSIONS
    {
        return Err(DecodeErrors::LargeDimensions(dimensions));
    }

    // Check image width or height is zero
    if img_width == 0 || img_height == 0
    {
        return Err(DecodeErrors::ZeroError);
    }

    // Number of components for the image.
    let num_components = read_byte(&mut buf);

    // length should be equal to num components
    if length != u16::from(8 + 3 * num_components)
    {
        return Err(DecodeErrors::SofError(format!(
            "Length of start of frame differs from expected {},value is {}",
            u16::from(8 + 3 * num_components),
            length
        )));
    }

    // set number of components
    img.info.components = num_components;

    let mut components = Vec::with_capacity(num_components as usize);

    let mut temp = [0; 3];

    for _ in 0..num_components
    {
        // read 3 bytes for each component
        buf.read_exact(&mut temp)
            .map_err(|x| DecodeErrors::Format(format!("Could not read component data\n{}", x)))?;
        // create a component.
        let component = Components::from(temp)?;

        components.push(component);
    }

    img.info.set_sof_marker(sof);

    for component in &mut components
    {
        // compute interleaved image info

        // h_max contains the maximum horizontal component
        img.h_max = max(img.h_max, component.horizontal_sample);

        // v_max contains the maximum vertical component
        img.v_max = max(img.v_max, component.vertical_sample);

        img.mcu_width = img.h_max * 8;

        img.mcu_height = img.v_max * 8;

        // Number of MCU's per width
        img.mcu_x = (usize::from(img.info.width) + img.mcu_width - 1) / img.mcu_width;

        // Number of MCU's per height
        img.mcu_y = (usize::from(img.info.height) + img.mcu_height - 1) / img.mcu_height;

        if img.h_max != 1 || img.v_max != 1
        {
            // interleaved images have horizontal and vertical sampling factors
            // not equal to 1.
            img.interleaved = true;
        }
        // Extract quantization tables from the arrays into components
        let qt_table = *img.qt_tables[component.quantization_table_number as usize]
            .as_ref()
            .ok_or_else(|| {
                DecodeErrors::DqtError(format!(
                    "No quantization table for component {:?}",
                    component.component_id
                ))
            })?;

        component.quantization_table = Aligned32(qt_table);
    }

    // delete quantization tables, we'll extract them from the components when
    // needed
    img.qt_tables = [None, None, None];

    img.components = components;

    Ok(())
}

/// Parse a start of scan data

pub fn parse_sos<R>(buf: &mut R, image: &mut Decoder) -> Result<(), DecodeErrors>
where
    R: Read + BufRead,
{
    let mut buf = buf;

    // Scan header length
    let ls = read_u16_be(&mut buf)?;

    // Number of image components in scan
    let ns = read_byte(&mut buf);

    if ls != u16::from(6 + 2 * ns)
    {
        return Err(DecodeErrors::SosError(
            "Bad SOS length,corrupt jpeg".to_string(),
        ));
    }
    // Check number of components.
    // Currently ths library doesn't support images with more than # components
    if !(1..4).contains(&ns)
    {
        return Err(DecodeErrors::SosError(format!(
            "Number of components in start of scan should be less than 3 but more than 0. Found {}",
            ns
        )));
    }

    // One component-> Grayscale
    if ns == 1
    {
        image.input_colorspace = ColorSpace::GRAYSCALE;
    }

    // consume spec parameters
    for i in 0..ns
    {
        // CS_i parameter, I don't need it so I might as well delete it
        let _ = read_byte(&mut buf);

        // DC and AC huffman table position
        // top 4 bits contain dc huffman destination table
        // lower four bits contain ac huffman destination table
        let y = read_byte(&mut buf);

        image.components[usize::from(i)].dc_huff_table = usize::from((y >> 4) & 0xF);

        image.components[usize::from(i)].ac_huff_table = usize::from(y & 0xF);
    }

    // Collect the component spec parameters
    if image.info.sof == SOFMarkers::ProgressiveDctHuffman
    {
        // Extract progressive information

        // https://www.w3.org/Graphics/JPEG/itu-t81.pdf
        // Page 42

        // Start of spectral / predictor selection. (between 0 and 63)
        image.ss = read_byte(&mut buf) & 63;

        // End of spectral selection
        image.se = read_byte(&mut buf) & 63;

        if image.se > image.ss
        {
            return Err(DecodeErrors::SosError(
                "End of spectral section smaller than start of spectral selection".to_string(),
            ));
        }
        let bit_approx = read_byte(&mut buf);

        // successive approximation bit position high
        image.ah = bit_approx >> 4;

        // successive approximation bit position low
        image.al = bit_approx & 0xF;
    }
    else
    {
        // ignore three bytes that contain progressive information
        buf.consume(3);
    }

    Ok(())
}

pub fn parse_app<R>(
    mut buf: &mut R, marker: Marker, info: &mut ImageInfo,
) -> Result<(), DecodeErrors>
where
    R: BufRead + Read,
{
    let length = read_u16_be(buf)? as usize;

    let mut bytes_read = 2;

    match marker
    {
        Marker::APP(0) =>
        {
            // The only thing we need is the x and y pixel densities here
            // which are found 10 bytes away
            buf.consume(8);

            let x_density = read_u16_be(&mut buf)?;

            info.set_x(x_density);

            let y_density = read_u16_be(&mut buf)?;

            info.set_y(y_density);
        }
        Marker::APP(1) =>
        {
            if length >= 6
            {
                let mut buffer = [0_u8; 6];

                buf.read_exact(&mut buffer).map_err(|x| {
                    DecodeErrors::Format(format!("Could not read Exif data\n{}", x))
                })?;

                bytes_read += 6;

                // https://web.archive.org/web/20190624045241if_/http://www.cipa.jp:80/std/documents/e/DC-008-Translation-2019-E.pdf
                // 4.5.4 Basic Structure of Decoder Compressed Data
                if &buffer == b"Exif\x00\x00"
                {
                    buf.consume(length - bytes_read);
                }
            }
        }

        _ =>
        {}
    }

    Ok(())
}

/// Small utility function to print Un-zig-zagged quantization tables

fn un_zig_zag(a: &[u8]) -> [i32; 64]
{
    let mut output = [0; 64];

    for i in 0..64
    {
        output[UN_ZIGZAG[i]] = i32::from(a[i]);
    }

    output
}
