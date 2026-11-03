use crate::{ColorStandard, Error, Frame};
use std::ops::Range;

/// Caller-defined false-colour exposure bands, not a renderer or fixed camera palette.
/// Sorted thresholds split luma into N+1 bands; equality belongs to the upper band.
/// `u16::MAX` marks non-finite input. Apps map band indices to their own colours.
#[derive(Clone, Copy, Debug)]
pub struct FalseColour<'a> {
    thresholds: &'a [f32],
    standard: ColorStandard,
}

impl<'a> FalseColour<'a> {
    pub fn new(thresholds: &'a [f32], standard: ColorStandard) -> Result<Self, Error> {
        if thresholds.len() >= u16::MAX as usize
            || thresholds.iter().any(|value| !value.is_finite())
            || thresholds.windows(2).any(|pair| pair[0] >= pair[1])
        {
            return Err(Error::Configuration);
        }
        Ok(Self { thresholds, standard })
    }

    pub fn classify(self, rgb: [f32; 3]) -> u16 {
        if !rgb.iter().all(|value| value.is_finite()) {
            return u16::MAX;
        }
        let luma = self.standard.ycbcr(rgb)[0];
        if !luma.is_finite() {
            return u16::MAX;
        }
        self.thresholds.partition_point(|threshold| *threshold <= luma) as u16
    }

    /// Writes only the requested rows into an exactly sized output *chunk*, not a full-frame
    /// buffer. Workers can borrow disjoint chunks with `split_at_mut`; no merge is needed.
    pub fn map_rows(self, frame: Frame<'_>, rows: Range<usize>, output: &mut [u16]) -> Result<(), Error> {
        frame.validate_rows(&rows)?;
        if output.len() != (rows.end - rows.start) * frame.width() {
            return Err(Error::BufferLength);
        }
        let start = rows.start * frame.width();
        for (offset, band) in output.iter_mut().enumerate() {
            *band = self.classify(frame.pixel(start + offset));
        }
        Ok(())
    }
}
