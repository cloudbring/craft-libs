//! Conformance of `craft-resample` to the Lean-generated vectors in `tests/vectors/lean-resample.csv`.
//!
//! Each row: ratio `L/M`, a prototype `h` of random small rationals (injected through
//! `Polyphase::from_prototype`, so the Kaiser design is bypassed; the design is tested separately
//! for its per-phase normalisation), a random rational input `x`, and the exact rational output of
//! the direct form `↓M (h * ↑L x)` (cross-checked in Lean against the polyphase form and the
//! accumulator, started at output 0 and at 3).
//!
//! Bound. Rationals are rounded to the working type (relative error `u`: 2^-53 for `f64`, 2^-24 for
//! `f32`), products and the `T`-term sum add at most `T + 1` more roundings, and the reference is
//! rounded once to `f64`, so
//!   |y_hat[n] - y[n]| <= (T + 4) * u * sum_q |h[p + qL]| * |x[i - q]|
//! where `T` is the taps per phase. The worst observed ratio error/bound and the worst absolute
//! error are printed with `--nocapture` and recorded in `lean/TRUST.md`.

use craft_resample::{Polyphase, direct_form, kaiser_prototype};

type TestResult = Result<(), Box<dyn std::error::Error>>;

const TABLE: &str = include_str!("vectors/lean-resample.csv");

struct Row {
    case: String,
    up: usize,
    down: usize,
    taps: usize,
    out_len: usize,
    x: Vec<f64>,
    h: Vec<f64>,
    y: Vec<f64>,
}

fn rational(text: &str) -> Result<f64, Box<dyn std::error::Error>> {
    let (n, d) = text.split_once('/').ok_or("not a fraction")?;
    Ok(n.parse::<f64>()? / d.parse::<f64>()?)
}

fn rationals(text: &str) -> Result<Vec<f64>, Box<dyn std::error::Error>> {
    text.split(' ').filter(|t| !t.is_empty()).map(rational).collect()
}

fn rows() -> Result<Vec<Row>, Box<dyn std::error::Error>> {
    let mut out = Vec::new();
    for line in TABLE.lines().skip(1) {
        let f: Vec<&str> = line.split(',').collect();
        if f.len() != 9 {
            return Err(format!("bad row: {} fields", f.len()).into());
        }
        let row = Row {
            case: f[0].to_string(),
            up: f[1].parse()?,
            down: f[2].parse()?,
            taps: f[3].parse()?,
            out_len: f[5].parse()?,
            x: rationals(f[6])?,
            h: rationals(f[7])?,
            y: rationals(f[8])?,
        };
        if row.h.len() != row.taps || row.x.len() != f[4].parse::<usize>()? || row.y.len() != row.out_len {
            return Err("row lengths disagree with header columns".into());
        }
        out.push(row);
    }
    Ok(out)
}

/// `sum_q |h[p + qL]| |x[i - q]|` for output `n`, the scale of the rounding bound.
fn magnitude(row: &Row, n: usize) -> f64 {
    let start = n * row.down;
    let (i, p) = ((start / row.up) as i64, start % row.up);
    (0..)
        .map(|q| p + q * row.up)
        .take_while(|&k| k < row.h.len())
        .enumerate()
        .map(|(q, k)| {
            let j = i - q as i64;
            let xv = usize::try_from(j).ok().and_then(|j| row.x.get(j)).copied().unwrap_or(0.0);
            (row.h[k] * xv).abs()
        })
        .sum()
}

fn taps_per_phase(row: &Row) -> usize {
    row.taps.div_ceil(row.up)
}

struct Worst {
    ratio: f64,
    abs: f64,
}

