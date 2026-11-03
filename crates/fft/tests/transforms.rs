#![forbid(unsafe_code)]

use craft_fft::{Complex, ComplexFft, Direction, Error, Imdct, Mdct, RealFft, Sample, Stft, TdacWindow, kbd_window, sine_window};
use proptest::prelude::*;
use rustdct::{DctPlanner, RequiredScratch, mdct::Mdct as ReferenceMdct};
use std::f64::consts::PI;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn signal<T: Sample>(length: usize) -> Vec<T> {
    (0..length).map(|index| T::scalar((index as f64 * 0.37).sin() + (index as f64 * 0.19).cos() * 0.3)).collect()
}

fn close(actual: f64, expected: f64, tolerance: f64) {
    assert!((actual - expected).abs() <= tolerance, "actual={actual} expected={expected} tolerance={tolerance}");
}

fn direct_dft(input: &[Complex<f64>]) -> Vec<Complex<f64>> {
    (0..input.len())
        .map(|bin| {
            input.iter().enumerate().fold(Complex::new(0.0, 0.0), |sum, (index, value)| {
                let angle = -2.0 * PI * index as f64 * bin as f64 / input.len() as f64;
                let (sine, cosine) = angle.sin_cos();
                sum + *value * Complex::new(cosine, sine)
            })
        })
        .collect()
}

fn compare_dft(length: usize) -> TestResult {
    let input: Vec<_> = (0..length).map(|index| Complex::new(((index * 7) % 13) as f64 - 6.0, ((index * 3) % 5) as f64 - 2.0)).collect();
    let expected = direct_dft(&input);
    let mut double = vec![Complex::new(0.0, 0.0); length];
    let mut single: Vec<_> = input.iter().map(|value| Complex::new(value.re as f32, value.im as f32)).collect();
    ComplexFft::new(length)?.process_into(Direction::Forward, &input, &mut double)?;
    ComplexFft::new(length)?.process_in_place(Direction::Forward, &mut single)?;
    for ((double, single), expected) in double.iter().zip(&single).zip(&expected) {
        close(double.re, expected.re, 1e-9 * length as f64);
        close(double.im, expected.im, 1e-9 * length as f64);
        close(single.re as f64, expected.re, 1e-4 * length as f64);
        close(single.im as f64, expected.im, 1e-4 * length as f64);
    }
    Ok(())
}

#[test]
fn fft_matches_dft_every_size_1_through_64() -> TestResult {
    for length in 1..=64 {
        compare_dft(length)?;
    }
    Ok(())
}

#[test]
fn fft_matches_dft_powers_of_two_through_8192() -> TestResult {
    for exponent in 7..=13 {
        compare_dft(1 << exponent)?;
    }
    Ok(())
}

fn round_trip_parseval<T: Sample>(length: usize, tolerance: f64) -> TestResult {
    let input: Vec<_> =
        signal::<T>(length).into_iter().enumerate().map(|(index, value)| Complex::new(value, T::scalar((index as f64 * 0.91).cos()))).collect();
    let mut plan = ComplexFft::new(length)?;
    let mut output = input.clone();
    plan.process_in_place(Direction::Forward, &mut output)?;
    let energy = |buffer: &[Complex<T>]| buffer.iter().map(|value| value.re.as_f64().powi(2) + value.im.as_f64().powi(2)).sum::<f64>();
    close(energy(&output) / length as f64, energy(&input), tolerance * length as f64);
    plan.process_in_place(Direction::Inverse, &mut output)?;
    for (actual, expected) in output.iter().zip(&input) {
        close(actual.re.as_f64(), expected.re.as_f64(), tolerance);
        close(actual.im.as_f64(), expected.im.as_f64(), tolerance);
    }
    plan.process_into(Direction::Inverse, &input, &mut output)?;
    plan.process_in_place(Direction::Forward, &mut output)?;
    for (actual, expected) in output.iter().zip(&input) {
        close(actual.re.as_f64(), expected.re.as_f64(), tolerance);
        close(actual.im.as_f64(), expected.im.as_f64(), tolerance);
    }
    Ok(())
}

