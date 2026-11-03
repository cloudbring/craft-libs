//! CELT low-overlap IMDCT / TDAC adapter (RFC 6716 §4.3.7).
//!
//! craft-fft returns a sine-windowed, 2/M-scaled full block. CELT needs its unscaled
//! middle half, shifted by overlap/2, followed by the Opus low-overlap rotation.
//! Only the middle half is unwindowed, where the sine window is at least sqrt(1/2).
//! Every shift's plan and scratch are owned by the decoder and allocated at construction.

use craft_fft::{Error, TdacWindow};

struct Plan {
    inverse: craft_fft::Imdct<f32>,
    input: Vec<f32>,
    output: Vec<f32>,
    gains: Vec<f32>,
}

impl Plan {
    fn new(coefficients: usize) -> Result<Self, Error> {
        let inverse = craft_fft::Imdct::new(coefficients, TdacWindow::Sine)?;
        let half = coefficients / 2;
        let gains = inverse.window()[half..3 * half].iter().map(|window| half as f32 / window).collect();
        Ok(Self { inverse, input: vec![0.0; coefficients], output: vec![0.0; 2 * coefficients], gains })
    }
}

pub struct ImdctScratch {
    plans: [Plan; 4],
}

impl ImdctScratch {
    pub fn new() -> Result<Self, Error> {
        Ok(Self { plans: [Plan::new(960)?, Plan::new(480)?, Plan::new(240)?, Plan::new(120)?] })
    }

