//! Implements routines to decode a MCU
//!
//! # Side notes
//! Yes, I pull in some dubious tricks, like really dubious here, they're not hard to come up
//! but I know they're hard to understand(e.g how I don't allocate space for Cb and Cr
//! channels if output colorspace is grayscale) but bear with me, it's the search for fast software
//! that got me here.
//!
//! # Multithreading
//!
//!This isn't exposed so I can dump all the info here
//!
//! To make multithreading work, we want to break dependency chains but in cool ways.
//! i.e we want to find out where we can forward one section as another one does something.
//!
//! # The algorithm
//!  Simply do it per MCU width taking into account sub-sampling ratios
//!
//! 1. Decode an MCU width taking into account how many image channels we have(either Y only or Y,Cb and Cr)
//!
//! 2. After successfully decoding, copy pixels decoded and spawn a thread to handle post processing(IDCT,
//! upsampling and color conversion)
//!
//! 3. After successfully decoding all pixels, join threads.
//!
//! 4. Call it a day,
//!
//!But as easy as this sounds in theory, in practice, it sucks...
//!
//! We essentially have to consider that down-sampled images have weird MCU arrangement and for such cases
//! ! choose the path of decoding 2 whole MCU heights for horizontal/vertical upsampling and
//! 4 whole MCU heights for horizontal and vertical upsampling, which when expressed in code doesn't look nice.
//!
//! There is also the overhead of synchronization which makes some things annoying.
//!
//! Also there is the overhead of `cloning` and allocating intermediate memory to ensure multithreading is safe.
//! This may make this library almost 3X slower if someone chooses to disable `threadpool` (please don't) feature because
//! we are optimized for the multithreading path.
//!
//! # Scoped ThreadPools
//! Things you don't want to do in the fast path. **Lock da mutex**
//! Things you don't want to have in your code. **Mutex**
//!
//! Multithreading is not everyone's cake because synchronization is like battling with the devil
//! The default way is a mutex for when threads may write to the same memory location. But in our case we
//! don't write to the same, location, so why pay for something not used.
//!
//! In C/C++ land we can just pass mutable chunks to different threads but in Rust don't you know about
//! the borrow checker?...
//!
//! To send different mutable chunks  to threads, we use scoped threads which guarantee that the thread
//! won't outlive the data and finally let it compile.
//! This allows us to not use locks during decoding avoiding that overhead. and allowing more cleaner
//! faster code in post processing..

use std::cmp::min;
use std::io::Cursor;
use std::sync::Arc;

use crate::bitstream::BitStream;
use crate::components::{ComponentID, SubSampRatios};
use crate::errors::DecodeErrors;
use crate::marker::Marker;
use crate::worker::post_process;
use crate::{ColorSpace, Decoder};

/// The size of a DC block for a MCU.

pub const DCT_BLOCK: usize = 64;

impl Decoder
{
    /// Check for existence of DC and AC Huffman Tables
    pub(crate) fn check_tables(&self) -> Result<(), DecodeErrors>
    {
        // check that dc and AC tables exist outside the hot path
        for i in 0..self.input_colorspace.num_components()
        {
            let _ = &self
                .dc_huffman_tables
                .get(self.components[i].dc_huff_table)
                .as_ref()
                .ok_or_else(|| {
                    DecodeErrors::HuffmanDecode(format!(
                        "No Huffman DC table for component {:?} ",
                        self.components[i].component_id
                    ))
                })?
                .as_ref()
                .ok_or_else(|| {
                    DecodeErrors::HuffmanDecode(format!(
                        "No DC table for component {:?}",
                        self.components[i].component_id
                    ))
                })?;

            let _ = &self
                .ac_huffman_tables
                .get(self.components[i].ac_huff_table)
                .as_ref()
                .ok_or_else(|| {
                    DecodeErrors::HuffmanDecode(format!(
                        "No Huffman AC table for component {:?} ",
                        self.components[i].component_id
                    ))
                })?
                .as_ref()
                .ok_or_else(|| {
                    DecodeErrors::HuffmanDecode(format!(
                        "No AC table for component {:?}",
                        self.components[i].component_id
                    ))
                })?;
        }
        Ok(())
    }

