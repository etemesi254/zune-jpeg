use std::io::Cursor;

use crate::bitstream::BitStream;
use crate::components::ComponentID;
use crate::Decoder;
use crate::errors::DecodeErrors;
use crate::headers::{parse_huffman, parse_sos};
use crate::marker::Marker;
use crate::misc::read_byte;

// Some notes on progressive decoding
//
// Spectral selection-> Send data as rows from  1..63
//
// successive approximation-> Send data as columns from 0..7
//
// Spec page 120.
impl Decoder
{
    /// Decode a progressive image with no interleaving.
    pub(crate) fn decode_mcu_ycbcr_progressive(
        &mut self, reader: &mut Cursor<Vec<u8>>,
    ) -> Result<Vec<u8>, DecodeErrors>
    {
        let mut temporary_block = [vec![], vec![], vec![]];
        let output_buffer_size = usize::from(self.width() + 7) * usize::from(self.height() + 7);

        for block in temporary_block.iter_mut().take(self.input_colorspace.num_components())
        {
            *block = vec![0; output_buffer_size];
        }
        let mut stream = BitStream::new_progressive(
            self.succ_high,
            self.succ_low,
            self.spec_start,
            self.spec_end,
        );

        // there are multiple scans in the stream, this should resolve the first scan
        // if it returns an EOI marker,it means we are already done,
        // but if it returns any other marker, we are supposed to parse it
        self.parse_entropy_coded_data(reader, &mut stream, &mut temporary_block)?;

        // extract marker
        let mut marker = stream.marker.unwrap();
        stream.marker = None;
        // if marker is EOI, we are done, otherwise continue scanning.
        'eoi: while marker != Marker::EOI
        {
            match marker
            {
                Marker::DHT =>
                    {
                        parse_huffman(self, reader)?;
                    }
                Marker::SOS =>
                    {
                        // after every SOS, marker, read data aSOS parameters and
                        // parse data for that scan.
                        parse_sos(reader, self)?;

                        stream.update_progressive_params(
                            self.succ_high,
                            self.succ_low,
                            self.spec_start,
                            self.spec_end,
                        );

                        self.parse_entropy_coded_data(reader, &mut stream, &mut temporary_block)?;
                        // extract marker, might either indicate end of image or we continue
                        // scanning(hence the continue statement to determine).

                        let tmp_marker = get_marker(reader, &mut stream);
                        if let Some(m) = tmp_marker {
                            marker = m;
                        } else {
                            // read until we get a marker.
                            let length = reader.get_ref().len();
                            let mut t = reader.position() as usize;
                            while t < length
                            {
                                let v = read_byte(reader);
                                if v == 255
                                {
                                    let r = read_byte(reader);
                                    if r != 0
                                    {
                                        marker = Marker::from_u8(r).unwrap();
                                        break;
                                    }
                                }
                                t += 1;
                            }
                        }
                        stream.reset();
                        continue 'eoi;
                    }
                _ =>
                    {
                        break 'eoi;
                    }
            }
            marker = self.get_marker(reader, &mut stream).unwrap();
        }

        return Ok(Vec::new());
    }

    #[rustfmt::skip]
    fn parse_entropy_coded_data(
        &mut self, reader: &mut Cursor<Vec<u8>>, stream: &mut BitStream, buffer: &mut [Vec<i16>; 3],
    ) -> Result<bool, DecodeErrors>
    {
        stream.reset();

        self.components.iter_mut().for_each(|x| x.dc_pred = 0);
        if self.num_scans == 1
        {
            // non interleaved data, process one block at a time in trivial scanline order
            let k = self.z_order[0];

            let (mcu_width, mcu_height);
            // For Y channel  or non interleaved scans , mcu's is the image dimensions divided
            // by 8
            if self.components[k].component_id == ComponentID::Y || !self.interleaved
            {
                mcu_width = ((self.info.width + 7) / 8) as usize;

                mcu_height = ((self.info.height + 7) / 8) as usize;
            } else {
                // For other channels, in an interleaved mcu, number of MCU's
                // are determined by some weird maths done in headers.rs->parse_sos()
                mcu_width = self.mcu_x;

                mcu_height = self.mcu_y;
            }

            for i in 0..mcu_height
            {
                for j in 0..mcu_width
                {
                    let start = 64 * (j + i * (self.components[k].width_stride / 8));

                    let data: &mut [i16; 64] = buffer.get_mut(k).unwrap().get_mut(start..start + 64)
                        .unwrap().try_into().unwrap();

                    if self.spec_start == 0
                    {
                        let pos = self.components[k].dc_huff_table;

                        let dc_table = self.dc_huffman_tables.get(pos).unwrap().as_ref().unwrap();

                        let dc_pred = &mut self.components[k].dc_pred;

                        stream.decode_prog_dc(reader, dc_table, &mut data[0], dc_pred)?;
                    } else {
                        let pos = self.components[k].ac_huff_table;
                        let ac_table = self.ac_huffman_tables.get(pos).unwrap().as_ref().unwrap();

                        stream.decode_prog_ac(reader, ac_table, data)?;
                    }
                }
            }
        } else {

            // Interleaved scan

            // Components shall not be interleaved in progressive mode, except for
            // the DC coefficients in the first scan for each component of a progressive frame.
            for i in 0..self.mcu_y
            {
                for j in 0..self.mcu_x
                {
                    // process scan n elements in order
                    for k in 0..self.num_scans
                    {
                        let n = self.z_order[k as usize];

                        let component = &mut self.components[n];

                        let huff_table = self.dc_huffman_tables.get(component.dc_huff_table)
                            .expect("No DC table for component").as_ref().unwrap();

                        for v_samp in 0..component.vertical_sample
                        {
                            for h_samp in 0..component.horizontal_sample
                            {
                                let x2 = j * component.horizontal_sample + h_samp;
                                let y2 = i * component.vertical_sample + v_samp;
                                let position = 64 * (x2 + y2 * component.width_stride / 8);
                                // data will contain the position for this coefficient in our array.
                                let data = &mut buffer[n as usize][position];

                                stream.decode_prog_dc(reader, huff_table, data, &mut component.dc_pred)?;
                            }
                        }
                    }
                }
            }
        }
        return Ok(true);
    }
}
fn get_marker(reader: &mut Cursor<Vec<u8>>, stream: &mut BitStream) -> Option<Marker>
    {
        if let Some(marker) = stream.marker
        {
            stream.marker = None;
            return Some(marker);
        }
        let mut marker = read_byte(reader);

        if marker != 0xFF
        {
            return None;
        }
        while marker == 0xFF
        {
            marker = read_byte(reader);
        }
        return Some(Marker::from_u8(marker).unwrap());
    }


#[test]
fn try_decoding()
{
    let mut v = Decoder::new();
    v.decode_file("/home/caleb/2.jpg").unwrap();
}
