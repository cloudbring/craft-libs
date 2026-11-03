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
        Ok(Self { sigma, feedback, gain, boundary })
    }
}