fn check(got: &[f64], row: &Row, u: f64, worst: &mut Worst) -> TestResult {
    assert_eq!(got.len(), row.out_len, "{} {}/{}: output length", row.case, row.up, row.down);
    for (n, (&g, &want)) in got.iter().zip(&row.y).enumerate() {
        let bound = (taps_per_phase(row) as f64 + 4.0) * u * magnitude(row, n);
        let err = (g - want).abs();
        worst.abs = worst.abs.max(err);
        if bound > 0.0 {
            worst.ratio = worst.ratio.max(err / bound);
        }
        assert!(err <= bound, "{} {}/{} n={n}: |{g} - {want}| = {err:e} > {bound:e}", row.case, row.up, row.down);
    }
    Ok(())
}

#[test]
fn f64_matches_exact_rational_outputs() -> TestResult {
    let mut worst = Worst { ratio: 0.0, abs: 0.0 };
    let all = rows()?;
    for row in &all {
        let pp = Polyphase::<f64>::from_prototype(&row.h, row.up, row.down)?;
        assert_eq!(pp.out_len(row.x.len()), row.out_len);
        check(&pp.process_all(&row.x), row, f64::EPSILON / 2.0, &mut worst)?;
    }
    println!("f64: {} rows, worst abs error {:e}, worst error/bound {:.3}", all.len(), worst.abs, worst.ratio);
    Ok(())
}

#[test]
fn f32_matches_exact_rational_outputs() -> TestResult {
    let mut worst = Worst { ratio: 0.0, abs: 0.0 };
    let all = rows()?;
    for row in &all {
        let h: Vec<f32> = row.h.iter().map(|&v| v as f32).collect();
        let x: Vec<f32> = row.x.iter().map(|&v| v as f32).collect();
        let pp = Polyphase::<f32>::from_prototype(&h, row.up, row.down)?;
        let got: Vec<f64> = pp.process_all(&x).into_iter().map(f64::from).collect();
        check(&got, row, f64::from(f32::EPSILON) / 2.0, &mut worst)?;
    }
    println!("f32: {} rows, worst abs error {:e}, worst error/bound {:.3}", all.len(), worst.abs, worst.ratio);
    Ok(())
}

/// The literal direct form (builds the zero-stuffed signal) agrees with the polyphase path, and
/// is exactly zero past `out_len` (the Lean theorem `direct_zero_after`).
#[test]
fn direct_form_agrees_and_ends_at_out_len() -> TestResult {
    for row in rows()? {
        let pp = Polyphase::<f64>::from_prototype(&row.h, row.up, row.down)?;
        let direct = direct_form(&row.x, &row.h, row.up, row.down, row.out_len + 3);
        assert!(direct[row.out_len..].iter().all(|&v| v == 0.0), "{} {}/{}: nonzero past out_len", row.case, row.up, row.down);
        let fast = pp.process_all(&row.x);
        for (n, (&a, &b)) in direct.iter().zip(&fast).enumerate() {
            let bound = (taps_per_phase(&row) as f64 + 4.0) * f64::EPSILON * magnitude(&row, n);
            assert!((a - b).abs() <= bound, "{} {}/{} n={n}", row.case, row.up, row.down);
        }
    }
    Ok(())
}

/// Starting a stream at `first` gives the same bits as the slice of the whole run.
#[test]
fn streaming_from_an_offset_is_bit_identical() -> TestResult {
    for row in rows()? {
        let pp = Polyphase::<f64>::from_prototype(&row.h, row.up, row.down)?;
        let all = pp.process_all(&row.x);
        for first in [0, 1, 3, row.out_len / 2] {
            let tail = pp.process(&row.x, first, row.out_len - first);
            assert_eq!(&all[first..], &tail[..], "{} {}/{} first={first}", row.case, row.up, row.down);
        }
    }
    Ok(())
}