    pub fn backward(&mut self, shift: usize, input: &[f32], stride: usize, output: &mut [f32], window: &[f32], overlap: usize) -> Result<(), Error> {
        let plan = self.plans.get_mut(shift).ok_or(Error::InvalidSize)?;
        let length = plan.input.len();
        let needed = (length - 1).checked_mul(stride).and_then(|last| last.checked_add(1)).ok_or(Error::InvalidSize)?;
        if stride == 0 || overlap > length || !overlap.is_multiple_of(2) {
            return Err(Error::InvalidSize);
        }
        if input.len() < needed {
            return Err(Error::Length { expected: needed, actual: input.len() });
        }
        if output.len() < length + overlap / 2 {
            return Err(Error::Length { expected: length + overlap / 2, actual: output.len() });
        }
        if window.len() < overlap {
            return Err(Error::Length { expected: overlap, actual: window.len() });
        }
        for (index, coefficient) in plan.input.iter_mut().enumerate() {
            *coefficient = input[index * stride];
        }
        plan.inverse.process_into(&plan.input, &mut plan.output)?;
        let half = length / 2;
        for ((sample, transformed), gain) in output[overlap / 2..overlap / 2 + length].iter_mut().zip(&plan.output[half..3 * half]).zip(&plan.gains) {
            *sample = transformed * gain;
        }
        for index in 0..overlap / 2 {
            let left = output[index];
            let right = output[overlap - 1 - index];
            let rising = window[index];
            let falling = window[overlap - 1 - index];
            output[index] = left * falling - right * rising;
            output[overlap - 1 - index] = left * rising + right * falling;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fft_matches_dft() {
        use craft_fft::{Complex, ComplexFft, Direction};
        for length in [15, 30, 60, 120, 240, 480, 960] {
            let mut fft = ComplexFft::new(length).unwrap();
            let input: Vec<_> = (0..length).map(|index| Complex::new(((index * 7) % 13) as f32 - 6.0, ((index * 3) % 5) as f32 - 2.0)).collect();
            let mut output = input.clone();
            fft.process_in_place(Direction::Forward, &mut output).unwrap();
            for bin in [0, 1, 7, length / 2, length - 1] {
                let mut expected = Complex::new(0.0f64, 0.0);
                for (index, value) in input.iter().enumerate() {
                    let phase = -2.0 * std::f64::consts::PI * (index * bin) as f64 / length as f64;
                    expected += Complex::new(value.re as f64, value.im as f64) * Complex::new(phase.cos(), phase.sin());
                }
                assert!((expected.re - output[bin].re as f64).abs() < 1e-2);
                assert!((expected.im - output[bin].im as f64).abs() < 1e-2);
            }
        }
    }

    #[test]
    fn imdct_matches_direct_formula_all_shifts_strides_and_overlap() {
        let mut work = ImdctScratch::new().unwrap();
        let window = super::super::rate::Mode::new().window;
        for (shift, length) in [960, 480, 240, 120].into_iter().enumerate() {
            let spectrum: Vec<_> = (0..length).map(|index| ((index * 37 % 11) as f32 - 5.0) / 5.0).collect();
            let direct: Vec<f64> = (length / 2..3 * length / 2)
                .map(|time| {
                    spectrum
                        .iter()
                        .enumerate()
                        .map(|(bin, value)| {
                            *value as f64 * (std::f64::consts::PI / length as f64 * (time as f64 + 0.5 + length as f64 / 2.0) * (bin as f64 + 0.5)).cos()
                        })
                        .sum()
                })
                .collect();
            for stride in [1, 2, 4, 8] {
                let mut input = vec![1234.0; length * stride];
                for (index, value) in spectrum.iter().enumerate() {
                    input[index * stride] = *value;
                }
                for overlap in [0, 120] {
                    let mut output = vec![0.125; length + overlap / 2 + 3];
                    let mut expected = vec![0.125f64; output.len()];
                    for _ in 0..2 {
                        expected[overlap / 2..overlap / 2 + length].copy_from_slice(&direct);
                        for index in 0..overlap / 2 {
                            let left = expected[index];
                            let right = expected[overlap - 1 - index];
                            expected[index] = left * window[overlap - 1 - index] as f64 - right * window[index] as f64;
                            expected[overlap - 1 - index] = left * window[index] as f64 + right * window[overlap - 1 - index] as f64;
                        }
                        work.backward(shift, &input, stride, &mut output, &window, overlap).unwrap();
                        for (actual, reference) in output.iter().zip(&expected) {
                            assert!((*actual as f64 - reference).abs() < 1e-3, "shift={shift} stride={stride} overlap={overlap}: {actual} != {reference}");
                        }
                        output.copy_within(length..length + overlap / 2, 0);
                        expected.copy_within(length..length + overlap / 2, 0);
                    }
                }
            }
        }
    }

    #[test]
    fn processing_including_first_call_does_not_allocate() {
        let mut work = ImdctScratch::new().unwrap();
        let window = super::super::rate::Mode::new().window;
        let input = [0.25; 960];
        let mut output = [0.0; 1020];
        let allocations = allocation_counter::measure(|| {
            for shift in 0..4 {
                for stride in [1, 1 << shift] {
                    for _ in 0..16 {
                        work.backward(shift, &input, stride, &mut output, &window, 120).unwrap();
                    }
                }
            }
        });
        assert_eq!(allocations.count_total, 0);
    }

    #[test]
    fn invalid_arguments_do_not_panic_or_modify_output() {
        let mut work = ImdctScratch::new().unwrap();
        let mut output = [7.0; 1020];
        for (shift, input_length, stride, output_length, window_length, overlap) in [
            (4, 960, 1, 1020, 120, 120),
            (0, 960, 0, 1020, 120, 120),
            (0, 959, 1, 1020, 120, 120),
            (0, 960, usize::MAX, 1020, 120, 120),
            (0, 960, 1, 1019, 120, 120),
            (0, 960, 1, 1020, 119, 120),
            (0, 960, 1, 1020, 120, 119),
            (0, 960, 1, 1020, 120, usize::MAX),
        ] {
            assert!(work.backward(shift, &[0.0; 960][..input_length], stride, &mut output[..output_length], &[0.0; 120][..window_length], overlap).is_err());
            assert_eq!(output, [7.0; 1020]);
        }
    }
}
