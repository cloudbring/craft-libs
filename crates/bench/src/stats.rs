//! Order statistics and the bootstrap. All functions are deterministic given the seed.

use crate::rng::Rng;

fn sorted(v: &[f64]) -> Vec<f64> {
    let mut s = v.to_vec();
    s.sort_by(f64::total_cmp);
    s
}

fn median_sorted(s: &[f64]) -> Option<f64> {
    let n = s.len();
    if n == 0 {
        return None;
    }
    Some(if n % 2 == 1 { s[n / 2] } else { f64::midpoint(s[n / 2 - 1], s[n / 2]) })
}

/// Median; `None` for an empty slice.
pub fn median(v: &[f64]) -> Option<f64> {
    median_sorted(&sorted(v))
}

/// Raw (unscaled) median absolute deviation from the median.
pub fn mad(v: &[f64]) -> Option<f64> {
    let m = median(v)?;
    let dev: Vec<f64> = v.iter().map(|x| (x - m).abs()).collect();
    median(&dev)
}

/// Linear-interpolation percentile (`p` in 0..=100) of an ascending slice (R type 7).
/// Returns NaN for an empty slice.
pub fn percentile_sorted(s: &[f64], p: f64) -> f64 {
    if s.is_empty() {
        return f64::NAN;
    }
    let rank = (p / 100.0).clamp(0.0, 1.0) * (s.len() - 1) as f64;
    let lo = rank.floor() as usize;
    let hi = rank.ceil() as usize;
    s[lo] + (s[hi] - s[lo]) * (rank - lo as f64)
}

/// 2.5th and 97.5th percentiles.
pub fn ci95(samples: &[f64]) -> (f64, f64) {
    let s = sorted(samples);
    (percentile_sorted(&s, 2.5), percentile_sorted(&s, 97.5))
}

/// Ratio of two medians, each floored at 1 ns. A zero reading (a clock coarser than the work)
/// would otherwise give 0, infinity or NaN, which the geometric mean and JSON cannot hold.
/// Work this short is below timer resolution and should be batched by the caller.
pub fn ratio_of_medians(new: f64, old: f64) -> f64 {
    new.max(1.0) / old.max(1.0)
}

fn resample_median(src: &[f64], buf: &mut Vec<f64>, rng: &mut Rng) -> f64 {
    buf.clear();
    buf.extend((0..src.len()).map(|_| src[rng.below(src.len())]));
    buf.sort_by(f64::total_cmp);
    median_sorted(buf).unwrap_or(f64::NAN)
}

/// Bootstrap distribution of `median(new) / median(old)`: `resamples` ratios, each from
/// resampling both samples with replacement. Empty if either sample is empty.
pub fn bootstrap_ratios(old: &[f64], new: &[f64], resamples: usize, seed: u64) -> Vec<f64> {
    if old.is_empty() || new.is_empty() {
        return Vec::new();
    }
    let mut rng = Rng::new(seed);
    let (mut bo, mut bn) = (Vec::with_capacity(old.len()), Vec::with_capacity(new.len()));
    (0..resamples)
        .map(|_| {
            let o = resample_median(old, &mut bo, &mut rng);
            let n = resample_median(new, &mut bn, &mut rng);
            ratio_of_medians(n, o)
        })
        .collect()
}

/// Geometric mean of positive values; NaN if empty or any value is not positive.
pub fn geomean(v: &[f64]) -> f64 {
    if v.is_empty() || v.iter().any(|x| *x <= 0.0 || x.is_nan()) {
        return f64::NAN;
    }
    (v.iter().map(|x| x.ln()).sum::<f64>() / v.len() as f64).exp()
}

