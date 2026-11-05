//! Rational-ratio polyphase FIR resampler.
//!
//! The reference definition is the direct form: zero-stuff the input by `L`, convolve with the
//! prototype FIR `h`, keep every `M`-th sample.
//!
//! ```text
//! u[m] = x[m / L]  if L divides m, else 0
//! y[n] = (h * u)[n * M] = sum_k h[k] * u[n*M - k]
//! ```
//!
//! The polyphase form never builds `u`. Output `n` sits at upsampled index `n*M = i*L + p`
//! (`i` the whole input position, `p < L` the phase), and
//!
//! ```text
//! y[n] = sum_q h[p + q*L] * x[i - q]
//! ```
//!
//! [`Polyphase`] stores `coeffs[p * taps + q] = h[p + q*L]` (zero past the end of `h`) and walks
//! `(i, p)` with an integer accumulator: `p += M mod L`, `i += M div L`, carrying one into `i`
//! when `p` reaches `L`. All arithmetic on positions is exact integer arithmetic, so there is no
//! phase drift however long the stream runs. Samples outside the input read as zero, so
//! [`Polyphase::process`] over `0..out_len(x.len())` is the full convolution output.
//!
//! The equality of the two forms for every input, and the DC gain of a per-phase normalised
//! prototype, are proved in Lean 4 in `lean/` (see `lean/TRUST.md`).

use std::f64::consts::PI;
use std::ops::{Add, Mul};

/// Sample and coefficient type of a [`Polyphase`] resampler.
pub trait Sample: Copy + Default + Add<Output = Self> + Mul<Output = Self> {}
impl Sample for f32 {}
impl Sample for f64 {}

/// Why a resampler could not be built.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// `L` or `M` is zero.
    ZeroRatio,
    /// The prototype filter is empty (or zero taps per phase were requested).
    EmptyPrototype,
    /// The tap table would exceed `usize`.
    TooLarge,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::ZeroRatio => "upsample and downsample factors must be non-zero",
            Self::EmptyPrototype => "prototype filter must have at least one tap",
            Self::TooLarge => "tap table too large",
        })
    }
}

impl std::error::Error for Error {}

/// Polyphase resampler by `up / down`.
#[derive(Clone, Debug)]
pub struct Polyphase<S: Sample> {
    up: usize,
    down: usize,
    taps: usize,
    proto_len: usize,
    /// `coeffs[p * taps + q] = h[p + q * up]`, zero past the end of `h`.
    coeffs: Vec<S>,
}

impl<S: Sample> Polyphase<S> {
    /// Splits the prototype `h` (index 0 first) into `up` phases. `up` and `down` are used as
    /// given (not reduced): the direct form is defined for any positive pair.
    pub fn from_prototype(h: &[S], up: usize, down: usize) -> Result<Self, Error> {
        if up == 0 || down == 0 {
            return Err(Error::ZeroRatio);
        }
        if h.is_empty() {
            return Err(Error::EmptyPrototype);
        }
        let taps = h.len().div_ceil(up);
        let mut coeffs = vec![S::default(); up.checked_mul(taps).ok_or(Error::TooLarge)?];
        for (k, &c) in h.iter().enumerate() {
            if let Some(slot) = coeffs.get_mut((k % up) * taps + k / up) {
                *slot = c;
            }
        }
        Ok(Self { up, down, taps, proto_len: h.len(), coeffs })
    }

    /// `(up, down)`.
    pub fn ratio(&self) -> (usize, usize) {
        (self.up, self.down)
    }

    /// Taps per phase.
    pub fn taps_per_phase(&self) -> usize {
        self.taps
    }

    /// The coefficients of phase `p` (`h[p], h[p + L], ...`), if `p < L`.
    pub fn phase(&self, p: usize) -> Option<&[S]> {
        self.coeffs.get(p.checked_mul(self.taps)?..)?.get(..self.taps)
    }

    /// Length of the full convolution output for an input of `len` samples:
    /// `ceil(((len - 1) * L + N) / M)` where `N` is the prototype length; 0 for an empty input.
    pub fn out_len(&self, len: usize) -> usize {
        if len == 0 {
            return 0;
        }
        ((len - 1) * self.up + self.proto_len).div_ceil(self.down)
    }

    /// Output samples `first .. first + count` of the resampled stream. The position accumulator
    /// starts at `(first * M) divmod L` and is then advanced by integer steps.
    pub fn process(&self, x: &[S], first: usize, count: usize) -> Vec<S> {
        self.process_from(x, first as u128 * self.down as u128, count)
    }

