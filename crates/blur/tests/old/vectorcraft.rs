//! Vendored from storytold/vectorcraft @ 11862f61198c8f62c0b22b6c9a331cc5950ac98d (origin/main, 2026-11-02)
//! Source path: crates/effects/src/pixel.rs (MAX_BOX, gauss_boxes, gaussian's sigma guard)
//! Functions below the `harness` marker are NOT vendored; everything above it is copied unchanged
//! (comments included) except that some `fn`/`pub(crate) fn` were made `pub fn` so the tests can call them.

/// The largest box radius the blurs use: wider boxes look the same on any raster we make.
const MAX_BOX: usize = 1 << 17;

/// Radii of three box blurs approximating a Gaussian of `sigma` pixels.
pub fn gauss_boxes(sigma: f64) -> [usize; 3] {
    let sigma = if sigma.is_finite() { sigma.clamp(0.0, MAX_BOX as f64 / 2.0) } else { 0.0 };
    let n = 3.0;
    let w_ideal = (12.0 * sigma * sigma / n + 1.0).sqrt();
    let mut wl = w_ideal.floor() as i64;
    if wl % 2 == 0 {
        wl -= 1;
    }
    let wu = wl + 2;
    let m_ideal = (12.0 * sigma * sigma - n * (wl * wl) as f64 - 4.0 * n * wl as f64 - 3.0 * n) / (-4.0 * wl as f64 - 4.0);
    let m = m_ideal.round() as i64;
    let r = |i: i64| ((((if i < m { wl } else { wu }) - 1) / 2).max(0) as usize).min(MAX_BOX);
    [r(0), r(1), r(2)]
}

// ---- harness (not vendored) ----------------------------------------------------------------
// `gaussian` / `blur_plane` (same file) do nothing when sigma < 0.2, else run one running-mean
// box pass per radius of `gauss_boxes(sigma)` along each axis (zero past the edges).

/// Impulse response of vectorcraft `effects::pixel::gaussian` per axis.
pub fn impulse(sigma: f64) -> Vec<f64> {
    if sigma < 0.2 {
        return vec![1.0];
    }
    super::box_cascade(&gauss_boxes(sigma))
}