/// 95% CI of the geometric-mean ratio across inputs. `per_input[i]` is input i's bootstrap
/// ratio distribution (all the same length); resample r of the aggregate is the geometric mean
/// of resample r over inputs. Inputs are resampled independently.
pub fn geomean_ci(per_input: &[Vec<f64>]) -> (f64, f64) {
    let n = per_input.iter().map(Vec::len).min().unwrap_or(0);
    if n == 0 {
        return (f64::NAN, f64::NAN);
    }
    let agg: Vec<f64> = (0..n)
        .map(|r| {
            let col: Vec<f64> = per_input.iter().map(|d| d[r]).collect();
            geomean(&col)
        })
        .collect();
    ci95(&agg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn median_and_mad_known_values() {
        assert_eq!(median(&[3.0, 1.0, 2.0]), Some(2.0));
        assert_eq!(median(&[4.0, 1.0, 3.0, 2.0]), Some(2.5));
        assert_eq!(median(&[]), None);
        // median 2; deviations 1,1,0,0,2,4,7 -> sorted 0,0,1,1,2,4,7 -> MAD 1
        assert_eq!(mad(&[1.0, 1.0, 2.0, 2.0, 4.0, 6.0, 9.0]), Some(1.0));
        assert_eq!(mad(&[5.0, 5.0, 5.0]), Some(0.0));
    }

    #[test]
    fn percentile_interpolates() {
        let s = [1.0, 2.0, 3.0, 4.0, 5.0];
        assert_eq!(percentile_sorted(&s, 0.0), 1.0);
        assert_eq!(percentile_sorted(&s, 50.0), 3.0);
        assert_eq!(percentile_sorted(&s, 25.0), 2.0);
        assert_eq!(percentile_sorted(&s, 100.0), 5.0);
        assert!((percentile_sorted(&s, 12.5) - 1.5).abs() < 1e-12);
    }

    #[test]
    fn geomean_known() {
        assert!((geomean(&[2.0, 8.0]) - 4.0).abs() < 1e-12);
        assert!(geomean(&[1.0, 0.0]).is_nan());
        assert!(geomean(&[]).is_nan());
    }

    #[test]
    fn bootstrap_is_deterministic_per_seed() {
        let old: Vec<f64> = (0..12).map(|i| 100.0 + f64::from(i)).collect();
        let new: Vec<f64> = (0..12).map(|i| 50.0 + f64::from(i % 5)).collect();
        let a = bootstrap_ratios(&old, &new, 2000, 42);
        let b = bootstrap_ratios(&old, &new, 2000, 42);
        let c = bootstrap_ratios(&old, &new, 2000, 43);
        assert_eq!(a.len(), 2000);
        assert_eq!(a, b);
        assert_ne!(a, c);
        let (lo, hi) = ci95(&a);
        assert!(lo <= hi && lo > 0.3 && hi < 0.7, "{lo} {hi}");
    }

    #[test]
    fn bootstrap_of_constant_data_collapses_to_the_ratio() {
        let a = bootstrap_ratios(&[100.0; 10], &[50.0; 10], 2000, 1);
        assert_eq!(ci95(&a), (0.5, 0.5));
    }

    #[test]
    fn bootstrap_ci_covers_one_when_there_is_no_difference() {
        let x: Vec<f64> = (0..20).map(|i| 100.0 + f64::from(i % 7)).collect();
        let y: Vec<f64> = (0..20).map(|i| 100.0 + f64::from((i + 3) % 7)).collect();
        let (lo, hi) = ci95(&bootstrap_ratios(&x, &y, 2000, 9));
        assert!(lo <= 1.0 && 1.0 <= hi, "{lo} {hi}");
    }

    #[test]
    fn empty_samples_give_empty_bootstrap() {
        assert!(bootstrap_ratios(&[], &[1.0], 10, 1).is_empty());
        assert!(geomean_ci(&[]).0.is_nan());
    }

    #[test]
    fn geomean_ci_of_constant_inputs() {
        let d = vec![vec![0.5; 100], vec![0.125; 100]];
        let (lo, hi) = geomean_ci(&d);
        assert!((lo - 0.25).abs() < 1e-12 && (hi - 0.25).abs() < 1e-12);
    }
}
