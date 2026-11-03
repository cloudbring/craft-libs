#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![forbid(unsafe_code)]
#![doc = include_str!("../README.md")]

mod mdct;
mod stft;

pub use mdct::{Imdct, Mdct, TdacWindow, kbd_window, sine_window};
pub use rustfft::num_complex::Complex;
pub use stft::Stft;

use realfft::{ComplexToReal, RealFftPlanner, RealToComplex};
use rustfft::num_traits::Zero;
use rustfft::{Fft, FftNum, FftPlanner};
use std::{fmt, sync::Arc};

mod sealed {
    pub trait Sealed {}
    impl Sealed for f32 {}
    impl Sealed for f64 {}
}

/// Supported scalar precision: `f32` or `f64`.
pub trait Sample: FftNum + PartialOrd + sealed::Sealed {
    #[doc(hidden)]
    fn scalar(value: f64) -> Self;
    #[doc(hidden)]
    fn as_f64(self) -> f64;
}

impl Sample for f32 {
    fn scalar(value: f64) -> Self {
        value as Self
    }
    fn as_f64(self) -> f64 {
        self as f64
    }
}

impl Sample for f64 {
    fn scalar(value: f64) -> Self {
        value
    }
    fn as_f64(self) -> f64 {
        self
    }
}

/// Transform direction. Only inverse FFTs apply `1/N` scaling.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Forward,
    Inverse,
}

/// Invalid arguments are reported before invoking backend buffer assertions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    InvalidSize,
    Length { expected: usize, actual: usize },
    InvalidWindow,
    InvalidHop,
    InvalidSpectrum,
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSize => formatter.write_str("invalid or overflowing transform size"),
            Self::Length { expected, actual } => {
                write!(formatter, "expected {expected} elements, got {actual}")
            }
            Self::InvalidWindow => formatter.write_str("invalid window or uncovered overlap phase"),
            Self::InvalidHop => formatter.write_str("hop must be in 1..=window length"),
            Self::InvalidSpectrum => formatter.write_str("DC/Nyquist bins must be real"),
        }
    }
}

impl std::error::Error for Error {}

fn check_len(actual: usize, expected: usize) -> Result<(), Error> {
    if actual == expected { Ok(()) } else { Err(Error::Length { expected, actual }) }
}

fn check_size<T>(length: usize) -> Result<(), Error> {
    if length == 0 || length > isize::MAX as usize / (16 * size_of::<T>()) { Err(Error::InvalidSize) } else { Ok(()) }
}

/// Fixed-size complex plan with cached forward/inverse kernels and owned scratch.
/// Construction allocates. All processing methods are allocation-free.
pub struct ComplexFft<T: Sample> {
    forward: Arc<dyn Fft<T>>,
    inverse: Arc<dyn Fft<T>>,
    scratch: Vec<Complex<T>>,
}

impl<T: Sample> ComplexFft<T> {
    pub fn new(length: usize) -> Result<Self, Error> {
        check_size::<T>(length)?;
        let mut planner = FftPlanner::new();
        let forward = planner.plan_fft_forward(length);
        let inverse = planner.plan_fft_inverse(length);
        let scratch_len = forward.get_inplace_scratch_len().max(inverse.get_inplace_scratch_len());
        Ok(Self { forward, inverse, scratch: vec![Complex::zero(); scratch_len] })
    }

    pub fn len(&self) -> usize {
        self.forward.len()
    }

    pub fn is_empty(&self) -> bool {
        false
    }

    pub fn process_in_place(&mut self, direction: Direction, buffer: &mut [Complex<T>]) -> Result<(), Error> {
        check_len(buffer.len(), self.len())?;
        match direction {
            Direction::Forward => self.forward.process_with_scratch(buffer, &mut self.scratch),
            Direction::Inverse => {
                self.inverse.process_with_scratch(buffer, &mut self.scratch);
                let scale = T::scalar(1.0 / self.len() as f64);
                for value in buffer {
                    *value = *value * scale;
                }
            }
        }
        Ok(())
    }

