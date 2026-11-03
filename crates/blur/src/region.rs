use crate::{Cancel, Edge, Error, Gaussian, Scratch, Separable, Shape, lines, resize, validate_channels};

/// An output rectangle in original-plane coordinates, with an exclusive right/bottom edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub height: usize,
}

impl Gaussian {
    /// Per-axis halo used by [`Self::blur_region`]: full finite support, or ceil(8 sigma).
    pub fn region_halo(&self) -> usize {
        match &self.passes {
            Separable::Recursive(_) => (8.0 * f64::from(self.sigma)).ceil() as usize,
            _ => self.passes.reach().unwrap_or(0),
        }
    }

    /// Conservative two-axis truncation error divided by the maximum absolute input sample.
    /// Zero for finite kernels. This analytic bound excludes floating-point roundoff and the
    /// Mirror/Wrap warmup residual; see the README for its pole-envelope derivation.
    pub fn region_error_bound(&self) -> f64 {
        match &self.passes {
            Separable::Recursive(coefficients) => coefficients.region_bound(self.region_halo()),
            _ => 0.0,
        }
    }

    /// Evaluate only `rect` and its halo, returning packed interleaved output without modifying
    /// the source. Empty rectangles are valid; rectangles outside the plane are rejected.
    /// Original-plane edges, not dirty-tile edges, define the extension. Recursive results are
    /// approximate within [`Self::region_error_bound`] times the input infinity norm, excluding
    /// roundoff. Use [`Self::apply`] when bit-identical infinite-support evaluation is required.
    pub fn blur_region(
        &self,
        pixels: &[f32],
        shape: Shape,
        rect: Rect,
        edge: Edge,
        scratch: &mut Scratch,
        cancel: Cancel<'_>,
    ) -> Result<Vec<f32>, Error> {
        validate_channels(shape.channels(), edge)?;
        if pixels.len() != shape.samples()
            || rect.x.checked_add(rect.width).is_none_or(|end| end > shape.width())
            || rect.y.checked_add(rect.height).is_none_or(|end| end > shape.height())
        {
            return Err(Error::InvalidDimensions);
        }
        cancel.check()?;
        if rect.width == 0 || rect.height == 0 {
            return Ok(Vec::new());
        }
        let halo = self.region_halo();
        let horizontal = Axis::new(rect.x, rect.width, shape.width(), halo, edge)?;
        let vertical = Axis::new(rect.y, rect.height, shape.height(), halo, edge)?;
        let local_shape = Shape::new(horizontal.length, vertical.length, shape.channels())?;
        let mut local = Vec::new();
        resize(&mut local, local_shape.samples(), 0.0)?;
        let channels = shape.channels();
        for row in 0..vertical.length {
            if row.is_multiple_of(crate::LINE_BATCH) {
                cancel.check()?;
            }
            let original_row = vertical.original(row, shape.height());
            for column in 0..horizontal.length {
                let original_column = horizontal.original(column, shape.width());
                let source = (original_row * shape.width() + original_column) * channels;
                let destination = (row * horizontal.length + column) * channels;
                local[destination..destination + channels].copy_from_slice(&pixels[source..source + channels]);
            }
        }
        self.filter_rows(&mut local, local_shape, horizontal.edge, scratch, cancel)?;
        let mut rows = Vec::new();
        rows.try_reserve_exact(vertical.length).map_err(|_| Error::Allocation)?;
        rows.extend(local.chunks_exact_mut(horizontal.length * channels));
        self.filter_columns(&mut rows, channels, vertical.edge, scratch, cancel)?;
        let mut output = Vec::new();
        let output_shape = Shape::new(rect.width, rect.height, channels)?;
        output.try_reserve_exact(output_shape.samples()).map_err(|_| Error::Allocation)?;
        for row in 0..rect.height {
            if row.is_multiple_of(crate::LINE_BATCH) {
                cancel.check()?;
            }
            let start = ((row + vertical.offset) * horizontal.length + horizontal.offset) * channels;
            output.extend_from_slice(&local[start..start + rect.width * channels]);
        }
        Ok(output)
    }
}

struct Axis {
    start: i128,
    length: usize,
    offset: usize,
    edge: Edge,
}

impl Axis {
    fn new(start: usize, length: usize, extent: usize, halo: usize, edge: Edge) -> Result<Self, Error> {
        if edge == Edge::Wrap {
            let expanded = length.checked_add(halo.checked_mul(2).ok_or(Error::SizeOverflow)?).ok_or(Error::SizeOverflow)?;
            if expanded >= extent {
                Ok(Self { start: 0, length: extent, offset: start, edge })
            } else {
                Ok(Self { start: start as i128 - halo as i128, length: expanded, offset: halo, edge: Edge::Clamp })
            }
        } else {
            let left = start.saturating_sub(halo);
            let right = start.saturating_add(length).saturating_add(halo).min(extent);
            Ok(Self { start: left as i128, length: right - left, offset: start - left, edge })
        }
    }

    fn original(&self, position: usize, extent: usize) -> usize {
        lines::mapped(self.start + position as i128, extent, Edge::Wrap).unwrap_or(0)
    }
}