#[test]
fn fft_parseval_and_both_round_trip_orders() -> TestResult {
    for length in (1..=64).chain((7..=13).map(|exponent| 1 << exponent)) {
        round_trip_parseval::<f32>(length, 1e-4)?;
        round_trip_parseval::<f64>(length, 1e-9)?;
    }
    Ok(())
}

fn real_matches_complex<T: Sample>(length: usize, tolerance: f64) -> TestResult {
    let input = signal::<T>(length);
    let mut real = RealFft::new(length)?;
    assert_eq!(real.len(), length);
    assert!(!real.is_empty());
    let mut spectrum = vec![Complex::new(T::scalar(0.0), T::scalar(0.0)); real.spectrum_len()];
    real.forward_into(&input, &mut spectrum)?;
    let mut complex: Vec<_> = input.iter().map(|value| Complex::new(*value, T::scalar(0.0))).collect();
    ComplexFft::new(length)?.process_in_place(Direction::Forward, &mut complex)?;
    for (real, complex) in spectrum.iter().zip(&complex) {
        close(real.re.as_f64(), complex.re.as_f64(), tolerance * length as f64);
        close(real.im.as_f64(), complex.im.as_f64(), tolerance * length as f64);
    }
    let mut restored = vec![T::scalar(0.0); length];
    real.inverse_into(&spectrum, &mut restored)?;
    for (actual, expected) in restored.iter().zip(&input) {
        close(actual.as_f64(), expected.as_f64(), tolerance);
    }
    real.time_mut().copy_from_slice(&input);
    real.process_in_place(Direction::Forward)?;
    assert_eq!(real.spectrum(), spectrum);
    real.spectrum_mut().copy_from_slice(&spectrum);
    real.process_in_place(Direction::Inverse)?;
    for (actual, expected) in real.time().iter().zip(&input) {
        close(actual.as_f64(), expected.as_f64(), tolerance);
    }
    Ok(())
}

#[test]
fn real_fft_matches_complex_even_odd_and_singleton() -> TestResult {
    for length in (1..=64).chain((7..=13).map(|exponent| 1 << exponent)) {
        real_matches_complex::<f32>(length, 1e-4)?;
        real_matches_complex::<f64>(length, 1e-9)?;
    }
    Ok(())
}

/// Vectors copied from filmcraft/crates/opus/src/celt/mdct.rs::tests::fft_matches_dft
/// at eb59ec09daaea519c9c8fffa404a594ae50d00e0; original 1e-2 bound retained.
#[test]
fn opus_fft_matches_dft_vectors() -> TestResult {
    for &length in &[15usize, 30, 60, 120, 240, 480, 960] {
        let mut values: Vec<_> = (0..length).map(|index| Complex::new(((index * 7) % 13) as f32 - 6.0, ((index * 3) % 5) as f32 - 2.0)).collect();
        let reference: Vec<_> = values.iter().map(|value| Complex::new(value.re as f64, value.im as f64)).collect();
        ComplexFft::new(length)?.process_in_place(Direction::Forward, &mut values)?;
        let expected = direct_dft(&reference);
        for bin in [0, 1, 7, length / 2, length - 1] {
            close(values[bin].re as f64, expected[bin].re, 1e-2);
            close(values[bin].im as f64, expected[bin].im, 1e-2);
        }
    }
    Ok(())
}

fn direct_mdct(input: &[f64], window: &[f64]) -> Vec<f64> {
    let coefficients = input.len() / 2;
    (0..coefficients)
        .map(|bin| {
            input
                .iter()
                .zip(window)
                .enumerate()
                .map(|(index, (value, weight))| {
                    value * weight * (PI / coefficients as f64 * (index as f64 + 0.5 + coefficients as f64 / 2.0) * (bin as f64 + 0.5)).cos()
                })
                .sum()
        })
        .collect()
}

fn direct_imdct(input: &[f64], window: &[f64]) -> Vec<f64> {
    window
        .iter()
        .enumerate()
        .map(|(index, weight)| {
            input
                .iter()
                .enumerate()
                .map(|(bin, value)| value * (PI / input.len() as f64 * (index as f64 + 0.5 + input.len() as f64 / 2.0) * (bin as f64 + 0.5)).cos())
                .sum::<f64>()
                * weight
                * 2.0
                / input.len() as f64
        })
        .collect()
}

