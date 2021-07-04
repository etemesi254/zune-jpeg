use crate::errors::DecodeErrors;

/// Component Data from start of frame
#[derive(Clone)]
pub(crate) struct Components {
    pub component_id: ComponentID,
    vertical_sample: u8,
    horizontal_sample: u8,
    //Quantization table number
    pub quantization_table_number: u8,
    // Specifies quantization table to use with this component
    pub quantization_table: [i32; 64],
    // dc prediction for the component
    pub dc_pred: i32,
}
impl Components {
    /// Create a new instance from three bytes from the start of frame
    #[inline]
    pub fn from(a: [u8; 3]) -> Result<Components, DecodeErrors> {
        let id = match a[0] {
            1 => ComponentID::Y,
            2 => ComponentID::Cb,
            3 => ComponentID::Cr,
            r => {
                return Err(DecodeErrors::Format(format!(
                    "Unknown component id found,{}, expected value between 1 and 3\nNote I and Q components are not supported yet",
                    r
                )))
            }
        };

        // first 4 bits are vertical sample, we discard bottom 4 bits by a right shift.
        let vertical_sample = a[1] >> 4;
        // last 4 bits are horizontal samples, we get bottom n bits
        let horizontal_sample = a[1] & 0x0f;

        let quantization_table_number = a[2];
        debug!("\n\tComponent ID:{:?},\n\tVertical Sample:{}\n\tHorizontal Sample:{},\n\tquantization Table destinator:{}",
               id,vertical_sample,horizontal_sample,quantization_table_number);

        Ok(Components {
            component_id: id,
            vertical_sample,
            horizontal_sample,
            quantization_table_number,
            quantization_table: [0; 64],
            dc_pred: 0,
        })
    }
}
/// Component ID's
#[derive(Copy, Debug, Clone, PartialEq, Eq)]
pub enum ComponentID {
    Y,
    Cb,
    Cr,
}
