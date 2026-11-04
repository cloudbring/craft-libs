//! Vendored from storytold/filmcraft @ eb59ec09daaea519c9c8fffa404a594ae50d00e0 (origin/main, 2026-11-02)
//! Source path: crates/render/src/effects.rs (boxes_for_gauss, gaussian_boxes)
//! Functions below the `harness` marker are NOT vendored; everything above it is copied unchanged
//! (comments included) except that some `fn`/`pub(crate) fn` were made `pub fn` so the tests can call them.

/// Box radii for an n-pass box blur approximating a Gaussian of sigma (Kovesi / Wells).
fn boxes_for_gauss(sigma: f32, n: usize) -> Vec<usize> {
    let wideal = ((12.0 * sigma * sigma / n as f32) + 1.0).sqrt();
    let mut wl = wideal.floor() as i32;
    if wl % 2 == 0 {
        wl -= 1;
    }
    let wu = wl + 2;
    let mideal = (12.0 * sigma * sigma - (n as i32 * wl * wl) as f32 - 4.0 * n as f32 * wl as f32 - 3.0 * n as f32) / (-4.0 * wl as f32 - 4.0);
    let m = mideal.round() as i32;
    (0..n as i32).map(|i| (((if i < m { wl } else { wu }) - 1) / 2).max(0) as usize).collect()
}

/// The box radii [`gaussian`] runs on a `w`×`h` image, per axis (empty: that axis is untouched).
pub fn gaussian_boxes(w: usize, h: usize, sigma_x: f32, sigma_y: f32) -> (Vec<u32>, Vec<u32>) {
    // beyond a few image sizes every radius gives the same (flat) result; cap to keep box sizes sane
    let sigma_x = if sigma_x.is_finite() { sigma_x.min(w.max(8) as f32 * 2.0) } else { 0.0 };
    let sigma_y = if sigma_y.is_finite() { sigma_y.min(h.max(8) as f32 * 2.0) } else { 0.0 };
    let radii = |s: f32| if s > 0.3 { boxes_for_gauss(s, 3).into_iter().map(|r| r.min(u32::MAX as usize) as u32).collect() } else { Vec::new() };
    (radii(sigma_x), radii(sigma_y))
}

// ---- harness (not vendored) ----------------------------------------------------------------
// `gaussian` runs `box_blur` with the radii `gaussian_boxes` returns (empty when sigma <= 0.3):
// one running-mean pass of radius r per entry, edges repeated.

/// Impulse response of filmcraft `render::effects::gaussian` on a large image, per axis.
pub fn impulse(sigma: f64) -> Vec<f64> {
    let (radii, _) = gaussian_boxes(1 << 20, 1 << 20, sigma as f32, 0.0);
    super::box_cascade(&radii.iter().map(|&r| r as usize).collect::<Vec<_>>())
}