    /// Decode MCUs and carry out post processing.
    ///
    /// This is the main decoder loop for the library, the hot path.
    ///
    /// Because of this, we pull in some very crazy optimization tricks hence readability is a pinch
    /// here.
    #[allow(clippy::similar_names,clippy::too_many_lines,clippy::cast_possible_truncation)]
    #[inline(never)]
    #[rustfmt::skip]
    pub(crate) fn decode_mcu_ycbcr_baseline(
        &mut self, reader: &mut Cursor<Vec<u8>>,
    ) -> Result<Vec<u8>, DecodeErrors>
    {
        self.check_component_dimensions()?;
        // check dc and AC tables
        self.check_tables()?;

        let  mut scoped_pools = scoped_threadpool::Pool::new(
            self.options.get_threads());
        info!("Created {} worker threads", scoped_pools.thread_count());

        let (mut mcu_width, mut mcu_height);
        let mut bias = 1;

        if self.interleaved
        {
            // set upsampling functions
            self.set_upsampling()?;

            if self.sub_sample_ratio == SubSampRatios::H
            {
                // horizontal sub-sampling.

                // Values for horizontal samples end halfway the image and do not complete an MCU width.
                // To make it complete we multiply width by 2 and divide mcu_height by 2
                mcu_width = self.mcu_x * 2;
                mcu_height = self.mcu_y / 2;
            } else if self.sub_sample_ratio == SubSampRatios::HV
            {
                mcu_width = self.mcu_x;
                mcu_height = self.mcu_y / 2;
                bias = 2;
            } else {
                mcu_width = self.mcu_x;
                mcu_height = self.mcu_y;
            }
        } else {
            // For non-interleaved images( (1*1) subsampling)
            // number of MCU's are the widths (+7 to account for paddings) divided bu 8.
            mcu_width = ((self.info.width + 7) / 8) as usize;
            mcu_height = ((self.info.height + 7) / 8) as usize;
        }

        if self.input_colorspace == ColorSpace::GRAYSCALE && self.interleaved {
            /*
            Apparently, grayscale images which can be down sampled exists, which is weird in the sense
            that it has one component Y, which is not usually down sampled.

            This means some calculations will be wrong, so for that we explicitly reset params
            for such occurrences, warn and reset the image info to appear as if it were
            a non-sampled image to ensure decoding works
            */
            if self.options.get_strict_mode(){
                return Err(DecodeErrors::FormatStatic("[strict-mode]: Grayscale image with down-sampled component."))
            }

            warn!("Grayscale image with down-sampled component, resetting component details");

            mcu_width = ((self.info.width + 7) / 8) as usize;
            self.h_max = 1;
            self.options = self.options.set_out_colorspace(ColorSpace::GRAYSCALE);
            self.v_max = 1;
            self.sub_sample_ratio = SubSampRatios::None;
            self.components[0].vertical_sample = 1;
            self.components[0].width_stride = mcu_width * 8;
            self.components[0].horizontal_sample = 1;
            mcu_height = ((self.info.height + 7) / 8) as usize;
            bias = 1;
        }
        // Size of our output image(width*height)
        let capacity = usize::from(self.info.width + 8) * usize::from(self.info.height + 8);
        let component_capacity = mcu_width * DCT_BLOCK;
        // Create an Arc of components to prevent cloning on every MCU width
        let global_component = Arc::new(self.components.clone());
        let is_hv = self.sub_sample_ratio == SubSampRatios::HV;
        // There are some images where we need to overallocate  especially for small buffers,
        // because the chunking calculation will do it wrongly,
        // this only applies to  small down-sampled images
        // See https://github.com/etemesi254/zune-jpeg/issues/11
        let extra_space = usize::from(self.interleaved) * 128 * usize::from(self.height()) * self.options.get_out_colorspace().num_components();
        // things needed for post processing that we can remove out of the loop
        let input = self.input_colorspace;
        let output = self.options.get_out_colorspace();
        let idct_func = self.idct_func;
        let color_convert_16 = self.color_convert_16;
        let width = usize::from(self.width());
        let h_max = self.h_max;
        let v_max = self.v_max;
        // Halfway width size, used for vertical sub-sampling to write |Y2| in the right position.
        let width_stride = (component_capacity * self.components[0].vertical_sample * self.components[0].horizontal_sample * bias) >> 1;
        let hv_width_stride = width_stride >> 1;

        let mut stream = BitStream::new();
        // Storage for decoded pixels
        let mut global_channel = vec![0; (capacity * self.options.get_out_colorspace().num_components()) + extra_space];

        // Split output into different blocks each containing enough space for an MCU width
        let mut chunks =
            global_channel.chunks_exact_mut(width * output.num_components() * 8 * h_max * v_max);
        let mut tmp = [0; DCT_BLOCK];

        // Argument for scoped threadpools, see file docs.
        scoped_pools.scoped::<_, Result<(), DecodeErrors>>(|scope| {
            for _ in 0..mcu_height
            {
                // faster to memset than a later memcpy

                // We allocate on every mcu_height since this is sent to a separate
                // thread (that's how we're multi-threaded and thread safe).

                let mut temporary = [vec![], vec![], vec![]];

                for (pos, comp) in self.components.iter().enumerate()
                {
                    // multiply capacity with sampling factor, it  should be 1*1 for un-sampled images
                    // Allocate only needed components.
                    if min(self.options.get_out_colorspace().num_components() - 1, pos) == pos
                    {
                        let len = component_capacity * comp.vertical_sample * comp.horizontal_sample * bias;

                        temporary[pos] = vec![0; len];
                    }
                }
                // Bias only affects 4:2:0(chroma quartered) sub-sampled images.
                // since we want to fetch two MCU rows before we send it to post process
                for v in 0..bias
                {
                    for j in 0..mcu_width
                    {
                        // iterate over components

                        for pos in 0..self.input_colorspace.num_components()
                        {
                            let component = &mut self.components[pos];
                            let dc_table = self.dc_huffman_tables[component.dc_huff_table & 3]
                                .as_ref()
                                .ok_or_else(|| {
                                    DecodeErrors::HuffmanDecode(format!(
                                        "No DC table for component {:?}",
                                        component.component_id
                                    ))
                                })?;
                            let ac_table = self.ac_huffman_tables[component.ac_huff_table & 3]
                                .as_ref()
                                .ok_or_else(|| {
                                    DecodeErrors::HuffmanDecode(format!(
                                        "No AC table for component {:?}",
                                        component.component_id
                                    ))
                                })?;

                            // If image is interleaved iterate over scan  components,
                            // otherwise if it-s non-interleaved, these routines iterate in
                            // trivial scanline order(Y,Cb,Cr)
                            for v_samp in 0..component.vertical_sample
                            {
                                for h_samp in 0..component.horizontal_sample
                                {
                                    // only decode needed components
                                    if min(self.options.get_out_colorspace().num_components() - 1, pos) == pos
                                    {
                                        // The spec  https://www.w3.org/Graphics/JPEG/itu-t81.pdf page 26

                                        // Get position to write
                                        // This is complex, don't even try to understand it. ~author
                                        let is_y =
                                            usize::from(component.component_id == ComponentID::Y);
                                        // This only affects 4:2:0 images.
                                        let y_offset = is_y
                                            * v
                                            * (hv_width_stride
                                            + (hv_width_stride * (component.vertical_sample - 1)));
                                        let another_stride =
                                            (width_stride * v_samp * usize::from(!is_hv))
                                                + hv_width_stride * v_samp * usize::from(is_hv);
                                        let yet_another_stride = usize::from(is_hv)
                                            * (width_stride >> 2)
                                            * v
                                            * usize::from(component.component_id != ComponentID::Y);
                                        // offset calculator.
                                        let start = (j * 64 * component.horizontal_sample)
                                            + (h_samp * 64)
                                            + another_stride
                                            + y_offset
                                            + yet_another_stride;
                                        // It will always be zero since it's initialized per MCU height.
                                        let tmp: &mut [i16; 64] = temporary.get_mut(pos).unwrap().get_mut(start..start + 64).unwrap().try_into().unwrap();

                                        stream.decode_mcu_block(reader, dc_table, ac_table, tmp, &mut component.dc_pred)?;
                                    } else {
                                        // component not needed, decode and discard bits
                                        stream.decode_mcu_block(reader, dc_table, ac_table, &mut tmp, &mut component.dc_pred)?;
                                    }
                                }
                            }
                            self.todo = self.todo.wrapping_sub(1);
                            // after every interleaved MCU that's a mcu, count down restart markers.
                            if self.todo == 0
                            {
                                self.handle_rst(&mut stream)?;
                            }

                            // In some corrupt images, it may occur that header markers occur in the stream.
                            // The spec EXPLICITLY FORBIDS this, specifically, in
                            // routine F.2.2.5  it says
                            // `The only valid marker which may occur within the Huffman coded data is the RSTm marker.`
                            //
                            // But libjpeg-turbo allows it because of some weird reason. so I'll also
                            // allow it because of some weird reason.
                            if let Some(m) = stream.marker
                            {
                                if m == Marker::EOI
                                {
                                    break;
                                }

                                if let Marker::RST(_) = m { continue }

                                error!("Marker `{:?}` Found within Huffman Stream, possibly corrupt jpeg",m);
                                self.parse_marker_inner(m, reader)?;
                            }
                        }
                    }
                }
                // Clone things, to make multithreading safe
                let component = global_component.clone();
                let next_chunk = chunks.next().unwrap();

                scope.execute(move || {

                    let mut coeff :[&[i16];3]=[&[];3];

                    temporary.iter().enumerate().for_each(|(pos,x)|{
                        coeff[pos] = x;
                    });

                    post_process(&coeff, &component,
                                 idct_func, color_convert_16,
                                 input, output, next_chunk,
                                 width);
                });
            }
            //everything is okay
            Ok(())
        })?;
        info!("Finished decoding image");
        // remove excess allocation for images.
        global_channel.truncate(
            usize::from(self.width())
                * usize::from(self.height())
                * self.options.get_out_colorspace().num_components(),
        );
        return Ok(global_channel);
    }
    // handle RST markers.
    // No-op if not using restarts
    // this routine is shared with mcu_prog
    #[cold]
    pub(crate) fn handle_rst(&mut self, stream: &mut BitStream) -> Result<(), DecodeErrors>
    {
        self.todo = self.restart_interval;

        if let Some(marker) = stream.marker
        {
            // Found a marker
            // Read stream and see what marker is stored there
            match marker
            {
                Marker::RST(_) =>
                {
                    // reset stream
                    stream.reset();
                    // Initialize dc predictions to zero for all components
                    self.components.iter_mut().for_each(|x| x.dc_pred = 0);
                    // Start iterating again. from position.
                }
                Marker::EOI =>
                {
                    // silent pass
                }
                _ =>
                {
                    return Err(DecodeErrors::MCUError(format!(
                        "Marker {:?} found in bitstream, possibly corrupt jpeg",
                        marker
                    )));
                }
            }
        }
        Ok(())
    }
}
