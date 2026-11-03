use crate::{Complex, Error, RealFft, Sample, check_len, check_size};

/// Frame-wise STFT with weighted overlap-add synthesis and explicit tail drain.
/// Frames are supplied by the caller at offsets `0, hop, 2*hop, ...`.
pub struct Stft<T: Sample> {
    fft: RealFft<T>,
    window: Vec<T>,
    hop: usize,
    frame: Vec<T>,
    overlap: Vec<T>,
    weights: Vec<T>,
    pending: usize,
}

impl<T: Sample> Stft<T> {
    /// Allocates all buffers. Every hop phase must have nonzero window energy.
    pub fn new(window: &[T], hop: usize) -> Result<Self, Error> {
        check_size::<T>(window.len())?;
        if hop == 0 || hop > window.len() {
            return Err(Error::InvalidHop);
        }
        if window.iter().any(|value| !value.as_f64().is_finite())
            || (0..hop).any(|phase| {
                let energy: f64 = window[phase..].iter().step_by(hop).map(|value| value.as_f64().powi(2)).sum();
                energy <= 0.0 || !T::scalar(energy).as_f64().is_finite() || T::scalar(energy) == T::zero()
            })
        {
            return Err(Error::InvalidWindow);
        }
        Ok(Self {
            fft: RealFft::new(window.len())?,
            window: window.to_vec(),
            hop,
            frame: vec![T::zero(); window.len()],
            overlap: vec![T::zero(); window.len()],
            weights: vec![T::zero(); window.len()],
            pending: 0,
        })
    }

    pub fn frame_len(&self) -> usize {
        self.window.len()
    }

    pub fn spectrum_len(&self) -> usize {
        self.fft.spectrum_len()
    }

    pub fn hop(&self) -> usize {
        self.hop
    }

    /// Windows a complete frame and writes its unscaled real FFT; no allocation.
    pub fn analyze_into(&mut self, input: &[T], output: &mut [Complex<T>]) -> Result<(), Error> {
        check_len(input.len(), self.frame_len())?;
        check_len(output.len(), self.spectrum_len())?;
        for ((value, input), window) in self.frame.iter_mut().zip(input).zip(&self.window) {
            *value = *input * *window;
        }
        self.fft.forward_into(&self.frame, output)
    }

    /// Adds a windowed inverse frame and emits exactly `hop()` finalized samples.
    /// Divides by accumulated squared windows; an uncovered boundary emits zero.
    pub fn synthesize_into(&mut self, input: &[Complex<T>], output: &mut [T]) -> Result<(), Error> {
        check_len(output.len(), self.hop)?;
        self.fft.inverse_into(input, &mut self.frame)?;
        for index in 0..self.frame_len() {
            self.overlap[index] = self.overlap[index] + self.frame[index] * self.window[index];
            self.weights[index] = self.weights[index] + self.window[index] * self.window[index];
        }
        self.pending = self.frame_len();
        self.flush_into(output)?;
        Ok(())
    }

    /// Drains up to one hop without adding a frame. Returns the valid sample count;
    /// any unused output suffix is zero. Repeat until zero, without allocations.
    pub fn flush_into(&mut self, output: &mut [T]) -> Result<usize, Error> {
        check_len(output.len(), self.hop)?;
        let count = self.pending.min(self.hop);
        output.fill(T::zero());
        for (index, value) in output.iter_mut().enumerate().take(count) {
            if self.weights[index] > T::zero() {
                *value = self.overlap[index] / self.weights[index];
            }
        }
        self.overlap.copy_within(self.hop.., 0);
        self.weights.copy_within(self.hop.., 0);
        let tail = self.frame_len() - self.hop;
        self.overlap[tail..].fill(T::zero());
        self.weights[tail..].fill(T::zero());
        self.pending -= count;
        Ok(count)
    }

    /// Clears synthesis history without reallocating; analysis is stateless.
    pub fn reset(&mut self) {
        self.overlap.fill(T::zero());
        self.weights.fill(T::zero());
        self.pending = 0;
    }
}
