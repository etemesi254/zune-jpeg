//! Routines for progressive decoding
//!
//! This module implements the routines needed to decode progressive images.
//!
//! Progressive images require multiple scans to reconstruct back the image.
//!
//! Since most of the image is contained in DC coeff,the first scan may (let's say) encode DC coefficient.
//! of all scans
//!
//! A more sophisticated decode(not this) may create a rough image from the first scan and then progressively
//! make it better.
//!
//! This is useful for let's say slow web connections where the user can get a rough sketch of the image.
//!
//! But Gad damn, doesn't the spec make it a mess.
//!
//! Each scan contains a DHT and SOS, but each scan can have more than one component,(Why did you just
//!  make it one scan jpeg!!) and it can also be interleaved(okay someone was just trolling at this point).
//! and it's all a bloody mess of codw.
//!
//! And furthermore, it takes way too much code to process images. All in a serial manner since we are doing
//!  Huffman decoding still. And like the baseline case, we cannot break after finishing MCU's width
//! since we are still not yet done.
//!
//!
//! So here we use a different scheme. Just decode everything and then finally use threads when post processing.

use std::io::Cursor;
use std::sync::Arc;

use crate::bitstream::BitStream;
use crate::components::{ComponentID, SubSampRatios};
use crate::Decoder;
use crate::errors::DecodeErrors;
use crate::headers::{parse_huffman, parse_sos};
use crate::marker::Marker;
use crate::misc::read_byte;
use crate::worker::post_process_prog;

impl Decoder
{
    /// Decode a progressive image
    ///
    /// This routine decodes a progressive image, stopping if it finds any error.
    #[rustfmt::skip]
    pub(crate) fn decode_mcu_ycbcr_progressive(
        &mut self, reader: &mut Cursor<Vec<u8>>,
    ) -> Result<Vec<u8>, DecodeErrors>
    {
        // memory location for decoded pixels for components
        let mut block = [vec![], vec![], vec![]];

        let mut mcu_width;

        let mcu_height;

        if self.interleaved
        {
            mcu_width = self.mcu_x;

            mcu_height = self.mcu_y;
        } else {
            mcu_width = (self.info.width as usize + 7) / 8;

            mcu_height = (self.info.height as usize + 7) / 8;
        }
        mcu_width *= 64;

        for i in 0..self.input_colorspace.num_components()
        {
            let comp = &self.components[i];

            let len = mcu_width * comp.vertical_sample * comp.horizontal_sample * mcu_height;

            block[i] = vec![0; len];
        }

        let mut stream = BitStream::new_progressive(self.succ_high, self.succ_low,
                                                    self.spec_start, self.spec_end);

        // there are multiple scans in the stream, this should resolve the first scan
        self.parse_entropy_coded_data(reader, &mut stream, &mut block)?;

        // extract marker
        let mut marker = stream.marker.take().unwrap();
        // if marker is EOI, we are done, otherwise continue scanning.
        'eoi: while marker != Marker::EOI
        {
            match marker
            {
                Marker::DHT => {
                        parse_huffman(self, reader)?;
                    }
                Marker::SOS =>
                    {
                        parse_sos(reader, self)?;

                        stream.update_progressive_params(self.succ_high, self.succ_low,
                                                         self.spec_start, self.spec_end);

                        // after every SOS, marker, parse data for that scan.
                        self.parse_entropy_coded_data(reader, &mut stream, &mut block)?;
                        // extract marker, might either indicate end of image or we continue
                        // scanning(hence the continue statement to determine).
                        marker = get_marker(reader, &mut stream).unwrap();

                        stream.reset();

                        continue 'eoi;
                    }
                _ =>
                    {
                        break 'eoi;
                    }
            }
            marker = get_marker(reader, &mut stream).unwrap();
        }

        return Ok(self.finish_progressive_decoding(block, mcu_width));
    }

    #[rustfmt::skip]
    fn finish_progressive_decoding(&mut self, block: [Vec<i16>; 3], mcu_width: usize) -> Vec<u8> {
        self.set_upsampling().unwrap();

        let mut mcu_width = mcu_width;

        if self.sub_sample_ratio == SubSampRatios::H
        {
            mcu_width = mcu_width * 2;
        }
        // remove items from  top block
        let y = &block[0];
        let cb = &block[1];
        let cr = &block[2];

        let capacity = usize::from(self.info.width) * usize::from(self.info.height);

        let mut out_vector = vec![0_u8; capacity * self.output_colorspace.num_components()];

        // Chunk sizes. Each determine how many pixels go per thread.
        let y_chunk_size =
            mcu_width * self.components[0].vertical_sample * self.components[0].horizontal_sample;

        // Cb and Cr contains equal sub-sampling so don't calculate for them.
        let cb_chunk_size =
            mcu_width * self.components[1].vertical_sample * self.components[1].horizontal_sample;

        // Divide into chunks
        let y_chunk = y.chunks_exact(y_chunk_size);

        let cb_chunk = cb.chunks_exact(cb_chunk_size);

        let cr_chunk = cr.chunks_exact(cb_chunk_size);

        // Things we need for multithreading.
        let h_max = self.h_max;

        let v_max = self.v_max;

        let components = Arc::new(self.components.clone());

        let mut pool = scoped_threadpool::Pool::new(num_cpus::get_physical() as u32);

        let input = self.input_colorspace;

        let output = self.output_colorspace;

        let idct_func = self.idct_func;

        let color_convert = self.color_convert;

        let color_convert_16 = self.color_convert_16;

        let width = usize::from(self.width());

        // Divide the output into small blocks and send to threads/
        let chunks_size = width * self.output_colorspace.num_components() * 8 * h_max * v_max;

        let out_chunks = out_vector.chunks_exact_mut(chunks_size);

        // open threads.
        pool.scoped(|scope| {
            for (((y, cb), cr), out) in
            y_chunk.zip(cb_chunk).zip(cr_chunk).zip(out_chunks)
            {
                let component = components.clone();

                scope.execute(move || {

                    post_process_prog(&[y, cb, cr], &component, idct_func, color_convert_16,
                                      color_convert, input, output, out, mcu_width, width,
                    )
                });
            }
        });
        debug!("Finished decoding image");

        return out_vector;
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

    // read until we get a marker
    let len = u64::try_from(reader.get_ref().len()).unwrap();
    loop {
        let marker = read_byte(reader);

        if marker == 255
        {
            let mut r = read_byte(reader);
            // 0xFF 0XFF(some images may be like that)
            while r == 0xFF
            {
                r = read_byte(reader);
            }

            if r != 0
            {
                return Some(Marker::from_u8(r).unwrap());
            }

            if reader.position()>=len{
                // end of buffer
                return  None;
            }
        }
    }
}

#[test]
fn try_decoding()
{
    let mut v = Decoder::new();
    v.decode_file("/home/caleb/2.jpg").unwrap();
}