    /// `count` outputs where output `n` sits at upsampled index `start + n*M`, i.e.
    /// `y[n] = (h * u)[start + n*M]`. [`Polyphase::process`] is `start = first * M`. Any `start`
    /// is allowed, so a filter centred on the output (taps reaching `lead` input samples ahead)
    /// is `start = lead * L + first * M`: the same theorem as a start offset of the direct form.
    pub fn process_from(&self, x: &[S], start: u128, count: usize) -> Vec<S> {
        let mut out = vec![S::default(); count];
        self.process_from_into(x, start, &mut out);
        out
    }

    /// [`Polyphase::process_from`] writing `out.len()` samples into `out`: no allocation, for
    /// real-time callers (for example a per-sample window of the last few inputs).
    pub fn process_from_into(&self, x: &[S], start: u128, out: &mut [S]) {
        let (mut i, mut p) = ((start / self.up as u128) as i64, (start % self.up as u128) as usize);
        let (step_i, step_p) = ((self.down / self.up) as i64, self.down % self.up);
        let len = x.len() as i64;
        for slot in out.iter_mut() {
            let mut acc = S::default();
            if let Some(w) = self.phase(p) {
                // taps q with 0 <= i - q < len
                let lo = (i - (len - 1)).max(0) as usize;
                let hi = if i < 0 { 0 } else { (i as usize + 1).min(self.taps) };
                for (q, &c) in w.iter().enumerate().take(hi).skip(lo) {
                    if let Some(&v) = x.get((i - q as i64) as usize) {
                        acc = acc + c * v;
                    }
                }
            }
            *slot = acc;
            i += step_i;
            p += step_p;
            if p >= self.up {
                p -= self.up;
                i += 1;
            }
        }
    }

    /// The whole resampled signal, `out_len(x.len())` samples.
    pub fn process_all(&self, x: &[S]) -> Vec<S> {
        self.process(x, 0, self.out_len(x.len()))
    }
}

/// The reference direct form, `y[n] = sum_k h[k] * u[n*M - k]` with `u` the zero-stuffed input,
/// computed literally (builds `u`). For tests and verification only: cost is `O(L * len * N)`.
pub fn direct_form<S: Sample>(x: &[S], h: &[S], up: usize, down: usize, count: usize) -> Vec<S> {
    let mut u = vec![S::default(); if x.is_empty() { 0 } else { (x.len() - 1) * up + 1 }];
    for (j, &v) in x.iter().enumerate() {
        if let Some(slot) = u.get_mut(j * up) {
            *slot = v;
        }
    }
    (0..count)
        .map(|n| {
            let m = n * down;
            let mut acc = S::default();
            for (k, &c) in h.iter().enumerate() {
                if let Some(&v) = m.checked_sub(k).and_then(|j| u.get(j)) {
                    acc = acc + c * v;
                }
            }
            acc
        })
        .collect()
}

fn bessel_i0(x: f64) -> f64 {
    let (mut sum, mut term, q) = (1.0, 1.0, x * x / 4.0);
    for k in 1..200 {
        term *= q / (k * k) as f64;
        sum += term;
        if term < sum * 1e-17 {
            break;
        }
    }
    sum
}

/// Kaiser-windowed sinc prototype of length `up * taps_per_phase` for resampling by `up / down`,
/// cut off at the lower of the two Nyquist frequencies, with every phase normalised to sum to 1
/// (so the whole filter sums to `up`, the gain of zero-stuffing, and DC passes unchanged).
pub fn kaiser_prototype(up: usize, down: usize, taps_per_phase: usize, beta: f64) -> Result<Vec<f64>, Error> {
    if up == 0 || down == 0 {
        return Err(Error::ZeroRatio);
    }
    let len = up.checked_mul(taps_per_phase).ok_or(Error::TooLarge)?;
    if len == 0 {
        return Err(Error::EmptyPrototype);
    }
    let centre = (len - 1) as f64 / 2.0;
    let cutoff = 1.0 / up.max(down) as f64; // fraction of the upsampled Nyquist
    let i0b = bessel_i0(beta);
    let mut h: Vec<f64> = (0..len)
        .map(|m| {
            let t = (m as f64 - centre) * cutoff;
            let sinc = if t.abs() < 1e-12 { 1.0 } else { (PI * t).sin() / (PI * t) };
            let r = (m as f64 - centre) / (centre + 1.0);
            sinc * bessel_i0(beta * (1.0 - r * r).max(0.0).sqrt()) / i0b
        })
        .collect();
    for p in 0..up {
        let sum: f64 = h.iter().skip(p).step_by(up).sum();
        if sum.abs() > 1e-300 {
            h.iter_mut().skip(p).step_by(up).for_each(|v| *v /= sum);
        }
    }
    Ok(h)
}
