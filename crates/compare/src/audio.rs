//! Audio comparison over interleaved `f32` samples.

use crate::CompareError;

fn check(a: &[f32], b: &[f32]) -> Result<(), CompareError> {
    if a.len() == b.len() { Ok(()) } else { Err(CompareError::Length { a: a.len(), b: b.len() }) }
}

/// Signal-to-noise ratio in dB of `test` against `reference`:
/// `10 log10(sum(ref^2) / sum((test-ref)^2))`. `+inf` for identical input; `-inf` for a silent
/// reference with nonzero error.
pub fn snr_db(reference: &[f32], test: &[f32]) -> Result<f64, CompareError> {
    check(reference, test)?;
    let (mut signal, mut noise) = (0.0f64, 0.0f64);
    for (r, t) in reference.iter().zip(test) {
        let (r, t) = (f64::from(*r), f64::from(*t));
        signal += r * r;
        noise += (t - r) * (t - r);
    }
    Ok(if noise == 0.0 { f64::INFINITY } else { 10.0 * (signal / noise).log10() })
}

/// Largest absolute per-sample difference.
pub fn max_abs_error(a: &[f32], b: &[f32]) -> Result<f64, CompareError> {
    check(a, b)?;
    Ok(a.iter().zip(b).map(|(x, y)| (f64::from(*x) - f64::from(*y)).abs()).fold(0.0, f64::max))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snr_known_values() {
        let reference = [1.0f32, -1.0, 1.0, -1.0];
        // Noise of 0.1 on every sample: power ratio 1 / 0.01 = 100 -> 20 dB.
        let noisy = [1.1f32, -0.9, 1.1, -0.9];
        let snr = snr_db(&reference, &noisy).unwrap();
        assert!((snr - 20.0).abs() < 1e-4, "{snr}");
        // Half amplitude: error 0.5, ratio 4 -> 6.0206 dB.
        let half = [0.5f32, -0.5, 0.5, -0.5];
        assert!((snr_db(&reference, &half).unwrap() - 6.0206).abs() < 1e-3);
    }

    #[test]
    fn identical_and_silent() {
        let s = [0.25f32, 0.5];
        assert_eq!(snr_db(&s, &s).unwrap(), f64::INFINITY);
        assert_eq!(snr_db(&[0.0, 0.0], &[0.1, 0.0]).unwrap(), f64::NEG_INFINITY);
    }

    #[test]
    fn max_error_and_length() {
        assert!((max_abs_error(&[0.0, 0.5, 1.0], &[0.0, 0.25, 0.5]).unwrap() - 0.5).abs() < 1e-12);
        assert_eq!(max_abs_error(&[], &[]).unwrap(), 0.0);
        assert_eq!(snr_db(&[0.0], &[0.0, 1.0]), Err(CompareError::Length { a: 1, b: 2 }));
        assert!(max_abs_error(&[0.0], &[]).is_err());
    }
}
