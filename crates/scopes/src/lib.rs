#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![forbid(unsafe_code)]
#![doc = include_str!("../README.md")]
//! CPU video scopes over straight, display-encoded R'G'B' samples.
//!
//! Allocate an [`Accumulator`] once, [`Accumulator::clear`] each frame, and call
//! [`Accumulator::accumulate_rows`]. For parallel work give each worker its own
//! reusable accumulator, then [`Accumulator::merge`] into a cleared destination.
//! Scheduling, alpha compositing, transfer functions, decimation and rendering
//! belong to the caller. No operation after construction allocates.
//!
//! Waveforms and parades have separate semantic trace planes; a parade is drawn
//! side by side by the app, not baked into a coloured canvas. Histograms have
//! R/G/B/Y planes; vectorscopes have one Cb/Cr plane. Counts saturate at `u64::MAX`.

mod false_colour;
mod input;

pub use false_colour::FalseColour;
pub use input::Frame;

use std::ops::Range;

/// Invalid inputs and fallible storage allocation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// Zero, overflowing or unaddressable dimensions.
    Dimensions,
    /// A slice does not have the required exact length.
    BufferLength,
    /// Non-finite, reversed or otherwise invalid configuration.
    Configuration,
    /// The requested row interval lies outside the frame.
    Rows,
    /// Accumulators must have identical configurations to merge.
    Incompatible,
    /// Storage could not be reserved.
    Allocation,
}

impl std::fmt::Display for Error {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "video scopes: {self:?}")
    }
}

impl std::error::Error for Error {}

/// ITU-R luma matrices (BT.2020 uses non-constant luminance).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ColorStandard {
    Bt601,
    #[default]
    Bt709,
    Bt2020,
}

impl ColorStandard {
    /// `(Kr, Kb)`; `Kg = 1 - Kr - Kb`, in the apps' original f32 order.
    pub fn coefficients(self) -> (f32, f32) {
        match self {
            Self::Bt601 => (0.299, 0.114),
            Self::Bt709 => (0.2126, 0.0722),
            Self::Bt2020 => (0.2627, 0.0593),
        }
    }

    /// Full-range Y' in 0..1 and signed Cb/Cr in -0.5..0.5 for nominal RGB.
    pub fn ycbcr(self, rgb: [f32; 3]) -> [f32; 3] {
        let (red, blue) = self.coefficients();
        let luma = red * rgb[0] + (1.0 - red - blue) * rgb[1] + blue * rgb[2];
        [luma, (rgb[2] - luma) / (2.0 * (1.0 - blue)), (rgb[0] - luma) / (2.0 * (1.0 - red))]
    }
}

/// Semantic traces, independent of the app's tinting and layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScopeKind {
    WaveformRgb,
    WaveformLuma,
    /// FilmCraft YC: Y and Y ± sqrt(Cb² + Cr²).
    WaveformYcEnvelope,
    /// EffectCraft YC: Y and sqrt(Cb² + Cr²).
    WaveformChroma,
    ParadeRgb,
    ParadeYuv,
    ParadeRgbWhite,
    /// R, G, B, Y; `Config::width` bins and height exactly 1.
    Histogram,
    /// Cb right, Cr up, independently of `Config::origin`.
    Vectorscope,
}

impl ScopeKind {
    /// Plane names in storage order.
    pub fn traces(self) -> &'static [&'static str] {
        match self {
            Self::WaveformRgb | Self::ParadeRgb => &["R", "G", "B"],
            Self::WaveformLuma => &["Y"],
            Self::WaveformYcEnvelope | Self::WaveformChroma => &["Y", "C"],
            Self::ParadeYuv => &["Y", "Cb", "Cr"],
            Self::ParadeRgbWhite | Self::Histogram => &["R", "G", "B", "Y"],
            Self::Vectorscope => &["CbCr"],
        }
    }
}

/// Waveform/parade row orientation. Histogram bins always increase left to right.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Origin {
    #[default]
    Bottom,
    Top,
}

/// Immutable configuration of one reusable accumulator.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Config {
    pub kind: ScopeKind,
    /// Columns per trace (not combined parade width), or histogram bins.
    pub width: usize,
    pub height: usize,
    pub standard: ColorStandard,
    /// Clamp RGB before waveform/vector conversion; histogram clamps each raw R/G/B/Y value.
    pub clamp: bool,
    /// Linear signal bounds. Choose [-0.1, 1.1] or [-0.25, 1.25] for extended range.
    pub range: [f32; 2],
    /// Half-width/height of the Cb/Cr plane (FilmCraft 0.6; EffectCraft 0.5).
    pub vector_extent: f32,
    pub origin: Origin,
}

impl Config {
    /// FilmCraft-compatible nominal defaults; dimensions are checked by `Accumulator::new`.
    pub fn new(kind: ScopeKind, width: usize, height: usize) -> Self {
        Self { kind, width, height, standard: ColorStandard::Bt709, clamp: true, range: [0.0, 1.0], vector_extent: 0.6, origin: Origin::Bottom }
    }

