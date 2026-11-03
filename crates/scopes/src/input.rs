use crate::Error;
use std::ops::Range;

#[derive(Clone, Copy, Debug)]
enum Layout<'a> {
    Interleaved(&'a [f32]),
    Planar([&'a [f32]; 3]),
}

/// Validated, borrowed packed RGB frame. Samples are straight R'G'B', not linear light.
/// No alpha, padding, implicit decimation or colour conversion is performed.
#[derive(Clone, Copy, Debug)]
pub struct Frame<'a> {
    width: usize,
    height: usize,
    layout: Layout<'a>,
}

pub(crate) fn area(width: usize, height: usize) -> Result<usize, Error> {
    width.checked_mul(height).filter(|size| width > 0 && height > 0 && *size <= isize::MAX as usize).ok_or(Error::Dimensions)
}

impl<'a> Frame<'a> {
    /// Exactly width × height × 3 floats in RGBRGB order.
    pub fn interleaved(width: usize, height: usize, pixels: &'a [f32]) -> Result<Self, Error> {
        let length = area(width, height)?.checked_mul(3).ok_or(Error::Dimensions)?;
        if pixels.len() != length {
            return Err(Error::BufferLength);
        }
        Ok(Self { width, height, layout: Layout::Interleaved(pixels) })
    }

    /// Three exactly width × height planes in R, G, B order.
    pub fn planar(width: usize, height: usize, planes: [&'a [f32]; 3]) -> Result<Self, Error> {
        let length = area(width, height)?;
        if planes.iter().any(|plane| plane.len() != length) {
            return Err(Error::BufferLength);
        }
        Ok(Self { width, height, layout: Layout::Planar(planes) })
    }

    pub fn width(self) -> usize {
        self.width
    }

    pub fn height(self) -> usize {
        self.height
    }

    pub(crate) fn validate_rows(self, rows: &Range<usize>) -> Result<(), Error> {
        if rows.start > rows.end || rows.end > self.height {
            return Err(Error::Rows);
        }
        Ok(())
    }

    pub(crate) fn pixel(self, index: usize) -> [f32; 3] {
        match self.layout {
            Layout::Interleaved(pixels) => [pixels[index * 3], pixels[index * 3 + 1], pixels[index * 3 + 2]],
            Layout::Planar(planes) => [planes[0][index], planes[1][index], planes[2][index]],
        }
    }
}