fn mdct_references<T: Sample + rustdct::DctNum>(length: usize, window: TdacWindow, tolerance: f64) -> TestResult {
    let mut forward = Mdct::<T>::new(length, window)?;
    let mut inverse = Imdct::<T>::new(length, window)?;
    let input = signal::<T>(2 * length);
    let mut output = vec![T::scalar(0.0); length];
    forward.process_into(&input, &mut output)?;
    let double_input: Vec<_> = input.iter().map(|value| value.as_f64()).collect();
    let double_window: Vec<_> = forward.window().iter().map(|value| value.as_f64()).collect();
    let expected = direct_mdct(&double_input, &double_window);
    for (actual, expected) in output.iter().zip(&expected) {
        close(actual.as_f64(), *expected, tolerance * length as f64);
    }
    let mut planner = DctPlanner::new();
    let oracle = rustdct::mdct::MdctViaDct4::new(planner.plan_dct4(length), |_| forward.window().to_vec());
    let mut oracle_output = vec![T::scalar(0.0); length];
    let mut oracle_scratch = vec![T::scalar(0.0); oracle.get_scratch_len()];
    oracle.process_mdct_with_scratch(&input[..length], &input[length..], &mut oracle_output, &mut oracle_scratch);
    for (actual, expected) in output.iter().zip(oracle_output) {
        close(actual.as_f64(), expected.as_f64(), tolerance * length as f64);
    }
    let coefficients = signal::<T>(length);
    let mut restored = vec![T::scalar(0.0); 2 * length];
    inverse.process_into(&coefficients, &mut restored)?;
    let double_coefficients: Vec<_> = coefficients.iter().map(|value| value.as_f64()).collect();
    for (actual, expected) in restored.iter().zip(direct_imdct(&double_coefficients, &double_window)) {
        close(actual.as_f64(), expected, tolerance);
    }
    assert_eq!(forward.window(), inverse.window());
    Ok(())
}

#[test]
fn mdct_and_imdct_match_cosine_spec_and_rustdct() -> TestResult {
    for length in [2, 4, 6, 10, 120, 128, 240, 480, 960, 1024] {
        for window in [TdacWindow::Sine, TdacWindow::Kbd { alpha: 4.0 }, TdacWindow::Kbd { alpha: 6.0 }] {
            mdct_references::<f32>(length, window, 1e-5)?;
            mdct_references::<f64>(length, window, 1e-10)?;
        }
    }
    Ok(())
}

fn tdac<T: Sample>(length: usize, window: TdacWindow, tolerance: f64) -> TestResult {
    let mut forward = Mdct::<T>::new(length, window)?;
    let mut inverse = Imdct::<T>::new(length, window)?;
    let mut input = vec![T::scalar(0.0); 7 * length];
    input[length..6 * length].copy_from_slice(&signal::<T>(5 * length));
    let mut restored = vec![T::scalar(0.0); input.len()];
    let mut coefficients = vec![T::scalar(0.0); length];
    let mut block = vec![T::scalar(0.0); 2 * length];
    for offset in (0..=input.len() - 2 * length).step_by(length) {
        forward.process_into(&input[offset..offset + 2 * length], &mut coefficients)?;
        inverse.process_into(&coefficients, &mut block)?;
        for (sum, value) in restored[offset..offset + 2 * length].iter_mut().zip(&block) {
            *sum = *sum + *value;
        }
    }
    for (actual, expected) in restored.iter().zip(&input) {
        close(actual.as_f64(), expected.as_f64(), tolerance);
    }
    Ok(())
}

#[test]
fn mdct_tdac_perfect_reconstruction_codec_sizes() -> TestResult {
    for length in [2, 6, 120, 128, 240, 480, 960, 1024] {
        for window in [TdacWindow::Sine, TdacWindow::Kbd { alpha: 4.0 }, TdacWindow::Kbd { alpha: 6.0 }] {
            tdac::<f32>(length, window, 1e-5)?;
            tdac::<f64>(length, window, 1e-10)?;
        }
    }
    Ok(())
}