/// Per-phase normalised taps (the `dc` rows) give exactly the constant in the interior.
#[test]
fn dc_rows_are_unity_in_the_interior() -> TestResult {
    let mut checked = 0;
    for row in rows()?.iter().filter(|r| r.case == "dc") {
        let t = taps_per_phase(row);
        let pp = Polyphase::<f64>::from_prototype(&row.h, row.up, row.down)?;
        for p in 0..row.up {
            let sum: f64 = pp.phase(p).ok_or("phase")?.iter().sum();
            assert!((sum - 1.0).abs() < 1e-14, "phase {p} sums to {sum}");
        }
        let c = row.x[0];
        for (n, &v) in pp.process_all(&row.x).iter().enumerate() {
            let i = n * row.down / row.up;
            if i + 1 >= t && i < row.x.len() {
                assert!((v - c).abs() <= 1e-14, "{}/{} n={n}: {v} != {c}", row.up, row.down);
                checked += 1;
            }
        }
    }
    assert!(checked > 20, "only {checked} interior outputs checked");
    Ok(())
}

/// The Kaiser design normalises every phase to 1, so the prototype sums to `L` and DC passes.
#[test]
fn kaiser_prototype_is_normalised_per_phase() -> TestResult {
    for (up, down) in [(1, 1), (3, 2), (2, 3), (4, 3), (160, 147), (147, 160), (2, 1), (1, 2)] {
        let h = kaiser_prototype(up, down, 16, 8.0)?;
        let pp = Polyphase::<f64>::from_prototype(&h, up, down)?;
        for p in 0..up {
            let sum: f64 = pp.phase(p).ok_or("phase")?.iter().sum();
            assert!((sum - 1.0).abs() < 1e-12, "{up}/{down} phase {p}: {sum}");
        }
        assert!((h.iter().sum::<f64>() - up as f64).abs() < 1e-9 * up as f64);
        let x = vec![0.75; 400];
        let y = pp.process_all(&x);
        let t = pp.taps_per_phase();
        for (n, &v) in y.iter().enumerate() {
            let i = n * down / up;
            if i + 1 >= t && i < x.len() {
                assert!((v - 0.75).abs() < 1e-12, "{up}/{down} n={n}: {v}");
            }
        }
    }
    Ok(())
}

/// Long run at 160/147: every output of one incremental pass equals a fresh start at that output
/// (closed-form `n*M divmod L`), so the integer accumulator does not drift.
#[test]
fn accumulator_does_not_drift_over_a_long_run() -> TestResult {
    let (up, down) = (160, 147);
    let h = kaiser_prototype(up, down, 4, 8.0)?;
    let pp = Polyphase::<f64>::from_prototype(&h, up, down)?;
    let x: Vec<f64> = (0..280_000u64).map(|i| ((i.wrapping_mul(2_654_435_761) >> 7) % 2001) as f64 / 1000.0 - 1.0).collect();
    let n = pp.out_len(x.len());
    let all = pp.process_all(&x);
    for (j, &v) in all.iter().enumerate() {
        let fresh = pp.process(&x, j, 1);
        assert_eq!(v.to_bits(), fresh[0].to_bits(), "drift at output {j}");
    }
    // far offset: the 64-bit product n*M exceeds u32 and still lands on the same grid
    let far = 4_000_000_000usize;
    let a = pp.process(&x, far, 50);
    for (k, &v) in a.iter().enumerate() {
        assert_eq!(v.to_bits(), pp.process(&x, far + k, 1)[0].to_bits());
    }
    assert!(n > 300_000);
    Ok(())
}

#[test]
fn rejects_degenerate_input() {
    assert!(Polyphase::<f64>::from_prototype(&[1.0], 0, 1).is_err());
    assert!(Polyphase::<f64>::from_prototype(&[1.0], 1, 0).is_err());
    assert!(Polyphase::<f64>::from_prototype(&[], 1, 1).is_err());
    assert!(kaiser_prototype(2, 3, 0, 8.0).is_err());
    let pp = Polyphase::<f64>::from_prototype(&[1.0], 1, 1).ok();
    assert_eq!(pp.map(|p| p.out_len(0)), Some(0));
}
