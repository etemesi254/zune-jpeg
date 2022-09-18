//! This module exports a single struct to store information about
//! JPEG image components
//!
//! The data is extracted from a SOF header.

use crate::decoder::MAX_COMPONENTS;
use crate::errors::DecodeErrors;
use crate::misc::Aligned32;
use crate::upsampler::upsample_no_op;

/// Represents an up-sampler function, this function will be called to upsample
/// a down-sampled image

pub type UpSampler = fn(&[i16], usize) -> Vec<i16>;

/// Component Data from start of frame
#[derive(Clone)]
pub(crate) struct Components
{
    /// The type of component that has the metadata below, can be Y,Cb or Cr
    pub component_id:              ComponentID,
    /// Sub-sampling ratio of this component in the x-plane
    pub vertical_sample:           usize,
    /// Sub-sampling ratio of this component in the y-plane
    pub horizontal_sample:         usize,
    /// DC huffman table position
    pub dc_huff_table:             usize,
    /// AC huffman table position for this element.
    pub ac_huff_table:             usize,
    /// Quantization table number
    pub quantization_table_number: u8,
    /// Specifies quantization table to use with this component
    pub quantization_table:        Aligned32<[i32; 64]>,
    /// dc prediction for the component
    pub dc_pred:                   i32,
    /// An up-sampling function, can be basic or SSE, depending
    /// on the platform
    pub up_sampler:                UpSampler,
    /// How pixels do we need to go to get to the next line?
    pub width_stride:              usize,
    /// Component ID for progressive
    pub(crate) id:                 u8,
}

impl Components
{
    /// Create a new instance from three bytes from the start of frame
    #[inline]
    pub fn from(a: [u8; 3]) -> Result<Components, DecodeErrors>
    {
        let id = match a[0]
        {
            1 => ComponentID::Y,
            2 => ComponentID::Cb,
            3 => ComponentID::Cr,
            r =>
            {
                return Err(DecodeErrors::Format(format!(
                        "Unknown component id found,{}, expected value between 1 and 3\nNote I and Q components are not supported yet",
                        r
                    )));
            }
        };

        let horizontal_sample = (a[1] >> 4) as usize;
        let vertical_sample = (a[1] & 0x0f) as usize;
        let quantization_table_number = a[2];
        // confirm quantization number is between 0 and MAX_COMPONENTS
        if usize::from(quantization_table_number) >= MAX_COMPONENTS
        {
            return Err(DecodeErrors::Format(format!(
                "Too large quantization number :{}, expected value between 0 and {}",
                quantization_table_number, MAX_COMPONENTS
            )));
        }
        // check that upsampling ratios are powers of two
        // if these fail, it's probably a corrupt image.
        if !horizontal_sample.is_power_of_two()
        {
            return Err(DecodeErrors::Format(format!(
                "Horizontal sample is not a power of two({}) cannot decode",
                horizontal_sample
            )));
        }

        if !vertical_sample.is_power_of_two()
        {
            return Err(DecodeErrors::Format(format!(
                "Vertical sub-sample is not power of two({}) cannot decode",
                vertical_sample
            )));
        }

        info!(
            "Component ID:{:?}\tHS:{} VS:{} QT:{}",
            id, horizontal_sample, vertical_sample, quantization_table_number
        );

        Ok(Components {
            component_id: id,
            vertical_sample,
            horizontal_sample,
            quantization_table_number,
            // These two will be set with sof marker
            dc_huff_table: 0,
            ac_huff_table: 0,
            quantization_table: Aligned32([0; 64]),
            dc_pred: 0,
            up_sampler: upsample_no_op,
            // set later
            width_stride: horizontal_sample,
            id: a[0],
        })
    }
}

/// Component ID's
#[derive(Copy, Debug, Clone, PartialEq, Eq)]
pub enum ComponentID
{
    /// Luminance channel
    Y,
    /// Blue chrominance
    Cb,
    /// Red chrominance
    Cr,
}

#[derive(Copy, Debug, Clone, PartialEq, Eq)]
pub enum SubSampRatios
{
    HV,
    V,
    H,
    None,
}
