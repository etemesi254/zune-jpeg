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
//! For JPEG decoding, I found a sweet spot of doing it per MCU width, I.e since the longest time
//! for jpeg decoding is probably bitstream decoding, we can allow it to continue on the main thread
//! as new threads are spawned to handle post processing(i.e IDCT, upsampling and color conversion).
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
use crate::components::ComponentID;
use crate::Decoder;
use crate::errors::DecodeErrors;
use crate::marker::Marker;
use crate::worker::post_process;

/// The size of a DC block for a MCU.

pub const DCT_BLOCK: usize = 64;

impl Decoder
{
    /// Check for existence of DC and AC Huffman Tables
    fn check_tables(&self) -> Result<(), DecodeErrors>
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
    /// This is the main decoder loop for the library aka the hot path.
    ///
    /// Because of this, we pull in some very crazy optimization tricks hence readability is a pinch
    /// here.
    #[allow(clippy::similar_names)]
    #[inline(never)]
    #[rustfmt::skip]
    pub(crate) fn decode_mcu_ycbcr_baseline(
        &mut self,
        reader: &mut Cursor<Vec<u8>>,
    ) -> Result<Vec<u8>, DecodeErrors>
    {
        let mut scoped_pools = scoped_threadpool::Pool::new(num_cpus::get() as u32);
        info!("Created {} worker threads",scoped_pools.thread_count());

        let (mcu_width, mcu_height);
        let mut bias = 1;

        if self.interleaved
        {
            // set upsampling functions
            self.set_upsampling()?;

            if self.h_max > self.v_max
            {
                // horizontal sub-sampling.

                // Values for horizontal samples end halfway the image and do not complete an MCU width.
                // To make it complete we multiply width by 2 and divide mcu_height by 2
                mcu_width = self.mcu_x * 2;

                mcu_height = self.mcu_y / 2;
            } else if self.h_max == self.v_max && self.h_max == 2 {
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


        let mut stream = BitStream::new();
        // Size of our output image(width*height)
        let capacity = usize::from(self.info.width + 7) * usize::from(self.info.height + 7);

        let component_capacity = mcu_width * DCT_BLOCK;
        // for those pointers storing unprocessed items, zero them out here
        for (pos, comp) in self.components.iter().enumerate()
        {
            // multiply capacity with sampling factor, it  should be 1*1 for un-sampled images

            //NOTE: We only allocate a block if we need it, so e.g for grayscale
            // we don't allocate for CB and Cr channels
            if min(self.output_colorspace.num_components() - 1, pos) == pos
            {
                let mut len = component_capacity * comp.vertical_sample * comp.horizontal_sample;
                // For 4:2:0 upsampling we need to do some tweaks, reason explained in bias
                if bias == 2 && comp.component_id == ComponentID::Y {
                    len *= 2;
                }
                self.mcu_block[pos] =
                    vec![0; len];
            }
        }


        // Create an Arc of components to prevent cloning on every MCU width
        let global_component = Arc::new(self.components.clone());

        // Storage for decoded pixels
        let mut global_channel = vec![0; capacity * self.output_colorspace.num_components()];

        // things needed for post processing that we can remove out of the loop
        let input = self.input_colorspace;

        let output = self.output_colorspace;

        let idct_func = self.idct_func;

        let color_convert = self.color_convert;

        let color_convert_16 = self.color_convert_16;

        let width = usize::from(self.width());

        let h_max = self.h_max;

        let v_max = self.v_max;

        let stride = (self.mcu_block[0].len()) >> 2;

        // check dc and AC tables
        self.check_tables()?;

        // Split output into different blocks each containing enough space for an MCU width
        let mut chunks = global_channel.chunks_exact_mut(width * output.num_components() * 8 * h_max * v_max);

        // Argument for scoped threadpools, see file docs.
        scoped_pools.scoped::<_, Result<(), DecodeErrors>>(|scope| {
            for _ in 0..mcu_height
            {
                // Bias only affects 4:2:0(chroma quartered) sub-sampled images. So let me explain
                // For  4:2:0 sub-sampling, we decode 4 rows of MCU's, the hard part is
                // determining where the Y channel is stored. Dor Y Channel it looks like this
                // |Y1| |Y2| |Cb| |Cr| |Y5| | Y6|
                // |Y3| |Y4|           |Y7|  |Y8|
                // ------------------------------
                // |Y9|  |Y11|        |Y13| |Y14|
                // |Y10| |Y12|        |Y15| |Y16|
                // The problem becomes knowing where to write the channel once decoded.
                // For vertical / horizontal sub-sampling, we use j(iterator in MCU width).
                // But j cannot be used to determine 4:2:0 sub-sampled because it will write in the wrong place
                //
                // What we want while decoding the first row, write in the first half of the vector(Y component only).
                // The second row, write in the second half of the vector.
                // Ideally this means that  our offset calculation must take this into account while remaining transparent
                // to Cb and Cr channels(those are written differently), and current whether we are in the first
                // row or second row. And that becomes quite complex, forgive me...
                for v in 0..bias {
                    // Ideally this should be one loop but I'm parallelizing per MCU width boys
                    'label: for j in 0..mcu_width
                    {
                        // iterate over components

                        'rst: for pos in 0..self.input_colorspace.num_components()
                        {
                            let component = &mut self.components[pos];
                            // Safety:The tables were confirmed to exist in self.check_tables();
                            // Reason.
                            // - These were 4 branch checks per component, for a 1080 * 1080 *3 component image
                            //   that becomes(1080*1080*3)/(16)-> 218700 branches in the hot path. And I'm not
                            //   paying that penalty
                            let dc_table = unsafe {
                                self.dc_huffman_tables.get_unchecked(component.dc_huff_table)
                                    .as_ref()
                                    .unwrap_or_else(|| std::hint::unreachable_unchecked())
                            };
                            let ac_table = unsafe {
                                self.ac_huffman_tables.get_unchecked(component.ac_huff_table)
                                    .as_ref()
                                    .unwrap_or_else(|| std::hint::unreachable_unchecked())
                            };
                            // If image is interleaved iterate over scan  components,
                            // otherwise if it-s non-interleaved, these routines iterate in
                            // trivial scanline order(Y,Cb,Cr)
                            for v_samp in 0..component.vertical_sample {
                                for h_samp in 0..component.horizontal_sample {
                                    let mut tmp = [0; DCT_BLOCK];
                                    // decode the MCU
                                    if !(stream.decode_mcu_block(reader, dc_table, ac_table, &mut tmp, &mut component.dc_pred))
                                    {
                                        // Found a marker
                                        // Read stream and see what marker is stored there
                                        let marker = stream.marker.expect("No marker found");

                                        match marker
                                        {
                                            Marker::RST(_) =>
                                                {
                                                    // reset stream
                                                    stream.reset();
                                                    // Initialize dc predictions to zero for all components
                                                    self.components.iter_mut().for_each(|x| x.dc_pred = 0);
                                                    // Start iterating again. from position.
                                                    break 'rst;
                                                }
                                            Marker::EOI =>
                                                {
                                                    info!("EOI marker found, wrapping up here ");
                                                    // Okay encountered end of Image break to IDCT and color convert.
                                                    break 'label;
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
                                    // Store only needed components (i.e for YCbCr->Grayscale don't store Cb and Cr channels)
                                    // improves speed when we do a clone(less items to clone)
                                    if min(self.output_colorspace.num_components() - 1, pos) == pos
                                    {
                                        // calculate where to start writing. This is quite complex because MCU's
                                        // in images are weird.
                                        // A good example is vertical sub-sampling(what sent me to this rabbit hole)
                                        // The run-length encoding is
                                        // |Y1| |Cb| Cr| |Y3| |Cb| |Cr| |Y5|
                                        // |Y2|          |Y4|           |Y6|
                                        //
                                        // During  decoding, we have to write |Y2| in the right place and this calculation
                                        // helps us do that.
                                        // We basically jump halfway our vector for writing |Y2| and jump to the start and an offset for
                                        // writing |Y3| and rinse and repeat.

                                        // This is the most complex offset calculation in existence
                                        let is_y = usize::from(component.component_id == ComponentID::Y);

                                        // This only affects 4:2:0 images.
                                        let y_offset = v * ((stride * is_y)
                                            + (stride * (component.vertical_sample - 1) * is_y));

                                        // offset calculator.
                                        let start = (j * 64 * component.horizontal_sample)
                                            + (h_samp * 64)
                                            + (stride * v_samp)
                                            + y_offset;
                                        self.mcu_block[pos][start..start + 64].copy_from_slice(&tmp);
                                    }
                                }
                            }
                        }
                    }
                }

                // Clone things, to make multithreading safe
                let component = global_component.clone();


                let mut block = self.mcu_block.clone();

                let next_chunk = chunks.next().unwrap();


                scope.execute(move || {
                    post_process(&mut block, &component, h_max, v_max,
                                 idct_func, color_convert_16, color_convert,
                                 input, output, next_chunk,
                                 mcu_width, width);
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
                * self.output_colorspace.num_components(),
        );
        return Ok(global_channel);
    }
}
