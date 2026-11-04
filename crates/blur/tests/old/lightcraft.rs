//! Vendored from storytold/lightcraft @ 06c1b9ae0775464ce83ef0a9588be770337600f1 (origin/main, 2026-11-02)
//! Source path: crates/raster/src/blur.rs (MAX_SIGMA, box_radii)
//! Functions below the `harness` marker are NOT vendored; everything above it is copied unchanged
//! (comments included) except that some `fn`/`pub(crate) fn` were made `pub fn` so the tests can call them.

/// The largest blur sigma (pixels) the box radii are made for. A box wider than any image (the
/// encoders' limit is 65535 px) blurs to the same flat result, and a larger sigma made the box
/// width's square overflow (from about 23 170 px: a panic where overflow is checked, a garbage
/// radius of billions of pixels, i.e. a hung render, where it isn't).
pub const MAX_SIGMA: f32 = 65_536.0;

/// Box radii approximating a Gaussian of `sigma` with 3 passes (Kovesi / Wells). A sigma that is
/// not a number counts as 0 (no blur); one above [`MAX_SIGMA`] (infinity too) as `MAX_SIGMA`.
pub fn box_radii(sigma: f32) -> [usize; 3] {
    let sigma = if sigma.is_nan() { 0.0 } else { sigma.clamp(0.0, MAX_SIGMA) };
    let n = 3.0f32;
    let w_ideal = (12.0 * sigma * sigma / n + 1.0).sqrt();
    let mut wl = w_ideal.floor() as i32;
    if wl % 2 == 0 {
        wl -= 1;
    }
    let wu = wl + 2;
    // (the square in i64: it passes i32 from a box 46 341 px wide)
    let m_ideal = (12.0 * sigma * sigma - n * (i64::from(wl) * i64::from(wl)) as f32 - 4.0 * n * wl as f32 - 3.0 * n) / (-4.0 * wl as f32 - 4.0);
    let m = m_ideal.round() as i32;
    std::array::from_fn(|i| (((if (i as i32) < m { wl } else { wu }) - 1) / 2).max(0) as usize)
}

// ---- harness (not vendored) ----------------------------------------------------------------
// `gaussian` (same file) returns a copy when `radii_for` is None (sigma <= 0.3), else runs three
// clamped running-mean box passes of the radii `box_radii` gives, along each axis.

/// Impulse response of lightcraft `raster::blur::gaussian` at `sigma`.
pub fn impulse(sigma: f64) -> Vec<f64> {
    let sigma = sigma as f32;
    if sigma <= 0.3 {
        return vec![1.0];
    }
    super::box_cascade(&box_radii(sigma))
}
