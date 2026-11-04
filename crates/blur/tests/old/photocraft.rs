//! Vendored from storytold/photocraft @ 327c4cc82e737a3e0af7c147a477126d77525495 (origin/main, 2026-11-02)
//! Source path: crates/algo/src/blur.rs (gaussian_kernel, gaussian_blur_kernel, boxes_for_gauss; the `box_widths` selection is in the harness) and crates/algo/src/fxutil.rs (gauss_box_radii)
//! Functions below the `harness` marker are NOT vendored; everything above it is copied unchanged
//! (comments included) except that some `fn`/`pub(crate) fn` were made `pub fn` so the tests can call them.

/// Normalized Gaussian kernel with standard deviation `sigma` (radius 3σ).
pub fn gaussian_kernel(sigma: f32) -> Vec<f32> {
    if sigma < 0.05 {
        return vec![1.0];
    }
    let r = (sigma * 3.0).ceil() as i32;
    let k: Vec<f32> = (-r..=r).map(|i| (-(i * i) as f32 / (2.0 * sigma * sigma)).exp()).collect();
    let s: f32 = k.iter().sum();
    k.into_iter().map(|v| v / s).collect()
}

/// Gaussian Blur's kernel for `radius` (its standard deviation). Sampling the curve at whole
/// pixels leaves a small radius almost no blur: at 0.3 the neighbour weight is about 0.004, a
/// tenth of the spread asked for (#8544). Below 1 the sampling width is instead chosen so the
/// kernel has exactly the variance `radius²`; from 1 up the sampled kernel is within about 0.4%
/// of it (its 3σ truncation), so larger radii are unchanged and the blur grows smoothly from 0.05.
pub fn gaussian_blur_kernel(radius: f32) -> Vec<f32> {
    if !(0.05..1.0).contains(&radius) {
        return gaussian_kernel(radius);
    }
    let variance = |k: &[f32]| {
        let r = (k.len() / 2) as f32;
        k.iter().enumerate().map(|(i, w)| w * (i as f32 - r).powi(2)).sum::<f32>()
    };
    // Sampled at `width` with one fixed half-width for every radius below 1: its variance then
    // rises steadily with `width` (the search can't settle on a step where taps are added), and
    // the blur grows without the dips a width chosen per radius made at 1/3 and 2/3. Three is the
    // half-width `gaussian_kernel` uses at radius 1, so the hand-over is smooth too.
    let half = 3;
    let sampled = |width: f32| {
        let k: Vec<f32> = (-half..=half).map(|i| (-(i * i) as f32 / (2.0 * width * width)).exp()).collect();
        let s: f32 = k.iter().sum();
        k.into_iter().map(|v| v / s).collect::<Vec<f32>>()
    };
    let target = radius * radius;
    // The sampled kernel's variance only falls short of sigma², so the width lies above `radius`.
    let (mut lo, mut hi) = (radius, radius + 1.0);
    for _ in 0..40 {
        let mid = 0.5 * (lo + hi);
        if variance(&sampled(mid)) < target {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    sampled(0.5 * (lo + hi))
}

/// Box widths (odd) whose `n`-fold convolution has standard deviation `sigma`.
fn boxes_for_gauss(sigma: f32, n: usize) -> Vec<usize> {
    let nf = n as f32;
    let w_ideal = (12.0 * sigma * sigma / nf + 1.0).sqrt();
    let mut wl = w_ideal.floor() as i32;
    if wl % 2 == 0 {
        wl -= 1;
    }
    let wl = wl.max(1);
    let wu = wl + 2;
    let m_ideal = (12.0 * sigma * sigma - nf * (wl * wl) as f32 - 4.0 * nf * wl as f32 - 3.0 * nf) / (-4.0 * wl as f32 - 4.0);
    let m = m_ideal.round().clamp(0.0, nf) as usize;
    (0..n).map(|i| if i < m { wl as usize } else { wu as usize }).collect()
}

/// Odd box widths whose 3-fold convolution approximates a Gaussian of `sigma`
/// (Kovesi 2010), returned as radii.
pub fn gauss_box_radii(sigma: f32) -> [usize; 3] {
    let n = 3.0f32;
    let w_ideal = (12.0 * sigma * sigma / n + 1.0).sqrt();
    let mut wl = w_ideal.floor() as i32;
    if wl % 2 == 0 {
        wl -= 1;
    }
    let wl = wl.max(1);
    let wu = wl + 2;
    let m = ((12.0 * sigma * sigma - n * (wl * wl) as f32 - 4.0 * n * wl as f32 - 3.0 * n) / (-4.0 * wl as f32 - 4.0)).round().clamp(0.0, n) as usize;
    let mut radii = std::array::from_fn(|i| if i < m { (wl as usize - 1) / 2 } else { (wu as usize - 1) / 2 });
    // Kovesi's rounded triple degenerates below sigma ~ 0.577: `wl` floors to 1 and every
    // radius becomes 0, so the passes would be no-ops and callers with documented small
    // sigmas (Camera Raw `sharpenRadius` 0.5, colour noise, HDR Toning `radius` 1) would
    // silently change nothing (issue #706). One radius-1 pass is the smallest real blur;
    // it only engages where the ideal widths are already at their floor, leaving larger
    // sigmas exactly as the paper prescribes.
    if radii.iter().all(|&r| r == 0) {
        radii[0] = 1;
    }
    radii
}

// ---- harness (not vendored) ----------------------------------------------------------------
// `box_widths` and `Separable::of` (crates/algo/src/blur.rs, crates/algo/src/separable.rs) choose:
// GaussianBlur radius > 4.0 -> boxes_for_gauss(radius, 3); otherwise the kernel from
// gaussian_blur_kernel(radius). That selection is reproduced here; it is two lines in the source.

/// Impulse response (centre = len / 2) of the photocraft Gaussian Blur filter at `radius` = σ.
pub fn impulse(sigma: f64) -> Vec<f64> {
    let radius = sigma as f32;
    if radius > 4.0 {
        let widths = boxes_for_gauss(radius, 3);
        super::box_cascade(&widths.iter().map(|w| w / 2).collect::<Vec<_>>())
    } else {
        gaussian_blur_kernel(radius).iter().map(|&v| f64::from(v)).collect()
    }
}

/// Impulse response of `gauss_blur_n` (crates/algo/src/fxutil.rs): no-op below σ 0.3, else
/// three boxes of the radii `gauss_box_radii` returns.
pub fn impulse_fxutil(sigma: f64) -> Vec<f64> {
    if (sigma as f32) < 0.3 {
        return vec![1.0];
    }
    super::box_cascade(&gauss_box_radii(sigma as f32))
}
