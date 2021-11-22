use std::io::{BufRead, Cursor};

use crate::bitstream::BitStream;
use crate::errors::DecodeErrors;
use crate::headers::{parse_huffman, parse_sos};
use crate::marker::Marker;
use crate::misc::read_byte;
use crate::Decoder;

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
        let output_buffer_size = usize::from(self.width() + 7) * usize::from(self.height() + 7);
        for i in 0..self.input_colorspace.num_components()
        {
            self.mcu_block[i] = vec![0; output_buffer_size];
        }
        let mut stream = BitStream::new_progressive(self.ah, self.al, self.ss, self.se);

        // there are multiple scans in the stream, this should resolve the first scan
        // if it returns an EOI marker,it means we are already done,
        // but if it returns any other marker, we are supposed to parse it
        self.parse_entropy_coded_data(reader, &mut stream)?;

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

                    stream.update_progressive_params(self.ah, self.al, self.ss, self.se);

                    self.parse_entropy_coded_data(reader, &mut stream)?;
                    // extract marker, might either indicate end of image or we continue
                    // scanning(hence the continue statement to determine).

                    let tmp_marker = self.get_marker(reader, &stream);
                    if tmp_marker.is_none()
                    {
                        let length = reader.get_ref().len();
                        let  mut t = reader.position() as usize;
                        while t<length
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
                            t+=1;
                        }
                    }
                    else
                    {
                        marker = tmp_marker.unwrap();
                    }
                    stream.reset();
                    continue 'eoi;
                }
                _ =>
                {
                    break 'eoi;
                }
            }
            marker = self.get_marker(reader, &stream).unwrap();
        }

        return Ok(Vec::new());
    }

    #[rustfmt::skip]
    fn parse_entropy_coded_data(
        &mut self, reader: &mut Cursor<Vec<u8>>, stream: &mut BitStream,
    ) -> Result<bool, DecodeErrors>
    {
        stream.reset();
        self.components.iter_mut().for_each(|x| x.dc_pred = 0);
        if self.ns != 1
        {
            // Interleaved scan

            // Components shall not be interleaved in progressive mode, except for
            // the DC coefficients in the first scan for each component of a progressive frame.
            for i in 0..self.mcu_y
            {
                for j in 0..self.mcu_x
                {
                    // process scan n elements in order
                    for k in 0..self.ns
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
                                let data = &mut self.mcu_block[n as usize][position];

                                stream.decode_prog_dc(reader, huff_table, data,
                                                          &mut component.dc_pred)?;


                            }
                        }
                    }
                }
            }
        } else {
            // non interleaved data, process one block at a time in trivial scanline order
            let mcu_width = ((self.info.width + 7) / 8) as usize;

            let mcu_height = ((self.info.height + 7) / 8) as usize;
            let k = self.z_order[0];
            for i in 0..mcu_height
            {
                for j in 0..mcu_width
                {
                    let start = 64 * (j + i * self.components[k].width_stride / 8);

                    let data: &mut [i16; 64] = &mut self.mcu_block[k][start..start + 64].try_into().unwrap();

                    if self.ss == 0
                    {
                        let pos = self.components[k].dc_huff_table;

                        let dc_table = self.dc_huffman_tables
                            .get(pos).unwrap().as_ref().unwrap();

                        let dc_pred = &mut self.components[k].dc_pred;

                        stream.decode_prog_dc(reader, dc_table, &mut data[0], dc_pred)?;


                    } else {
                        let pos = self.components[k].ac_huff_table;
                        let ac_table = self.ac_huffman_tables.get(pos)
                            .unwrap().as_ref().unwrap();

                        if !stream.decode_prog_ac(reader, ac_table, data)?
                        {
                            return Ok(false);
                        }
                    }
                }
            }
        }
        return Ok(true);
    }
    fn get_marker(&self, reader: &mut Cursor<Vec<u8>>, stream: &BitStream) -> Option<Marker>
    {
        if let Some(marker) = stream.marker
        {
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
}

#[test]
fn try_decoding()
{
    let mut v = Decoder::new();
    v.decode_file("/home/caleb/2.jpg").unwrap();
}