#[test]
fn windows_are_symmetric_and_princen_bradley() -> TestResult {
    for length in [4, 16, 240, 256, 1920, 2048] {
        for window in [sine_window::<f64>(length)?, kbd_window(length, 0.0)?, kbd_window(length, 4.0)?, kbd_window(length, 6.0)?] {
            for index in 0..length / 2 {
                close(window[index], window[length - index - 1], 1e-14);
                close(window[index].powi(2) + window[index + length / 2].powi(2), 1.0, 1e-14);
            }
        }
    }
    Ok(())
}

fn stft_identity<T: Sample>(length: usize, hop: usize, tolerance: f64, hann: bool) -> TestResult {
    let window = if hann {
        (0..length).map(|index| T::scalar(0.5 - 0.5 * (2.0 * PI * index as f64 / (length - 1) as f64).cos())).collect()
    } else {
        sine_window::<T>(length)?
    };
    let mut stft = Stft::new(&window, hop)?;
    assert_eq!(stft.hop(), hop);
    let mut input = vec![T::scalar(0.0); length * 7];
    input[length..length * 6].copy_from_slice(&signal::<T>(length * 5));
    let mut reconstructed = Vec::new();
    let mut spectrum = vec![Complex::new(T::scalar(0.0), T::scalar(0.0)); stft.spectrum_len()];
    let mut block = vec![T::scalar(0.0); hop];
    for offset in (0..=input.len() - length).step_by(hop) {
        stft.analyze_into(&input[offset..offset + length], &mut spectrum)?;
        stft.synthesize_into(&spectrum, &mut block)?;
        reconstructed.extend_from_slice(&block);
    }
    loop {
        let count = stft.flush_into(&mut block)?;
        if count == 0 {
            break;
        }
        reconstructed.extend_from_slice(&block[..count]);
    }
    assert!(reconstructed.len() >= 6 * length);
    for (actual, expected) in reconstructed.iter().zip(&input) {
        close(actual.as_f64(), expected.as_f64(), tolerance);
    }
    stft.analyze_into(&input[..length], &mut spectrum)?;
    stft.synthesize_into(&spectrum, &mut block)?;
    stft.reset();
    assert_eq!(stft.flush_into(&mut block)?, 0);
    assert!(block.iter().all(|value| *value == T::scalar(0.0)));
    Ok(())
}

#[test]
fn stft_identity_overlap_add_and_tail() -> TestResult {
    for (length, hop) in [(1, 1), (31, 7), (64, 16), (120, 60), (128, 128), (1024, 256)] {
        stft_identity::<f32>(length, hop, 1e-4, false)?;
        stft_identity::<f64>(length, hop, 1e-9, false)?;
    }
    stft_identity::<f32>(128, 32, 1e-4, true)?;
    stft_identity::<f64>(128, 32, 1e-9, true)?;
    Ok(())
}

#[test]
fn invalid_configuration_returns_errors() -> TestResult {
    for length in [0, usize::MAX] {
        assert!(matches!(ComplexFft::<f64>::new(length), Err(Error::InvalidSize)));
        assert!(matches!(RealFft::<f64>::new(length), Err(Error::InvalidSize)));
        assert!(matches!(Mdct::<f64>::new(length, TdacWindow::Sine), Err(Error::InvalidSize)));
        assert!(matches!(Imdct::<f64>::new(length, TdacWindow::Sine), Err(Error::InvalidSize)));
    }
    assert!(matches!(Mdct::<f32>::new(3, TdacWindow::Sine), Err(Error::InvalidSize)));
    for alpha in [f64::NAN, f64::INFINITY, -1.0, 65.0] {
        assert_eq!(kbd_window::<f64>(16, alpha), Err(Error::InvalidWindow));
    }
    assert_eq!(kbd_window::<f64>(3, 4.0), Err(Error::InvalidWindow));
    for hop in [0, 5] {
        assert!(matches!(Stft::<f64>::new(&[1.0; 4], hop), Err(Error::InvalidHop)));
    }
    for window in [[0.0, 1.0, 0.0, 1.0], [f64::NAN, 1.0, 1.0, 1.0], [f64::MAX; 4]] {
        assert!(matches!(Stft::new(&window, 2), Err(Error::InvalidWindow)));
    }
    Ok(())
}