    /// Preserves `input`; overwrites `output` without allocating.
    pub fn process_into(&mut self, direction: Direction, input: &[Complex<T>], output: &mut [Complex<T>]) -> Result<(), Error> {
        check_len(input.len(), self.len())?;
        check_len(output.len(), self.len())?;
        output.copy_from_slice(input);
        self.process_in_place(direction, output)
    }
}

/// Fixed-size real FFT, retaining `N/2 + 1` nonnegative-frequency bins.
/// Owns typed time/spectrum buffers: no unsafe packed-real reinterpretation.
pub struct RealFft<T: Sample> {
    forward: Arc<dyn RealToComplex<T>>,
    inverse: Arc<dyn ComplexToReal<T>>,
    scratch: Vec<Complex<T>>,
    time: Vec<T>,
    spectrum: Vec<Complex<T>>,
}

impl<T: Sample> RealFft<T> {
    /// Allocates both kernels, scratch, and staging buffers, including for odd N.
    pub fn new(length: usize) -> Result<Self, Error> {
        check_size::<T>(length)?;
        let mut planner = RealFftPlanner::new();
        let forward = planner.plan_fft_forward(length);
        let inverse = planner.plan_fft_inverse(length);
        let scratch_len = forward.get_scratch_len().max(inverse.get_scratch_len());
        Ok(Self {
            forward,
            inverse,
            scratch: vec![Complex::zero(); scratch_len],
            time: vec![T::zero(); length],
            spectrum: vec![Complex::zero(); length / 2 + 1],
        })
    }

    pub fn len(&self) -> usize {
        self.time.len()
    }

    pub fn is_empty(&self) -> bool {
        false
    }

    pub fn spectrum_len(&self) -> usize {
        self.spectrum.len()
    }

    pub fn time(&self) -> &[T] {
        &self.time
    }

    pub fn time_mut(&mut self) -> &mut [T] {
        &mut self.time
    }

    pub fn spectrum(&self) -> &[Complex<T>] {
        &self.spectrum
    }

    pub fn spectrum_mut(&mut self) -> &mut [Complex<T>] {
        &mut self.spectrum
    }

    /// Transforms the owned typed buffers, destroying the source buffer.
    /// Forward writes `spectrum()`; inverse writes `time()` scaled by `1/N`.
    pub fn process_in_place(&mut self, direction: Direction) -> Result<(), Error> {
        match direction {
            Direction::Forward => {
                self.forward.process_with_scratch(&mut self.time, &mut self.spectrum, &mut self.scratch).map_err(|_| Error::InvalidSpectrum)?
            }
            Direction::Inverse => {
                self.check_spectrum(&self.spectrum)?;
                self.inverse.process_with_scratch(&mut self.spectrum, &mut self.time, &mut self.scratch).map_err(|_| Error::InvalidSpectrum)?;
                let scale = T::scalar(1.0 / self.len() as f64);
                for value in &mut self.time {
                    *value = *value * scale;
                }
            }
        }
        Ok(())
    }

    /// Allocation-free; preserves input. Output has `spectrum_len()` elements.
    pub fn forward_into(&mut self, input: &[T], output: &mut [Complex<T>]) -> Result<(), Error> {
        check_len(input.len(), self.len())?;
        check_len(output.len(), self.spectrum_len())?;
        self.time.copy_from_slice(input);
        self.process_in_place(Direction::Forward)?;
        output.copy_from_slice(&self.spectrum);
        Ok(())
    }

    /// Allocation-free; preserves input. DC and (even N) Nyquist must be real.
    pub fn inverse_into(&mut self, input: &[Complex<T>], output: &mut [T]) -> Result<(), Error> {
        check_len(input.len(), self.spectrum_len())?;
        check_len(output.len(), self.len())?;
        self.check_spectrum(input)?;
        self.spectrum.copy_from_slice(input);
        self.process_in_place(Direction::Inverse)?;
        output.copy_from_slice(&self.time);
        Ok(())
    }

    fn check_spectrum(&self, input: &[Complex<T>]) -> Result<(), Error> {
        if input[0].im != T::zero() || (self.len().is_multiple_of(2) && input[self.len() / 2].im != T::zero()) {
            Err(Error::InvalidSpectrum)
        } else {
            Ok(())
        }
    }
}
