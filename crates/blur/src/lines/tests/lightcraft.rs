#![allow(dead_code, unexpected_cfgs)]
//! Verbatim bodies from cloudbring/lightcraft 39aa4d256939c3703f1e22c0f2a715a4c98a189f,
//! crates/raster/src/blur.rs::{variance_exact_kernel, Gaussian::{new, filter}}.
//! Only constructor visibility is widened; independent fixture never calls production math.

#[rustfmt::skip]
#[allow(clippy::type_complexity)]
#[path = "lightcraft/interleaved.rs"]
mod interleaved;

#[derive(Clone)]
pub struct Image<T> {
    pub width: usize,
    pub height: usize,
    pub data: Vec<T>,
}

impl<T: Copy + Default> Image<T> {
    pub fn new(width: usize, height: usize) -> Self {
        Self { width, height, data: vec![T::default(); width * height] }
    }
}

pub trait Pixel: Copy + Default + Send + Sync {}
impl<T: Copy + Default + Send + Sync> Pixel for T {}

#[rustfmt::skip]
pub fn variance_exact_kernel(sigma: f32) -> Option<Vec<f32>> {
    if !sigma.is_finite() || sigma <= 0.0 || sigma > 33.0 {
        return None;
    }
    let half = (3.0 * sigma).ceil() as i32;
    let target = f64::from(sigma).powi(2);
    let sample = |width: f64| {
        let mut weights: Vec<f64> = (-half..=half).map(|offset| (-f64::from(offset * offset) / (2.0 * width * width)).exp()).collect();
        let sum: f64 = weights.iter().sum();
        weights.iter_mut().for_each(|weight| *weight /= sum);
        weights
    };
    let (mut low, mut high) = (f64::from(sigma) * 0.01, f64::from(sigma) * 2.0 + 1.0);
    for _ in 0..48 {
        let middle = (low + high) * 0.5;
        let variance: f64 = sample(middle).iter().enumerate().map(|(index, weight)| weight * (index as f64 - f64::from(half)).powi(2)).sum();
        if variance < target {
            low = middle;
        } else {
            high = middle;
        }
    }
    Some(sample((low + high) * 0.5).into_iter().map(|weight| weight as f32).collect())
}

pub struct Gaussian {
    sigma: f32,
    kernel: Option<Vec<f32>>,
    feedback: [f64; 3],
    gain: f64,
    boundary: [[f64; 3]; 3],
}

#[rustfmt::skip]
impl Gaussian {
    pub fn new(sigma: f32) -> Self {
        let sigma64 = f64::from(sigma);
        let mut q = if sigma64 < 3.556 { -0.2568 + 0.5784 * sigma64 + 0.0561 * sigma64 * sigma64 } else { 2.5091 + 0.9804 * (sigma64 - 3.556) };
        let (real, pair, imaginary) = (1.16680_f64, 1.10783_f64, 1.40586_f64);
        let norm = pair * pair + imaginary * imaginary;
        let linear = 2.0 * (1.0 / real + 2.0 * pair / norm);
        let quadratic = 2.0 * (1.0 / (real * real) + 2.0 * (pair * pair - imaginary * imaginary) / (norm * norm));
        for _ in 0..5 {
            q -= (q * (quadratic * q + linear) - sigma64 * sigma64) / (2.0 * quadratic * q + linear);
        }
        let denominator = (q + real) * ((q + pair).powi(2) + imaginary * imaginary);
        let feedback = [
            q * ((q + pair).powi(2) + imaginary * imaginary + 2.0 * (q + real) * (q + pair)) / denominator,
            -q * q * (3.0 * q + real + 2.0 * pair) / denominator,
            q.powi(3) / denominator,
        ];
        let gain = real * norm / denominator;
        let [first, second, third] = feedback;
        let scale = 1.0 / ((1.0 + first - second + third) * gain * (1.0 + second + (first - third) * third));
        let boundary = [
            [1.0 - second - third * (first + third), (first + third) * (second + first * third), third * (first + second * third)],
            [first + second * third, (1.0 - second) * (second + first * third), third * (1.0 - second - third * (first + third))],
            [
                first * (first + third) + second * (1.0 - second),
                first * (second - third * third) + third * (1.0 + second * (second - 1.0) - third * third),
                third * (first + second * third),
            ],
        ]
        .map(|row| row.map(|value| value * scale * gain));
        Self { sigma, kernel: (sigma < 2.5).then(|| variance_exact_kernel(sigma)).flatten(), feedback, gain, boundary }
    }

    pub fn filter<const CHANNELS: usize, T: Copy>(
        &self,
        line: &mut [T],
        widen: impl Fn(T) -> [f64; CHANNELS],
        narrow: impl Fn([f64; CHANNELS]) -> T,
    ) {
        let Some((&left, &right)) = line.first().zip(line.last()) else { return };
        if let Some(kernel) = &self.kernel {
            let half = kernel.len() / 2;
            let mut source = vec![widen(left); half];
            source.extend(line.iter().copied().map(&widen));
            source.extend(std::iter::repeat_n(widen(right), half));
            let mut values = vec![[0.0; CHANNELS]; line.len()];
            for (tap, &weight) in kernel.iter().enumerate() {
                for (value, sample) in values.iter_mut().zip(&source[tap..]) {
                    for channel in 0..CHANNELS {
                        value[channel] += sample[channel] * f64::from(weight);
                    }
                }
            }
            for (pixel, value) in line.iter_mut().zip(values) {
                *pixel = narrow(value);
            }
            return;
        }
        let left = widen(left);
        let right = widen(right);
        let mut state = [[0.0; CHANNELS]; 3];
        let mut forward = Vec::with_capacity(line.len());
        for &pixel in line.iter() {
            let input = widen(pixel);
            let value = std::array::from_fn(|channel| {
                self.gain * (input[channel] - left[channel])
                    + self.feedback[0] * state[0][channel]
                    + self.feedback[1] * state[1][channel]
                    + self.feedback[2] * state[2][channel]
            });
            state = [value, state[0], state[1]];
            forward.push(value);
        }
        let mut future: [[f64; CHANNELS]; 3] = self.boundary.map(|row| {
            std::array::from_fn(|channel| {
                row.iter().zip(&state).map(|(coefficient, past)| coefficient * (past[channel] + left[channel] - right[channel])).sum::<f64>()
            })
        });
        if let Some(last) = line.last_mut() {
            *last = narrow(std::array::from_fn(|channel| future[0][channel] + right[channel]));
        }
        let count = line.len().saturating_sub(1);
        for (pixel, input) in line[..count].iter_mut().zip(&forward[..count]).rev() {
            let value = std::array::from_fn(|channel| {
                self.gain * (input[channel] + left[channel] - right[channel])
                    + self.feedback[0] * future[0][channel]
                    + self.feedback[1] * future[1][channel]
                    + self.feedback[2] * future[2][channel]
            });
            *pixel = narrow(std::array::from_fn(|channel| value[channel] + right[channel]));
            future = [value, future[0], future[1]];
        }
    }
}
