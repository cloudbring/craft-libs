use crate::{Complex, Error, Sample, check_len, check_size};
use rustfft::num_traits::Zero;
use rustfft::{Fft, FftPlanner};
use std::{f64::consts::PI, sync::Arc};

/// Full-overlap, symmetric Princen-Bradley windows (50% overlap).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TdacWindow {
    Sine,
    /// Kaiser-Bessel-derived window; AAC uses alpha 4 (long), 6 (short).
    Kbd {
        alpha: f64,
    },
}

/// Allocates a length-N sine window, `sin(pi * (n + 1/2) / N)`.
pub fn sine_window<T: Sample>(length: usize) -> Result<Vec<T>, Error> {
    check_size::<T>(length)?;
    Ok((0..length).map(|index| T::scalar((PI * (index as f64 + 0.5) / length as f64).sin())).collect())
}

fn bessel_i0(value: f64) -> f64 {
    let mut sum = 1.0;
    let mut term = 1.0;
    for order in 1..=256 {
        term *= (value / (2 * order) as f64).powi(2);
        sum += term;
        if term <= sum * f64::EPSILON {
            break;
        }
    }
    sum
}

/// Allocates an even-length KBD window. Finite alpha in `0..=64` is supported.
pub fn kbd_window<T: Sample>(length: usize, alpha: f64) -> Result<Vec<T>, Error> {
    check_size::<T>(length)?;
    if !length.is_multiple_of(2) || !alpha.is_finite() || !(0.0..=64.0).contains(&alpha) {
        return Err(Error::InvalidWindow);
    }
    let half = length / 2;
    let kaiser: Vec<f64> = (0..=half)
        .map(|index| {
            let position = 2.0 * index as f64 / half as f64 - 1.0;
            bessel_i0(PI * alpha * (1.0 - position * position).max(0.0).sqrt())
        })
        .collect();
    let total: f64 = kaiser.iter().sum();
    let mut cumulative = 0.0;
    let mut window = vec![T::zero(); length];
    for index in 0..half {
        cumulative += kaiser[index];
        let value = T::scalar((cumulative / total).sqrt());
        window[index] = value;
        window[length - 1 - index] = value;
    }
    Ok(window)
}

struct Kernel<T: Sample> {
    window: Vec<T>,
    folded: Vec<T>,
    fft: Arc<dyn Fft<T>>,
    buffer: Vec<Complex<T>>,
    scratch: Vec<Complex<T>>,
    pre: Vec<Complex<T>>,
    post: Vec<Complex<T>>,
}

impl<T: Sample> Kernel<T> {
    fn new(coefficients: usize, window: TdacWindow) -> Result<Self, Error> {
        check_size::<T>(coefficients)?;
        if !coefficients.is_multiple_of(2) {
            return Err(Error::InvalidSize);
        }
        let window = match window {
            TdacWindow::Sine => sine_window(coefficients * 2)?,
            TdacWindow::Kbd { alpha } => kbd_window(coefficients * 2, alpha)?,
        };
        let half = coefficients / 2;
        let mut planner = FftPlanner::new();
        let fft = planner.plan_fft_inverse(half);
        let rotation = |index: f64| {
            let angle = PI * index / coefficients as f64;
            Complex::new(T::scalar(angle.cos()), T::scalar(angle.sin()))
        };
        Ok(Self {
            window,
            folded: vec![T::zero(); coefficients],
            scratch: vec![Complex::zero(); fft.get_inplace_scratch_len()],
            fft,
            buffer: vec![Complex::zero(); half],
            pre: (0..half).map(|index| rotation(index as f64)).collect(),
            post: (0..half).map(|index| rotation(index as f64 + 0.25)).collect(),
        })
    }

    fn dct4(&mut self) {
        let length = self.folded.len();
        for (index, value) in self.buffer.iter_mut().enumerate() {
            *value = Complex::new(self.folded[2 * index], -self.folded[length - 1 - 2 * index]) * self.pre[index];
        }
        self.fft.process_with_scratch(&mut self.buffer, &mut self.scratch);
        for (index, value) in self.buffer.iter().enumerate() {
            let rotated = *value * self.post[index];
            self.folded[2 * index] = rotated.re;
            self.folded[length - 1 - 2 * index] = rotated.im;
        }
    }
}

/// Windowed MDCT: `2*M` input samples become `M` unscaled coefficients.
/// `M` must be positive and even; mixed-radix sizes are supported.
pub struct Mdct<T: Sample> {
    kernel: Kernel<T>,
}

impl<T: Sample> Mdct<T> {
    /// Allocates windows, rotations, the cached `M/2` complex FFT, and scratch.
    pub fn new(coefficients: usize, window: TdacWindow) -> Result<Self, Error> {
        Ok(Self { kernel: Kernel::new(coefficients, window)? })
    }

    pub fn coefficients_len(&self) -> usize {
        self.kernel.folded.len()
    }

    pub fn window(&self) -> &[T] {
        &self.kernel.window
    }

    /// Preserves input, overwrites output, and never allocates.
    pub fn process_into(&mut self, input: &[T], output: &mut [T]) -> Result<(), Error> {
        let length = self.coefficients_len();
        check_len(input.len(), 2 * length)?;
        check_len(output.len(), length)?;
        let half = length / 2;
        let window = &self.kernel.window;
        for index in 0..half {
            self.kernel.folded[index] =
                -input[3 * half - 1 - index] * window[3 * half - 1 - index] - input[3 * half + index] * window[3 * half + index];
            self.kernel.folded[half + index] = input[index] * window[index] - input[length - 1 - index] * window[length - 1 - index];
        }
        self.kernel.dct4();
        output.copy_from_slice(&self.kernel.folded);
        Ok(())
    }
}

/// Windowed IMDCT: `M` coefficients become `2*M` samples, scaled by `2/M`.
/// Add blocks spaced M samples apart to cancel time-domain aliasing.
pub struct Imdct<T: Sample> {
    kernel: Kernel<T>,
}

impl<T: Sample> Imdct<T> {
    /// Allocates once, just like [`Mdct::new`].
    pub fn new(coefficients: usize, window: TdacWindow) -> Result<Self, Error> {
        Ok(Self { kernel: Kernel::new(coefficients, window)? })
    }

    pub fn coefficients_len(&self) -> usize {
        self.kernel.folded.len()
    }

    pub fn window(&self) -> &[T] {
        &self.kernel.window
    }

    /// Preserves input and overwrites (does not add to) output; never allocates.
    pub fn process_into(&mut self, input: &[T], output: &mut [T]) -> Result<(), Error> {
        let length = self.coefficients_len();
        check_len(input.len(), length)?;
        check_len(output.len(), 2 * length)?;
        self.kernel.folded.copy_from_slice(input);
        self.kernel.dct4();
        let half = length / 2;
        let scale = T::scalar(2.0 / length as f64);
        for index in 0..half {
            output[index] = self.kernel.folded[half + index];
            output[half + index] = -self.kernel.folded[length - 1 - index];
            output[length + index] = -self.kernel.folded[half - 1 - index];
            output[3 * half + index] = -self.kernel.folded[index];
        }
        for (value, window) in output.iter_mut().zip(&self.kernel.window) {
            *value = *value * *window * scale;
        }
        Ok(())
    }
}
