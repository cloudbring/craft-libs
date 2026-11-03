#![forbid(unsafe_code)]
#![allow(clippy::needless_range_loop)]

use craft_fft::{Complex, ComplexFft, Direction};
use std::{hint::black_box, time::Instant};

/// Verbatim from gridcraft/crates/engine/src/cmd/analysis.rs:1312,
/// commit 85e51e009c24fad47575f04d3c08f12342615d82.
#[rustfmt::skip]
fn fft(a: &mut [(f64, f64)], inverse: bool) {
    let n = a.len();
    if n < 2 || !n.is_power_of_two() {
        return;
    }
    let bits = n.trailing_zeros();
    for i in 0..n {
        let j = i.reverse_bits() >> (usize::BITS - bits);
        if i < j {
            a.swap(i, j);
        }
    }
    let sign = if inverse { 1.0 } else { -1.0 };
    let mut len = 2;
    while len <= n {
        let ang = sign * 2.0 * std::f64::consts::PI / len as f64;
        for start in (0..n).step_by(len) {
            for k in 0..len / 2 {
                // Each twiddle factor is computed directly, not by repeated multiplication.
                let (wr, wi) = ((ang * k as f64).cos(), (ang * k as f64).sin());
                let (i, j) = (start + k, start + k + len / 2);
                let (Some(&(ur, ui)), Some(&(xr, xi))) = (a.get(i), a.get(j)) else { continue };
                let (vr, vi) = (xr * wr - xi * wi, xr * wi + xi * wr);
                if let Some(u) = a.get_mut(i) {
                    *u = (ur + vr, ui + vi);
                }
                if let Some(x) = a.get_mut(j) {
                    *x = (ur - vr, ui - vi);
                }
            }
        }
        len <<= 1;
    }
    if inverse {
        let nf = n as f64;
        a.iter_mut().for_each(|v| *v = (v.0 / nf, v.1 / nf));
    }
}

#[test]
fn gridcraft_differential_forward_and_inverse() -> Result<(), craft_fft::Error> {
    for exponent in 0..=13 {
        let length = 1 << exponent;
        let input: Vec<_> = (0..length).map(|index| ((index as f64 * 0.13).sin(), (index as f64 * 0.37).cos())).collect();
        let mut plan = ComplexFft::new(length)?;
        for (direction, inverse) in [(Direction::Forward, false), (Direction::Inverse, true)] {
            let mut baseline = input.clone();
            let mut output: Vec<_> = input.iter().map(|&(real, imaginary)| Complex::new(real, imaginary)).collect();
            fft(&mut baseline, inverse);
            plan.process_in_place(direction, &mut output)?;
            for (actual, expected) in output.iter().zip(&baseline) {
                assert!((actual.re - expected.0).abs() < 1e-9 * length as f64);
                assert!((actual.im - expected.1).abs() < 1e-9 * length as f64);
            }
        }
    }
    Ok(())
}

#[test]
#[ignore = "manual release-mode timing; no performance threshold"]
fn timing_against_gridcraft_radix2() -> Result<(), craft_fft::Error> {
    let iterations = 200;
    for length in [128, 1024, 8192] {
        let original: Vec<_> = (0..length).map(|index| ((index as f64 * 0.13).sin(), (index as f64 * 0.37).cos())).collect();
        let complex: Vec<_> = original.iter().map(|&(real, imaginary)| Complex::new(real, imaginary)).collect();
        let mut baseline = original.clone();
        let mut buffer = complex.clone();
        let mut plan = ComplexFft::new(length)?;
        fft(&mut baseline, false);
        plan.process_in_place(Direction::Forward, &mut buffer)?;
        let start = Instant::now();
        for _ in 0..iterations {
            baseline.copy_from_slice(&original);
            fft(black_box(&mut baseline), false);
            black_box(&baseline);
        }
        let gridcraft = start.elapsed();
        let start = Instant::now();
        for _ in 0..iterations {
            buffer.copy_from_slice(&complex);
            plan.process_in_place(Direction::Forward, black_box(&mut buffer))?;
            black_box(&buffer);
        }
        let craft = start.elapsed();
        eprintln!(
            "N={length}: gridcraft={:.3}us craft-fft={:.3}us speedup={:.2}x ({iterations} iterations)",
            gridcraft.as_secs_f64() * 1e6 / iterations as f64,
            craft.as_secs_f64() * 1e6 / iterations as f64,
            gridcraft.as_secs_f64() / craft.as_secs_f64()
        );
    }
    Ok(())
}
