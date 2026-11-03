#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![forbid(unsafe_code)]
#![doc = include_str!("../README.md")]
//! Separable filters for packed, interleaved f32 planes, without application or threading policy.
//!
//! See the crate README for numerical metrics, approximation domains and parallel scheduling.
//! Transparent planes must already be premultiplied; no alpha division is performed.

mod kernel;
mod lines;
mod native;
mod recursive;

pub use kernel::{box_widths, gaussian_kernel};
pub use recursive::Recursive;
use std::sync::atomic::{AtomicBool, Ordering};

/// Independent lines advanced together, and the maximum cancellation latency in lines.
pub const LINE_BATCH: usize = 8;

/// Requested quality, never selected implicitly from image dimensions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Quality {
    /// Normalized, variance-calibrated sampled Gaussian at every supported sigma.
    Exact,
    /// Variance-exact YvV with Triggs–Sdika boundaries. LightCraft's sampled fast path below
    /// sigma 2.5 also covers the 2.5–8 guard band for the 0.01 impulse-shape bound.
    /// Above 65,536 the variance-calibrated sampled kernel is used as well.
    Recursive,
    /// Unguarded YvV with Triggs–Sdika boundaries, matching LightCraft's V3 kernel for
    /// Clamp edges through sigma 65,536. Uses sampled Exact below 2.5 and above 65,536.
    /// Unlike Recursive, the 2.5–8 band does not promise a 0.01 impulse-shape bound.
    RecursiveRaw,
    /// Kovesi odd-width boxes. Only three passes are supported. Construction rejects widths
    /// whose relative variance error exceeds `1 / (3 * sigma)` (notably some subpixel sigmas).
    BoxApprox { passes: usize },
}

/// Extension of the original signal for each separable pass.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edge {
    /// Repeat the nearest pixel.
    Clamp,
    /// Half-sample reflection: `... b a | a b c | c b ...`.
    Mirror,
    /// Every channel is zero outside the plane; border DC is intentionally not preserved.
    Zero,
    /// Zero extension of premultiplied gray-alpha or RGBA (two or four channels).
    /// Inputs and outputs remain premultiplied, including hidden colors at zero alpha.
    Transparent,
}

/// Invalid inputs and cooperative interruption, returned rather than panicking.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Error {
    InvalidSigma,
    InvalidChannels,
    InvalidDimensions,
    InvalidPasses,
    SizeOverflow,
    Allocation,
    Cancelled,
    /// Quantized boxes cannot satisfy the requested relative variance bound.
    BoxTolerance {
        actual: f64,
        bound: f64,
    },
}

impl std::fmt::Display for Error {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "blur: {self:?}")
    }
}

impl std::error::Error for Error {}

/// Packed image layout; construction validates channels and all size arithmetic.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Shape {
    width: usize,
    height: usize,
    channels: usize,
}

impl Shape {
    pub fn new(width: usize, height: usize, channels: usize) -> Result<Self, Error> {
        validate_channels(channels, Edge::Clamp)?;
        let samples = width.checked_mul(height).and_then(|count| count.checked_mul(channels)).ok_or(Error::SizeOverflow)?;
        if samples > isize::MAX as usize / size_of::<f32>() || width.max(height) > isize::MAX as usize / 2 {
            return Err(Error::SizeOverflow);
        }
        Ok(Self { width, height, channels })
    }

    pub fn width(self) -> usize {
        self.width
    }

    pub fn height(self) -> usize {
        self.height
    }

    pub fn channels(self) -> usize {
        self.channels
    }

    pub fn samples(self) -> usize {
        self.width * self.height * self.channels
    }
}

/// Cancellation checked before each batch of at most eight independent lines.
#[derive(Clone, Copy, Default)]
pub enum Cancel<'a> {
    #[default]
    Never,
    Flag(&'a AtomicBool),
    Hook(&'a (dyn Fn() -> bool + Sync)),
}

impl Cancel<'_> {
    pub fn cancelled(self) -> bool {
        match self {
            Self::Never => false,
            Self::Flag(flag) => flag.load(Ordering::Relaxed),
            Self::Hook(hook) => hook(),
        }
    }

    fn check(self) -> Result<(), Error> {
        if self.cancelled() { Err(Error::Cancelled) } else { Ok(()) }
    }
}