#[test]
fn invalid_buffers_return_errors_without_mutating_output() -> TestResult {
    let mut complex = ComplexFft::<f64>::new(4)?;
    assert!(!complex.is_empty());
    let mut output = [Complex::new(42.0, 0.0); 4];
    assert!(complex.process_into(Direction::Forward, &[], &mut output).is_err());
    assert!(complex.process_in_place(Direction::Inverse, &mut []).is_err());
    assert_eq!(output, [Complex::new(42.0, 0.0); 4]);
    let mut real = RealFft::<f64>::new(4)?;
    let mut time = [42.0; 4];
    assert_eq!(real.inverse_into(&[Complex::new(0.0, 1.0); 3], &mut time), Err(Error::InvalidSpectrum));
    assert_eq!(time, [42.0; 4]);
    assert!(real.forward_into(&[], &mut output).is_err());
    assert!(real.inverse_into(&[], &mut time).is_err());
    let mut mdct = Mdct::<f64>::new(4, TdacWindow::Sine)?;
    assert!(mdct.process_into(&[], &mut time).is_err());
    assert!(mdct.process_into(&[0.0; 8], &mut []).is_err());
    let mut imdct = Imdct::<f64>::new(4, TdacWindow::Sine)?;
    assert!(imdct.process_into(&[], &mut time).is_err());
    assert!(imdct.process_into(&[0.0; 4], &mut []).is_err());
    let mut stft = Stft::<f64>::new(&[1.0; 4], 2)?;
    assert!(stft.analyze_into(&[], &mut output).is_err());
    assert!(stft.synthesize_into(&[], &mut []).is_err());
    assert!(stft.flush_into(&mut []).is_err());
    assert_eq!(time, [42.0; 4]);
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn random_sizes_fft_and_real_round_trip(length in 1usize..257) {
        prop_assert!(round_trip_parseval::<f32>(length, 1e-4).is_ok());
        prop_assert!(round_trip_parseval::<f64>(length, 1e-9).is_ok());
        prop_assert!(real_matches_complex::<f32>(length, 1e-4).is_ok());
        prop_assert!(real_matches_complex::<f64>(length, 1e-9).is_ok());
    }

    #[test]
    fn random_fft_values_match_dft(values in prop::collection::vec((-8.0f64..8.0, -8.0f64..8.0), 1..65)) {
        let input: Vec<_> = values.iter().map(|&(real, imaginary)| Complex::new(real, imaginary)).collect();
        let mut output = input.clone();
        let plan = ComplexFft::new(input.len());
        prop_assert!(plan.is_ok());
        if let Ok(mut plan) = plan {
            prop_assert!(plan.process_in_place(Direction::Forward, &mut output).is_ok());
            for (actual, expected) in output.iter().zip(direct_dft(&input)) {
                prop_assert!((actual.re - expected.re).abs() < 1e-9 * input.len() as f64);
                prop_assert!((actual.im - expected.im).abs() < 1e-9 * input.len() as f64);
            }
        }
    }

    #[test]
    fn random_even_mdct_sizes(half in 1usize..129, alpha in 0.0f64..10.0) {
        let window = TdacWindow::Kbd { alpha };
        prop_assert!(tdac::<f32>(2 * half, window, 1e-5).is_ok());
        prop_assert!(tdac::<f64>(2 * half, window, 1e-10).is_ok());
    }

    #[test]
    fn random_stft_sizes_and_hops(length in 1usize..129, fraction in 1usize..129) {
        let hop = 1 + fraction % length;
        prop_assert!(stft_identity::<f32>(length, hop, 1e-4, false).is_ok());
        prop_assert!(stft_identity::<f64>(length, hop, 1e-9, false).is_ok());
    }
}
