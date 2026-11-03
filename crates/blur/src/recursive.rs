use crate::Error;

/// Young–van Vliet coefficients with Triggs–Sdika initialization, ported without arithmetic
/// changes from lightcraft 39aa4d25, crates/raster/src/blur.rs::Gaussian::new.
/// Raw YvV does not meet the Recursive shape bound at sigma 2.5 or 4; RecursiveRaw
/// explicitly exposes that range without the sampled guard.
#[derive(Clone, Debug, PartialEq)]
pub struct Recursive {
    pub(crate) sigma: f32,
    pub feedback: [f64; 3],
    pub gain: f64,
    pub boundary: [[f64; 3]; 3],
    decay: [f64; 2],
    amplitude: [f64; 2],
}

impl Recursive {
    pub(crate) fn new(sigma: f32) -> Result<Self, Error> {
        if !sigma.is_finite() || !(2.5..=65_536.0).contains(&sigma) {
            return Err(Error::InvalidSigma);
        }
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
        let real_pole = q / (q + real);
        let pair_denominator = (q + pair).powi(2) + imaginary * imaginary;
        let pair_real = q * (q + pair) / pair_denominator;
        let pair_imaginary = q * imaginary / pair_denominator;
        let pair_radius = pair_real.hypot(pair_imaginary);
        let distance = (real_pole - pair_real).hypot(pair_imaginary);
        let decay = [real_pole, pair_radius];
        let amplitude = [gain * real_pole.powi(2) / distance.powi(2), gain * pair_radius.powi(2) / (distance * pair_imaginary)];
        Ok(Self { sigma, feedback, gain, boundary, decay, amplitude })
    }

    pub(crate) fn region_bound(&self, halo: usize) -> f64 {
        let causal_mass: f64 = self.amplitude.iter().zip(self.decay).map(|(amplitude, decay)| amplitude / (1.0 - decay)).sum();
        let mut tail = 0.0;
        for (amplitude, decay) in self.amplitude.iter().zip(self.decay) {
            let overlap: f64 = self.amplitude.iter().zip(self.decay).map(|(other, radius)| other / (1.0 - decay * radius)).sum();
            tail += 2.0 * amplitude * decay.powf((halo + 1) as f64) / (1.0 - decay) * overlap;
        }
        4.0 * causal_mass.powi(2) * tail
    }
}
