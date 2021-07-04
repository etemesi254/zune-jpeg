use std::io::{Cursor};

use crate::bitstream::BitStream;
use crate::components::ComponentID;
use crate::errors::DecodeErrors;
use crate::misc::Aligned32;
use crate::worker::{dequantize_idct_component, upsample_color_convert_ycbcr};
use crate::Decoder;


impl Decoder {
    /// Decode data from MCU's
    pub(crate) fn decode_mcu_ycbcr(
        &mut self,
        reader: &mut Cursor<Vec<u8>>,
    ) -> Result<Vec<u8>, DecodeErrors> {
        let mcu_width = (self.info.width + 7) / 8;
        let mcu_height = (self.info.height + 7) / 8;

        // Create a BitStream
        let mut stream = BitStream::new();

        // We only deal with YCbCr here so we can definitely do this
        // DC table for Y(Luminance)
        let dc_table_y = self
            .dc_huffman_tables
            .get(0)
            .expect("No DC table for Y component was found")
            .as_ref()
            .expect("No DC table was initialized for Y component");
        // DC table for Cb and Cr
        let dc_table_cb_cr = self
            .dc_huffman_tables
            .get(1)
            .expect("No DC table for Y component was found")
            .as_ref()
            .expect("No DC table was initialized for Y component");
        // Ac table for Y component
        let ac_table_y = self
            .ac_huffman_tables
            .get(0)
            .expect("No DC table for Y component was found")
            .as_ref()
            .expect("No DC table was initialized for Y component");
        // AC table for Cb and Cr
        let ac_table_cb_cr = self
            .ac_huffman_tables
            .get(1)
            .expect("No DC table for Y component was found")
            .as_ref()
            .expect("No DC table was initialized for Y component");
        let capacity =
            usize::from(self.info.width) * usize::from(self.info.height) as usize;

        let component_capacity = usize::from(mcu_width+1)*64;
        // The following contains containers for unprocessed values
        // by unprocessed we mean they haven't been dequantized and inverse DCT carried on them
        let mut y_u_component: Vec<i32> = Vec::with_capacity(component_capacity);
        let mut cb_u_component: Vec<i32> = Vec::with_capacity(component_capacity);
        let mut cr_u_component: Vec<i32> = Vec::with_capacity(component_capacity);
        let mut position = 0;
        let mut global_channel = vec![0; capacity * 3];
        let (y_qt, cb_qt, cr_qt) = {
            let y = self
                .components
                .get_mut()
                .iter()
                .find(|x| x.component_id == ComponentID::Y)
                .unwrap()
                .quantization_table;
            let cb = self
                .components
                .get_mut()
                .iter()
                .find(|x| x.component_id == ComponentID::Cb)
                .unwrap()
                .quantization_table;
            let cr = self
                .components
                .get_mut()
                .iter()
                .find(|x| x.component_id == ComponentID::Cr)
                .unwrap()
                .quantization_table;
            (y, cb, cr)
        };
        for _ in 0..mcu_height {
            // Drop all values inside the unprocessed vec since they will be passed
            // This makes it cheaper to clone (less elements) and reduce memory usage
            y_u_component.clear();
            cb_u_component.clear();
            cr_u_component.clear();
            for _ in 0..mcu_width {
                for component in &mut self.components.get_mut().iter_mut() {
                    let (dc_table, ac_table, values) = match component.component_id {
                        ComponentID::Y => (dc_table_y, ac_table_y, &mut y_u_component),
                        ComponentID::Cb => (dc_table_cb_cr, ac_table_cb_cr, &mut cb_u_component),
                        ComponentID::Cr => (dc_table_cb_cr, ac_table_cb_cr, &mut cr_u_component),
                    };
                    let mut c = Aligned32([0; 64]);
                    // decode the MCU
                    stream.decode_mcu_fast(
                        reader,
                        dc_table,
                        ac_table,
                        &mut c.0,
                        &mut component.dc_pred,
                    );
                    values.extend(c.0.iter());

                }
            }
            // Carry out dequantization and idct by calling the function at idct_func
            dequantize_idct_component(&mut y_u_component, &y_qt, &mut self.idct_func);
            dequantize_idct_component(&mut cb_u_component, &cb_qt, &mut self.idct_func);
            dequantize_idct_component(&mut cr_u_component, &cr_qt, &mut self.idct_func);

            upsample_color_convert_ycbcr(
                &Aligned32(y_u_component.as_ref()),
                &Aligned32(cb_u_component.as_ref()),
                &Aligned32(cr_u_component.as_ref()),
                &mut self.color_convert_func,
                position,
                &mut Aligned32(&mut global_channel).0,
            );
            position += usize::from(mcu_width << 3)
        }

        Ok(global_channel)
    }
}
