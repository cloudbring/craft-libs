//! Scalar comparison: relative/absolute tolerance and ULP distance.

pub use crate::verdict::Tolerance;

/// Number of representable `f32` values between `a` and `b` (0 for equal, including `+0 == -0`).
/// `None` if either is NaN.
pub fn ulp_distance_f32(a: f32, b: f32) -> Option<u32> {
    if a.is_nan() || b.is_nan() {
        return None;
    }
    // Map the sign-magnitude float bits onto a monotone unsigned line.
    let key = |x: f32| {
        let bits = x.to_bits();
        let mag = bits & 0x7FFF_FFFF;
        if bits >> 31 == 1 { 0x8000_0000 - mag } else { 0x8000_0000 + mag }
    };
    Some(key(a).abs_diff(key(b)))
}

/// `f64` counterpart of [`ulp_distance_f32`].
pub fn ulp_distance_f64(a: f64, b: f64) -> Option<u64> {
    if a.is_nan() || b.is_nan() {
        return None;
    }
    let key = |x: f64| {
        let bits = x.to_bits();
        let mag = bits & 0x7FFF_FFFF_FFFF_FFFF;
        if bits >> 63 == 1 { 0x8000_0000_0000_0000 - mag } else { 0x8000_0000_0000_0000 + mag }
    };
    Some(key(a).abs_diff(key(b)))
}

/// `|a-b|` and `|a-b| / max(|a|,|b|)` (0 when both are zero).
pub fn abs_rel_error(a: f64, b: f64) -> (f64, f64) {
    let abs = (a - b).abs();
    let scale = a.abs().max(b.abs());
    (abs, if scale == 0.0 { 0.0 } else { abs / scale })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adjacent_floats_are_one_ulp() {
        let x = 1.0f32;
        let next = f32::from_bits(x.to_bits() + 1);
        assert_eq!(ulp_distance_f32(x, next), Some(1));
        assert_eq!(ulp_distance_f32(next, x), Some(1));
        assert_eq!(ulp_distance_f32(x, x), Some(0));
        let y = 1.0f64;
        assert_eq!(ulp_distance_f64(y, f64::from_bits(y.to_bits() + 1)), Some(1));
        assert_eq!(ulp_distance_f64(y, f64::from_bits(y.to_bits() + 7)), Some(7));
    }

    #[test]
    fn across_zero_and_nan() {
        assert_eq!(ulp_distance_f32(0.0, -0.0), Some(0));
        let tiny = f32::from_bits(1);
        assert_eq!(ulp_distance_f32(tiny, -tiny), Some(2));
        assert_eq!(ulp_distance_f64(f64::from_bits(1), -f64::from_bits(1)), Some(2));
        assert_eq!(ulp_distance_f32(f32::NAN, 1.0), None);
        assert_eq!(ulp_distance_f64(1.0, f64::NAN), None);
        assert_eq!(ulp_distance_f32(f32::MAX, f32::INFINITY), Some(1));
    }

    #[test]
    fn abs_rel() {
        assert_eq!(abs_rel_error(0.0, 0.0), (0.0, 0.0));
        let (a, r) = abs_rel_error(100.0, 101.0);
        assert_eq!(a, 1.0);
        assert!((r - 1.0 / 101.0).abs() < 1e-15);
    }
}
