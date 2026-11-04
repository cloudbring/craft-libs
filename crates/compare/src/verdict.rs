//! Uniform verdicts over metric values.

/// Outcome of judging one metric (or a set) against a limit.
#[derive(Clone, Debug, PartialEq)]
pub enum Verdict {
    /// Perfect: the metric is at its ideal value (zero error, infinite PSNR, SSIM 1, ...).
    Identical,
    /// Not perfect but inside the limit.
    Within { metric: String, value: f64 },
    /// Outside the limit.
    Exceeds { metric: String, value: f64, limit: f64 },
}

impl Verdict {
    /// True for `Identical` and `Within`.
    pub fn passed(&self) -> bool {
        !matches!(self, Self::Exceeds { .. })
    }

    /// An error-like metric: lower is better, `value == 0` is identical, `value <= limit` passes.
    /// NaN exceeds.
    pub fn at_most(metric: &str, value: f64, limit: f64) -> Self {
        if value == 0.0 {
            Self::Identical
        } else if value <= limit {
            Self::Within { metric: metric.into(), value }
        } else {
            Self::Exceeds { metric: metric.into(), value, limit }
        }
    }

    /// A similarity-like metric: higher is better, `value >= perfect` is identical,
    /// `value >= min` passes. NaN exceeds.
    pub fn at_least(metric: &str, value: f64, min: f64, perfect: f64) -> Self {
        if value >= perfect {
            Self::Identical
        } else if value >= min {
            Self::Within { metric: metric.into(), value }
        } else {
            Self::Exceeds { metric: metric.into(), value, limit: min }
        }
    }

    /// Combine verdicts: the first `Exceeds` wins; otherwise the first `Within`; otherwise `Identical`.
    pub fn combine(verdicts: impl IntoIterator<Item = Verdict>) -> Self {
        let mut best = Self::Identical;
        for v in verdicts {
            match v {
                Self::Exceeds { .. } => return v,
                Self::Within { .. } if best == Self::Identical => best = v,
                _ => {}
            }
        }
        best
    }
}

/// Absolute plus relative tolerance for scalar comparison: `|a-b| <= abs + rel * max(|a|,|b|)`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tolerance {
    pub abs: f64,
    pub rel: f64,
}

impl Tolerance {
    pub const EXACT: Self = Self { abs: 0.0, rel: 0.0 };

    pub const fn new(abs: f64, rel: f64) -> Self {
        Self { abs, rel }
    }

    /// The allowed deviation between `a` and `b`.
    pub fn allowed(&self, a: f64, b: f64) -> f64 {
        self.abs + self.rel * a.abs().max(b.abs())
    }

    pub fn within(&self, a: f64, b: f64) -> bool {
        a == b || (a - b).abs() <= self.allowed(a, b)
    }

    /// Judge two scalars: equal is `Identical`, inside tolerance `Within`, else `Exceeds`
    /// (with `limit` the allowed deviation).
    pub fn verdict(&self, metric: &str, a: f64, b: f64) -> Verdict {
        if a == b {
            return Verdict::Identical;
        }
        let diff = (a - b).abs();
        let limit = self.allowed(a, b);
        if diff <= limit {
            Verdict::Within { metric: metric.into(), value: diff }
        } else {
            Verdict::Exceeds { metric: metric.into(), value: diff, limit }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn at_most_and_at_least() {
        assert_eq!(Verdict::at_most("e", 0.0, 1.0), Verdict::Identical);
        assert!(matches!(Verdict::at_most("e", 0.5, 1.0), Verdict::Within { .. }));
        assert_eq!(Verdict::at_most("e", 2.0, 1.0), Verdict::Exceeds { metric: "e".into(), value: 2.0, limit: 1.0 });
        assert!(!Verdict::at_most("e", f64::NAN, 1.0).passed());
        assert_eq!(Verdict::at_least("ssim", 1.0, 0.99, 1.0), Verdict::Identical);
        assert!(matches!(Verdict::at_least("ssim", 0.995, 0.99, 1.0), Verdict::Within { .. }));
        assert!(!Verdict::at_least("ssim", 0.5, 0.99, 1.0).passed());
        assert_eq!(Verdict::at_least("psnr", f64::INFINITY, 40.0, f64::INFINITY), Verdict::Identical);
    }

    #[test]
    fn combine_prefers_exceeds_then_within() {
        let w = Verdict::at_most("a", 0.1, 1.0);
        let x = Verdict::at_most("b", 3.0, 1.0);
        assert_eq!(Verdict::combine([Verdict::Identical, w.clone()]), w);
        assert_eq!(Verdict::combine([w, x.clone()]), x);
        assert_eq!(Verdict::combine([]), Verdict::Identical);
    }

    #[test]
    fn tolerance_scalar() {
        let t = Tolerance::new(1e-6, 1e-3);
        assert!(t.within(1000.0, 1000.5));
        assert!(!t.within(1000.0, 1002.0));
        assert!(Tolerance::EXACT.within(1.0, 1.0));
        assert_eq!(t.verdict("x", 2.0, 2.0), Verdict::Identical);
        assert!(matches!(t.verdict("x", 1000.0, 1000.5), Verdict::Within { .. }));
        assert!(matches!(t.verdict("x", 1.0, 2.0), Verdict::Exceeds { .. }));
    }
}
