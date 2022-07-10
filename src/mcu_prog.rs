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
//!  make it one scan jpeg!!) and it can also be interleaved(okay someone was just trolling at this point)
//! but on interleaving it only happens in DC coefficients. AC coefficients cannot be interleaved.
//! and it's all a bloody mess of code.
//!
//! And furthermore, it takes way too much code to process images. All in a serial manner since we are doing
//!  Huffman decoding still. And unlike the baseline case, we cannot break after finishing MCU's width
//! since we are still not yet done.
//!
//!
//! So here we use a different scheme. Just decode everything and then finally use threads when post processing.

use std::io::Cursor;
use std::sync::Arc;

use crate::bitstream::BitStream;
use crate::components::{ComponentID, SubSampRatios};
use crate::errors::DecodeErrors;
use crate::headers::{parse_huffman, parse_sos};
use crate::marker::Marker;
use crate::misc::read_byte;
use crate::worker::post_process_prog;
use crate::{ColorSpace, Decoder};

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
        self.check_component_dimensions()?;
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
        let mut marker = stream.marker.take().ok_or_else(|| DecodeErrors::Format(format!("Marker missing where expected")))?;
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
                        marker = get_marker(reader, &mut stream).ok_or_else(|| DecodeErrors::Format(format!("Marker missing where expected")))?;

                        stream.reset();

                        continue 'eoi;
                    }
                _ =>
                    {
                        break 'eoi;
                    }
            }
            marker = get_marker(reader, &mut stream).ok_or_else(|| DecodeErrors::Format(format!("Marker missing where expected")))?;
        }

        self.finish_progressive_decoding(&block, mcu_width)
    }

    #[rustfmt::skip]
    fn finish_progressive_decoding(&mut self, block: &[Vec<i16>; 3], mcu_width: usize) -> Result<Vec<u8> ,DecodeErrors>{
        self.set_upsampling()?;

        let mut mcu_width = mcu_width;
        let mut bias = 1;

        if self.sub_sample_ratio == SubSampRatios::H
        {
            mcu_width *= 2;
        }
        if self.sub_sample_ratio == SubSampRatios::HV{
            bias=2;
        }


        if self.input_colorspace == ColorSpace::GRAYSCALE   && self.interleaved{
            /*
            Apparently, grayscale images which can be down sampled exists, which is weird in the sense
            that it has one component Y, which is not usually down sampled.

            This means some calculations will be wrong, so for that we explicitly reset params
            for such occurrences, warn and reset the image info to appear as if it were
            a non-sampled image to ensure decoding works

            NOTE: not tested on progressive images as I couldn't find such an image.
            */
            warn!("Grayscale image with down-sampled component, resetting component details");
            self.h_max = 1;
            self.v_max = 1;
            self.sub_sample_ratio = SubSampRatios::None;
            self.components[0].vertical_sample = 1;
            self.components[0].width_stride = mcu_width* 8;
            self.components[0].horizontal_sample = mcu_width;
            bias=1;
        }
        // remove items from  top block
        let y = &block[0];
        let cb = &block[1];
        let cr = &block[2];

        let capacity = usize::from(self.info.width + 8) * usize::from(self.info.height + 8);

        let mut out_vector = vec![0_u8; capacity * self.output_colorspace.num_components()];


        // Things we need for multithreading.
        let h_max = self.h_max;

        let v_max = self.v_max;

        let components = Arc::new(self.components.clone());

        let mut pool = scoped_threadpool::Pool::new(self.num_threads.unwrap_or( num_cpus::get()) as u32);

        let input = self.input_colorspace;

        let output = self.output_colorspace;

        let idct_func = self.idct_func;


        let color_convert_16 = self.color_convert_16;

        let width = usize::from(self.width());

        // Divide the output into small blocks and send to threads/
        let chunks_size = width * self.output_colorspace.num_components() * 8 * h_max * v_max ;

        let out_chunks = out_vector.chunks_exact_mut(chunks_size);

        // Chunk sizes. Each determine how many pixels go per thread.
        let y_chunk_size =
            mcu_width * self.components[0].vertical_sample * self.components[0].horizontal_sample * bias;
        // Cb and Cr contains equal sub-sampling so don't calculate for them.

        // Divide into chunks
        let y_chunk = y.chunks_exact(y_chunk_size);

        if self.input_colorspace.num_components() == 3 {

            let cb_chunk_size =
                mcu_width * self.components[1].vertical_sample * self.components[1].horizontal_sample * bias;

            let cb_chunk = cb.chunks_exact(cb_chunk_size);

            let cr_chunk = cr.chunks_exact(cb_chunk_size);

            // open threads.
            pool.scoped(|scope| {
                for (((y, cb), cr), out) in
                y_chunk.zip(cb_chunk).zip(cr_chunk).zip(out_chunks)
                {
                    let component = components.clone();

                    scope.execute(move || {
                        post_process_prog(&[y, cb, cr], &component, idct_func, color_convert_16,
                                          input, output, out, width,
                        );
                    });
                }
            });
        } else {
            // one component
            pool.scoped(|scope| {
                for (y, out) in y_chunk.zip(out_chunks)
                {
                    let component = components.clone();
                    scope.execute(move || {
                        post_process_prog(&[y, &[], &[]], &component, idct_func, color_convert_16,
                                          input, output, out, width,
                        );
                    });
                }
            });
        }
        debug!("Finished decoding image");

        out_vector.truncate(
            usize::from(self.width())
                * usize::from(self.height())
                * self.output_colorspace.num_components(),
        );

        return Ok(out_vector);
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
            // Safety checks
            if self.spec_end != 0 && self.spec_start == 0
            {
                return Err(DecodeErrors::HuffmanDecode(
                    "Can't merge DC and AC corrupt jpeg".to_string(),
                ));
            }
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
            let mut i = 0;

            let mut j = 0;

            while i < mcu_height
            {
                while j < mcu_width
                {
                    let start = 64 * (j + i * (self.components[k].width_stride / 8));

                    let data: &mut [i16; 64] = buffer.get_mut(k).unwrap().get_mut(start..start + 64)
                        .unwrap().try_into().unwrap();

                    if self.spec_start == 0
                    {
                        let pos = self.components[k].dc_huff_table;

                        let dc_table = self.dc_huffman_tables.get(pos).unwrap().as_ref().unwrap();

                        let dc_pred = &mut self.components[k].dc_pred;
                        if self.succ_high == 0
                        {
                            // first scan for this mcu
                            stream.decode_prog_dc_first(reader, dc_table, &mut data[0], dc_pred)?;
                        } else {
                            // refining scans for this MCU
                            stream.decode_prog_dc_refine(reader, &mut data[0])?;
                        }
                    } else {
                        let pos = self.components[k].ac_huff_table;

                        let ac_table = self.ac_huffman_tables.get(pos)
                            .ok_or_else(|| DecodeErrors::Format(format!("No huffman table for component:{}", pos)))?
                            .as_ref()
                            .ok_or_else(|| DecodeErrors::Format(format!("Huffman table at index  {} not initialized",pos)))?;

                        if self.succ_high == 0
                        {
                            // first scan for this MCU
                            if stream.eob_run > 0
                            {
                                // EOB runs indicate the whole block is empty, but unlike for baseline
                                // EOB in progressive tell us the number of proceeding blocks currently zero.

                                // other decoders use a check in decode_mcu_first decrement and return if it's an
                                // eob run(since the array is expected to contain zeroes). but that's a function call overhead(if not inlined) and a branch check
                                // we do it a bit differently
                                // we can use divisors to determine how many MCU's to skip
                                // which is more faster than a decrement and return since EOB runs can be
                                // as big as 10,000

                                i += (j + stream.eob_run as usize - 1) / mcu_width;

                                j = (j + stream.eob_run as usize - 1) % mcu_width;

                                stream.eob_run = 0;
                            } else {
                                stream.decode_mcu_ac_first(reader, ac_table, data)?;
                            }
                        } else {
                            // refinement scan
                            stream.decode_mcu_ac_refine(reader, ac_table, data)?;
                        }
                    }
                    j += 1;

                    self.todo -= 1;

                    if self.todo == 0
                    {
                        self.handle_rst(stream)?;
                    }
                }
                j = 0;
                i += 1;
            }
        } else {
            if self.spec_end != 0
            {
                return Err(DecodeErrors::HuffmanDecode(
                    "Can't merge dc and AC corrupt jpeg".to_string(),
                ));
            }
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
                            .ok_or_else(|| DecodeErrors::Format(format!("No huffman table for component:{}", component.dc_huff_table)))?
                            .as_ref()
                            .ok_or_else(|| DecodeErrors::Format(format!("Huffman table at index  {} not initialized",component.dc_huff_table)))?
                            ;

                        for v_samp in 0..component.vertical_sample
                        {
                            for h_samp in 0..component.horizontal_sample
                            {
                                let x2 = j * component.horizontal_sample + h_samp;

                                let y2 = i * component.vertical_sample + v_samp;

                                let position = 64 * (x2 + y2 * component.width_stride / 8);

                                // data will contain the position for this coefficient in our array.
                                let data = &mut buffer[n as usize][position];

                                if self.succ_high == 0
                                {
                                    stream.decode_prog_dc_first(reader, huff_table, data, &mut component.dc_pred)?;
                                } else {
                                    stream.decode_prog_dc_refine(reader, data)?;
                                }
                            }
                        }
                        // We want wrapping subtraction here because it means
                        // we get a higher number in the case this underflows
                        self.todo = self.todo.wrapping_sub(1);
                        // after every scan that's a mcu, count down restart markers.
                        if self.todo == 0 {
                            self.handle_rst(stream)?;
                        }
                    }
                }
            }
        }
        return Ok(true);
    }
}

///Get a marker from the bit-stream.
///
/// This reads until it gets a marker or end of file is encountered
fn get_marker(reader: &mut Cursor<Vec<u8>>, stream: &mut BitStream) -> Option<Marker>
{
    if let Some(marker) = stream.marker
    {
        stream.marker = None;
        return Some(marker);
    }

    // read until we get a marker
    let len = u64::try_from(reader.get_ref().len()).unwrap();
    loop
    {
        let marker = read_byte(reader).ok()?;

        if marker == 255
        {
            let mut r = read_byte(reader).ok()?;
            // 0xFF 0XFF(some images may be like that)
            while r == 0xFF
            {
                r = read_byte(reader).ok()?;
            }

            if r != 0
            {
                return Some(
                    Marker::from_u8(r)
                        .ok_or_else(|| DecodeErrors::Format(format!("Unknown marker 0xFF{:X}", r)))
                        .ok()?,
                );
            }

            if reader.position() >= len
            {
                // end of buffer
                return None;
            }
        }
    }
}