type Block = [[f64; LINE_BATCH]; 4];

/// Reuse one per worker. No per-line allocation after its high-water capacities are reached.
#[derive(Default)]
pub struct Scratch {
    input: Vec<Block>,
    output: Vec<Block>,
    forward: Vec<Block>,
    native1: native::Native<1>,
    native2: native::Native<2>,
    native3: native::Native<3>,
    native4: native::Native<4>,
}

/// A device-neutral pass description. Boxes contain odd widths, not radii.
#[derive(Clone, Debug, PartialEq)]
pub enum Separable {
    Kernel(Vec<f32>),
    Recursive(Recursive),
    Boxes([usize; 3]),
}

impl Separable {
    /// Finite support per axis, or None for the infinite-support recurrence.
    pub fn reach(&self) -> Option<usize> {
        match self {
            Self::Kernel(kernel) => Some(kernel.len() / 2),
            Self::Recursive(_) => None,
            Self::Boxes(widths) => widths.iter().try_fold(0usize, |sum, width| sum.checked_add(width / 2)),
        }
    }
}

/// Precomputed, immutable Gaussian; share across workers and retain their separate scratch.
#[derive(Clone, Debug)]
pub struct Gaussian {
    sigma: f32,
    quality: Quality,
    passes: Separable,
}

impl Gaussian {
    pub fn new(sigma: f32, quality: Quality) -> Result<Self, Error> {
        validate_sigma(sigma)?;
        let passes = match quality {
            Quality::Exact => Separable::Kernel(gaussian_kernel(sigma)?),
            Quality::Recursive if sigma < 8.0 || sigma > 65_536.0 => Separable::Kernel(gaussian_kernel(sigma)?),
            Quality::RecursiveRaw if sigma < 2.5 || sigma > 65_536.0 => Separable::Kernel(gaussian_kernel(sigma)?),
            Quality::Recursive | Quality::RecursiveRaw => Separable::Recursive(Recursive::new(sigma)?),
            Quality::BoxApprox { passes: 3 } => {
                let widths = box_widths(sigma)?;
                let variance: f64 = widths.iter().map(|width| ((*width as f64).powi(2) - 1.0) / 12.0).sum();
                let target = f64::from(sigma).powi(2);
                let actual = if target == 0.0 { 0.0 } else { (variance / target - 1.0).abs() };
                let bound = if target == 0.0 { 0.0 } else { 1.0 / (3.0 * f64::from(sigma)) };
                if actual > bound + 1e-14 {
                    return Err(Error::BoxTolerance { actual, bound });
                }
                Separable::Boxes(widths)
            }
            Quality::BoxApprox { .. } => return Err(Error::InvalidPasses),
        };
        Ok(Self { sigma, quality, passes })
    }

    pub fn sigma(&self) -> f32 {
        self.sigma
    }

    pub fn quality(&self) -> Quality {
        self.quality
    }

    pub fn separable(&self) -> &Separable {
        &self.passes
    }

    /// Both axes, horizontal then vertical. Cancellation may leave completed batches changed;
    /// the caller must discard a cancelled render. A layout error never modifies the plane.
    pub fn apply(&self, pixels: &mut [f32], shape: Shape, edge: Edge, scratch: &mut Scratch, cancel: Cancel<'_>) -> Result<(), Error> {
        validate_channels(shape.channels, edge)?;
        if pixels.len() != shape.samples() {
            return Err(Error::InvalidDimensions);
        }
        cancel.check()?;
        if pixels.is_empty() {
            return Ok(());
        }
        if edge == Edge::Clamp
            && let Separable::Kernel(kernel) = &self.passes
        {
            return native::apply(kernel, pixels, shape, scratch, cancel);
        }
        let row_samples = shape.width * shape.channels;
        let mut rows = Vec::new();
        rows.try_reserve_exact(shape.height).map_err(|_| Error::Allocation)?;
        self.filter_rows(pixels, shape, edge, scratch, cancel)?;
        rows.extend(pixels.chunks_exact_mut(row_samples));
        self.filter_columns(&mut rows, shape.channels, edge, scratch, cancel)
    }

