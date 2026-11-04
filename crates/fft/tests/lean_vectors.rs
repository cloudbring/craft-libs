//! Conformance of `craft-fft` to the Lean-generated vectors in `tests/vectors/lean-fft.csv`.
//!
//! Complex rows: reference `X = DFT(x)` computed in Lean in 60-digit fixed point and rounded to
//! 20 decimals (absolute error <= 1e-19 per component, far below `f32`/`f64` rounding), inputs
//! exact multiples of 1/8 so they are exact in both `f32` and `f64`.
//!
//! Bound (checked in the Euclidean norm, `u` the unit roundoff, 2^-53 for `f64`, 2^-24 for `f32`):
//!   forward: ||X_hat - X||_2 <= (C * max(log2 N, 1) + 1) * u * ||X||_2
//!   inverse: ||x_hat - x||_2 <= (C * max(log2 N, 1) + 1) * u * ||x||_2   (inverse applied to rounded X)
//! with `C = 2`.  The `+ 1` pays for rounding the reference to working precision.  Radix-2 style
//! FFTs with correctly rounded twiddles satisfy `C ~ 1..3` (Higham, ASNA §24.1); `C = 2` leaves
//! margin for rustfft's mixed radix kernels.  The largest observed ratio is printed with
//! `--nocapture` and recorded in `lean/TRUST.md`.
//!
//! NTT rows: `craft-fft` has no number-theoretic transform, so these rows are NOT checked
//! against craft-fft.  They validate the Lean model itself against an independent `u64` O(N^2)
//! implementation written here.

use craft_fft::{Complex, ComplexFft, Direction, Sample};

type TestResult = Result<(), Box<dyn std::error::Error>>;

const TABLE: &str = include_str!("vectors/lean-fft.csv");
const C: f64 = 2.0;
const P: u64 = 998_244_353;

struct Row<'a> {
    ring: &'a str,
    case: &'a str,
    n: usize,
    cols: [&'a str; 4],
}

fn rows() -> Result<Vec<Row<'static>>, Box<dyn std::error::Error>> {
    let mut out = Vec::new();
    for line in TABLE.lines().skip(1) {
        let f: Vec<&str> = line.split(',').collect();
        if f.len() != 7 {
            return Err(format!("bad row: {} fields", f.len()).into());
        }
        out.push(Row { ring: f[0], case: f[1], n: f[2].parse()?, cols: [f[3], f[4], f[5], f[6]] });
    }
    Ok(out)
}

fn floats(text: &str) -> Result<Vec<f64>, Box<dyn std::error::Error>> {
    text.split(' ').map(|t| t.parse::<f64>().map_err(Into::into)).collect()
}

fn complex<T: Sample>(re: &str, im: &str, n: usize) -> Result<Vec<Complex<T>>, Box<dyn std::error::Error>> {
    let (re, im) = (floats(re)?, floats(im)?);
    if re.len() != n || im.len() != n {
        return Err("length mismatch".into());
    }
    Ok(re.iter().zip(&im).map(|(a, b)| Complex::new(T::scalar(*a), T::scalar(*b))).collect())
}

fn norm<T: Sample>(v: &[Complex<T>]) -> f64 {
    v.iter().map(|c| c.re.as_f64().powi(2) + c.im.as_f64().powi(2)).sum::<f64>().sqrt()
}

fn dist<T: Sample>(a: &[Complex<T>], b: &[Complex<T>]) -> f64 {
    a.iter().zip(b).map(|(p, q)| (p.re.as_f64() - q.re.as_f64()).powi(2) + (p.im.as_f64() - q.im.as_f64()).powi(2)).sum::<f64>().sqrt()
}

fn check<T: Sample>(unit_roundoff: f64) -> Result<f64, Box<dyn std::error::Error>> {
    let mut worst: f64 = 0.0;
    let mut seen = 0;
    let mut sizes = std::collections::BTreeSet::new();
    for row in rows()?.iter().filter(|r| r.ring == "complex") {
        let n = row.n;
        let x = complex::<T>(row.cols[0], row.cols[1], n)?;
        let want = complex::<T>(row.cols[2], row.cols[3], n)?;
        let mut plan = ComplexFft::<T>::new(n)?;
        let scale = (n.ilog2().max(1) as f64) * C + 1.0;

        let mut got = x.clone();
        plan.process_in_place(Direction::Forward, &mut got)?;
        let bound = scale * unit_roundoff * norm(&want);
        let err = dist(&got, &want);
        assert!(err <= bound, "forward {} n={n}: err={err:e} bound={bound:e}", row.case);
        worst = worst.max(err / (unit_roundoff * norm(&want) * n.ilog2().max(1) as f64));

        let mut back = want.clone();
        plan.process_in_place(Direction::Inverse, &mut back)?;
        let bound = scale * unit_roundoff * norm(&x);
        let err = dist(&back, &x);
        assert!(err <= bound, "inverse {} n={n}: err={err:e} bound={bound:e}", row.case);
        worst = worst.max(err / (unit_roundoff * norm(&x) * n.ilog2().max(1) as f64));
        seen += 1;
        sizes.insert(n);
    }
    assert_eq!(seen, 40);
    assert_eq!(sizes.iter().copied().collect::<Vec<_>>(), (1..=12).map(|m| 1usize << m).collect::<Vec<_>>());
    Ok(worst)
}

#[test]
fn f64_matches_lean_dft_within_bound() -> TestResult {
    let worst = check::<f64>(f64::EPSILON / 2.0)?;
    println!("f64 largest observed err / (u * log2 N * ||ref||) = {worst:.3} (C = {C})");
    Ok(())
}

#[test]
fn f32_matches_lean_dft_within_bound() -> TestResult {
    let worst = check::<f32>(f32::EPSILON as f64 / 2.0)?;
    println!("f32 largest observed err / (u * log2 N * ||ref||) = {worst:.3} (C = {C})");
    Ok(())
}

fn mul(a: u64, b: u64) -> u64 {
    (u128::from(a) * u128::from(b) % u128::from(P)) as u64
}

fn pow(mut base: u64, mut exp: u64) -> u64 {
    let mut acc = 1;
    while exp > 0 {
        if exp & 1 == 1 {
            acc = mul(acc, base);
        }
        base = mul(base, base);
        exp >>= 1;
    }
    acc
}

/// craft-fft has no NTT: this validates the Lean model against an independent `u64` NTT.
#[test]
fn ntt_vectors_match_independent_naive_ntt() -> TestResult {
    let mut seen = 0;
    for row in rows()?.iter().filter(|r| r.ring == "ntt") {
        let parse =
            |t: &str| -> Result<Vec<u64>, Box<dyn std::error::Error>> { t.split(' ').map(|s| s.parse::<u64>().map_err(Into::into)).collect() };
        let (x, want) = (parse(row.cols[0])?, parse(row.cols[2])?);
        assert_eq!((x.len(), want.len()), (row.n, row.n));
        assert!(row.cols[1].is_empty() && row.cols[3].is_empty());
        let w = pow(3, (P - 1) / row.n as u64);
        assert_eq!(pow(w, row.n as u64 / 2), P - 1, "w^(N/2) = -1");
        for (k, expected) in want.iter().enumerate() {
            let sum = x.iter().enumerate().fold(0, |acc, (j, v)| (acc + mul(*v % P, pow(w, (j * k) as u64))) % P);
            assert_eq!(sum, *expected, "{} n={} k={k}", row.case, row.n);
        }
        seen += 1;
    }
    assert_eq!(seen, 16);
    Ok(())
}