    fn validate(self) -> Result<usize, Error> {
        let cells = input::area(self.width, self.height)?;
        if self.width < 2
            || (self.kind == ScopeKind::Histogram && self.height != 1)
            || (self.kind != ScopeKind::Histogram && self.height < 2)
            || !self.range.iter().all(|value| value.is_finite())
            || self.range[0] >= self.range[1]
            || !(self.range[1] - self.range[0]).is_finite()
            || !self.vector_extent.is_finite()
            || self.vector_extent <= 0.0
        {
            return Err(Error::Configuration);
        }
        cells.checked_mul(self.kind.traces().len()).filter(|count| *count <= isize::MAX as usize / size_of::<u64>()).ok_or(Error::Dimensions)
    }

    fn row(self, value: f32) -> Option<usize> {
        let fraction = (value - self.range[0]) / (self.range[1] - self.range[0]);
        if !(0.0..=1.0).contains(&fraction) {
            return None;
        }
        let fraction = match self.origin {
            Origin::Bottom => fraction,
            Origin::Top => 1.0 - fraction,
        };
        Some(((fraction * (self.height - 1) as f32).round() as usize).min(self.height - 1))
    }

    /// Cell occupied by a chroma point, including a colour-bar target.
    pub fn vector_cell(self, cb: f32, cr: f32) -> Option<(usize, usize)> {
        if self.width < 2 || self.height < 2 || !self.vector_extent.is_finite() || self.vector_extent <= 0.0 {
            return None;
        }
        let last_x = (self.width - 1) as f32;
        let last_y = (self.height - 1) as f32;
        let column = (cb / self.vector_extent * 0.5 + 0.5) * last_x;
        let row = (0.5 - cr / self.vector_extent * 0.5) * last_y;
        if !(0.0..=last_x).contains(&column) || !(0.0..=last_y).contains(&row) {
            return None;
        }
        Some(((column.round() as usize).min(self.width - 1), (row.round() as usize).min(self.height - 1)))
    }
}

/// Density scaling only, not a signal transfer function. Does not modify raw counts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scale {
    Linear,
    Logarithmic,
}

impl Scale {
    /// Normalised intensity in 0..1. Empty bins stay zero; over-peak bins clamp to one.
    pub fn intensity(self, count: u64, peak: u64) -> f32 {
        if count == 0 || peak == 0 {
            return 0.0;
        }
        let count = count.min(peak) as f64;
        let peak = peak as f64;
        match self {
            Self::Linear => (count / peak) as f32,
            Self::Logarithmic => (count.ln_1p() / peak.ln_1p()) as f32,
        }
    }
}

/// Counts and diagnostics; no per-frame allocations or hidden threads.
#[derive(Debug)]
pub struct Accumulator {
    config: Config,
    counts: Vec<u64>,
    below: [u64; 4],
    above: [u64; 4],
    skipped: u64,
    saturated: bool,
}

impl Accumulator {
    pub fn new(config: Config) -> Result<Self, Error> {
        let count = config.validate()?;
        let mut counts = Vec::new();
        counts.try_reserve_exact(count).map_err(|_| Error::Allocation)?;
        counts.resize(count, 0);
        Ok(Self { config, counts, below: [0; 4], above: [0; 4], skipped: 0, saturated: false })
    }

    pub fn config(&self) -> Config {
        self.config
    }

    /// Plane-major, then row-major `u64` bins, in `ScopeKind::traces` order.
    pub fn counts(&self) -> &[u64] {
        &self.counts
    }

    pub fn plane(&self, trace: usize) -> Option<&[u64]> {
        let size = self.config.width * self.config.height;
        self.counts.chunks_exact(size).nth(trace)
    }

    /// Raw histogram values below/above its range, before clamping, R/G/B/Y order.
    pub fn out_of_range(&self) -> ([u64; 4], [u64; 4]) {
        (self.below, self.above)
    }

    /// Pixels with any non-finite RGB component; all their traces are skipped.
    pub fn skipped(&self) -> u64 {
        self.skipped
    }

    /// Sticky until clear: a bin or diagnostic counter exceeded `u64::MAX`.
    pub fn saturated(&self) -> bool {
        self.saturated
    }

    pub fn clear(&mut self) {
        self.counts.fill(0);
        self.below.fill(0);
        self.above.fill(0);
        self.skipped = 0;
        self.saturated = false;
    }

    /// Adds a frame without clearing existing counts.
    pub fn accumulate(&mut self, frame: Frame<'_>) -> Result<(), Error> {
        self.accumulate_rows(frame, 0..frame.height())
    }