    /// Horizontal passes over a packed set of complete rows. Apps can distribute disjoint row
    /// batches using their own rayon `for_each_init(Scratch::default, ...)`.
    pub fn filter_rows(&self, pixels: &mut [f32], shape: Shape, edge: Edge, scratch: &mut Scratch, cancel: Cancel<'_>) -> Result<(), Error> {
        let Shape { width, channels, .. } = shape;
        validate_channels(channels, edge)?;
        if pixels.len() != shape.samples() {
            return Err(Error::InvalidDimensions);
        }
        let row_samples = width.checked_mul(channels).ok_or(Error::SizeOverflow)?;
        if row_samples == 0 {
            return if pixels.is_empty() { cancel.check() } else { Err(Error::InvalidDimensions) };
        }
        if !pixels.len().is_multiple_of(row_samples) {
            return Err(Error::InvalidDimensions);
        }
        if edge == Edge::Clamp && !matches!(self.passes, Separable::Boxes(_)) {
            return native::rows(&self.passes, pixels, shape, scratch, cancel);
        }
        let batch_samples = row_samples.checked_mul(LINE_BATCH).ok_or(Error::SizeOverflow)?;
        for batch in pixels.chunks_mut(batch_samples) {
            cancel.check()?;
            let mut chunks = batch.chunks_mut(row_samples);
            let mut rows: [&mut [f32]; LINE_BATCH] = std::array::from_fn(|_| chunks.next().unwrap_or(&mut []));
            let count = rows.iter().take_while(|row| !row.is_empty()).count();
            lines::filter(&self.passes, &mut lines::View::rows(&mut rows[..count], channels), edge, scratch)?;
        }
        Ok(())
    }

    /// Vertical passes over equal-width row slices of a disjoint column stripe. Split each
    /// original row into stripes before distributing them; no transpose or unsafe code needed.
    pub fn filter_columns(
        &self,
        rows: &mut [&mut [f32]],
        channels: usize,
        edge: Edge,
        scratch: &mut Scratch,
        cancel: Cancel<'_>,
    ) -> Result<(), Error> {
        validate_channels(channels, edge)?;
        let width = rows.first().map_or(0, |row| row.len());
        if !width.is_multiple_of(channels) || rows.iter().any(|row| row.len() != width) {
            return Err(Error::InvalidDimensions);
        }
        cancel.check()?;
        if !rows.is_empty()
            && width != 0
            && edge == Edge::Clamp
            && let Separable::Recursive(coefficients) = &self.passes
        {
            return native::columns(coefficients, rows, channels, scratch, cancel);
        }
        for start in (0..width / channels).step_by(LINE_BATCH) {
            cancel.check()?;
            lines::filter(&self.passes, &mut lines::View::columns(rows, channels, start), edge, scratch)?;
        }
        Ok(())
    }
}

/// One running-sum box pass along each axis; radius zero is the identity.
pub fn box_blur(pixels: &mut [f32], shape: Shape, radius: usize, edge: Edge, scratch: &mut Scratch, cancel: Cancel<'_>) -> Result<(), Error> {
    let width = radius.checked_mul(2).and_then(|value| value.checked_add(1)).ok_or(Error::SizeOverflow)?;
    if width > isize::MAX as usize / 2 {
        return Err(Error::SizeOverflow);
    }
    Gaussian { sigma: 0.0, quality: Quality::BoxApprox { passes: 3 }, passes: Separable::Boxes([width, 1, 1]) }
        .apply(pixels, shape, edge, scratch, cancel)
}

fn validate_sigma(sigma: f32) -> Result<(), Error> {
    if sigma.is_finite() && sigma >= 0.0 { Ok(()) } else { Err(Error::InvalidSigma) }
}

fn validate_channels(channels: usize, edge: Edge) -> Result<(), Error> {
    if !(1..=4).contains(&channels) || (edge == Edge::Transparent && channels != 2 && channels != 4) { Err(Error::InvalidChannels) } else { Ok(()) }
}

fn resize<T: Clone>(buffer: &mut Vec<T>, length: usize, value: T) -> Result<(), Error> {
    if length > buffer.len() {
        buffer.try_reserve(length - buffer.len()).map_err(|_| Error::Allocation)?;
    }
    buffer.resize(length, value);
    Ok(())
}
