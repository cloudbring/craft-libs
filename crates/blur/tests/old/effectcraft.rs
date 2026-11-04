//! Vendored from storytold/effectcraft @ 7870b5916ae0749d497d880989683bf2946d6c8b (origin/main, 2026-11-02)
//! Source path: crates/raster/src/blur.rs (box_radii, EXACT_SIGMA_BELOW, gaussian_is_exact, gauss_pass kernel) and crates/effects/src/util.rs
//! Functions below the `harness` marker are NOT vendored; everything above it is copied unchanged
//! (comments included) except that some `fn`/`pub(crate) fn` were made `pub fn` so the tests can call them.

/// Box radii for `n` passes approximating a Gaussian of `sigma` (Kovesi / "boxes for Gauss").
pub fn box_radii(sigma: f64, n: usize) -> Vec<usize> {
    if sigma <= 0.0 {
        return vec![0; n];
    }
    let w_ideal = (12.0 * sigma * sigma / n as f64 + 1.0).sqrt();
    let mut wl = w_ideal.floor() as i64;
    if wl % 2 == 0 {
        wl -= 1;
    }
    let wu = wl + 2;
    let m_ideal = (12.0 * sigma * sigma - (n as i64 * wl * wl) as f64 - 4.0 * n as f64 * wl as f64 - 3.0 * n as f64) / (-4.0 * wl as f64 - 4.0);
    let m = m_ideal.round() as i64;
    (0..n as i64).map(|i| (((if i < m { wl } else { wu }) - 1) / 2).max(0) as usize).collect()
}

/// Below this sigma (pixels) three integer-radius box passes are too coarse (sigma under ~0.6 is
/// a no-op and the result steps between a few shapes), so [`gaussian_blur_exact`] is used instead.
const EXACT_SIGMA_BELOW: f64 = 3.0;

/// True when a Gaussian of these per-axis sigmas should use [`gaussian_blur_exact`].
pub fn gaussian_is_exact(sigma_x: f64, sigma_y: f64) -> bool {
    sigma_x.max(sigma_y) < EXACT_SIGMA_BELOW
}

// ---- harness (not vendored) ----------------------------------------------------------------
// `gaussian_blur` runs three box passes per axis when sigma > 0.05 using `box_radii`. The
// exact path is `gauss_pass` (same file); its kernel lines are reproduced verbatim here:
//     let r = (sigma * 3.0).ceil() as i64;
//     let mut k: Vec<f64> = (-r..=r).map(|i| (-((i * i) as f64) / (2.0 * sigma * sigma)).exp()).collect();
//     let total: f64 = k.iter().sum();
//     k.iter_mut().for_each(|v| *v /= total);
// (`gauss_pass` returns the image unchanged when sigma <= 1e-3.) misc.rs::gaussian picks the
// exact path when `gaussian_is_exact(sx, sy)`, i.e. sigma < 3.

/// Impulse response of effectcraft's Gaussian at per-axis `sigma` (after its blurriness/2 scaling).
pub fn impulse(sigma: f64) -> Vec<f64> {
    if gaussian_is_exact(sigma, sigma) {
        if sigma.is_nan() || sigma <= 1e-3 {
            return vec![1.0];
        }
        let r = (sigma * 3.0).ceil() as i64;
        let mut k: Vec<f64> = (-r..=r).map(|i| (-((i * i) as f64) / (2.0 * sigma * sigma)).exp()).collect();
        let total: f64 = k.iter().sum();
        k.iter_mut().for_each(|v| *v /= total);
        k
    } else {
        super::box_cascade(&box_radii(sigma, 3))
    }
}