    /// Adds complete rows. Use disjoint ranges in worker-local accumulators, then merge.
    /// Invalid ranges fail before changing any count; repeated ranges deliberately add again.
    pub fn accumulate_rows(&mut self, frame: Frame<'_>, rows: Range<usize>) -> Result<(), Error> {
        frame.validate_rows(&rows)?;
        for row in rows {
            for column in 0..frame.width() {
                let rgb = frame.pixel(row * frame.width() + column);
                if !rgb.iter().all(|value| value.is_finite()) {
                    add(&mut self.skipped, 1, &mut self.saturated);
                    continue;
                }
                if self.config.kind == ScopeKind::Histogram {
                    self.histogram(rgb);
                } else {
                    let rgb = if self.config.clamp { rgb.map(|value| value.clamp(0.0, 1.0)) } else { rgb };
                    let position = (column as u128 * self.config.width as u128 / frame.width() as u128) as usize;
                    self.plot(rgb, position);
                }
            }
        }
        Ok(())
    }

    /// Allocation-free associative, saturating reduction. Source stays reusable.
    pub fn merge(&mut self, other: &Self) -> Result<(), Error> {
        if self.config != other.config {
            return Err(Error::Incompatible);
        }
        for (destination, source) in self.counts.iter_mut().zip(&other.counts) {
            add(destination, *source, &mut self.saturated);
        }
        for channel in 0..4 {
            add(&mut self.below[channel], other.below[channel], &mut self.saturated);
            add(&mut self.above[channel], other.above[channel], &mut self.saturated);
        }
        add(&mut self.skipped, other.skipped, &mut self.saturated);
        self.saturated |= other.saturated;
        Ok(())
    }

    /// Fill caller-owned intensities, using one peak over all planes. No tinting or pixels.
    pub fn scaled_into(&self, scale: Scale, output: &mut [f32]) -> Result<(), Error> {
        if output.len() != self.counts.len() {
            return Err(Error::BufferLength);
        }
        let peak = self.counts.iter().copied().max().unwrap_or(0);
        for (output, count) in output.iter_mut().zip(&self.counts) {
            *output = scale.intensity(*count, peak);
        }
        Ok(())
    }

    fn bump(&mut self, trace: usize, column: usize, row: usize) {
        let index = (trace * self.config.height + row) * self.config.width + column;
        add(&mut self.counts[index], 1, &mut self.saturated);
    }

    fn level(&mut self, trace: usize, column: usize, value: f32) {
        let value = if self.config.clamp { value.clamp(0.0, 1.0) } else { value };
        if let Some(row) = self.config.row(value) {
            self.bump(trace, column, row);
        }
    }

    fn plot(&mut self, rgb: [f32; 3], column: usize) {
        let [luma, cb, cr] = self.config.standard.ycbcr(rgb);
        match self.config.kind {
            ScopeKind::WaveformRgb | ScopeKind::ParadeRgb | ScopeKind::ParadeRgbWhite => {
                for (trace, value) in rgb.into_iter().enumerate() {
                    self.level(trace, column, value);
                }
                if self.config.kind == ScopeKind::ParadeRgbWhite {
                    self.level(3, column, luma);
                }
            }
            ScopeKind::WaveformLuma => self.level(0, column, luma),
            ScopeKind::WaveformYcEnvelope | ScopeKind::WaveformChroma => {
                let chroma = (cb * cb + cr * cr).sqrt();
                self.level(0, column, luma);
                if self.config.kind == ScopeKind::WaveformChroma {
                    self.level(1, column, chroma);
                } else {
                    self.level(1, column, luma + chroma);
                    if chroma > 1e-6 {
                        self.level(1, column, luma - chroma);
                    }
                }
            }
            ScopeKind::ParadeYuv => {
                for (trace, value) in [luma, cb + 0.5, cr + 0.5].into_iter().enumerate() {
                    self.level(trace, column, value);
                }
            }
            ScopeKind::Vectorscope => {
                if let Some((column, row)) = self.config.vector_cell(cb, cr) {
                    self.bump(0, column, row);
                }
            }
            ScopeKind::Histogram => {}
        }
    }

    fn histogram(&mut self, rgb: [f32; 3]) {
        let luma = self.config.standard.ycbcr(rgb)[0];
        let [low, high] = self.config.range;
        for (trace, value) in [rgb[0], rgb[1], rgb[2], luma].into_iter().enumerate() {
            let fraction = (value - low) / (high - low);
            if !(0.0..=1.0 + 1e-6).contains(&fraction) {
                if value < low {
                    add(&mut self.below[trace], 1, &mut self.saturated);
                } else {
                    add(&mut self.above[trace], 1, &mut self.saturated);
                }
                if !self.config.clamp {
                    continue;
                }
            }
            let value = if self.config.clamp { value.clamp(0.0, 1.0) } else { value };
            let fraction = (value - low) / (high - low);
            if (0.0..=1.0 + 1e-6).contains(&fraction) {
                let bin = ((fraction * (self.config.width - 1) as f32).round() as usize).min(self.config.width - 1);
                self.bump(trace, bin, 0);
            }
        }
    }
}

fn add(destination: &mut u64, count: u64, saturated: &mut bool) {
    let (sum, overflow) = destination.overflowing_add(count);
    *destination = if overflow { u64::MAX } else { sum };
    *saturated |= overflow;
}
