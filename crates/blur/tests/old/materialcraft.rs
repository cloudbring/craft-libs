//! Vendored from storytold/materialcraft @ 613d603b8fc5f0805f70fe94f8797db9cff3001a (origin/main, 2026-11-03)
//! Source path: crates/nodes/src/ops.rs, fn gaussian (lines 11-36)
//! Functions below the `harness` marker are NOT vendored; everything above it is copied unchanged
//! (comments included) except that some `fn`/`pub(crate) fn` were made `pub fn` so the tests can call them.
//! `gaussian` convolves images, so only the lines that choose the kernel and box radii are copied
//! (source lines 12-15, 17-20, 25-29 and 32; each is marked). They are unchanged except that the
//! image's size is passed as `img_w`/`img_h` and the early return yields `Plan::Identity`.

/// What `ops::gaussian` runs: nothing, a sampled kernel (sigma < 3), or three boxes (radii).
pub enum Plan {
    Identity,
    Kernel(Vec<f32>),
    Boxes([i64; 3]),
}

pub fn plan(sigma: f32, img_w: u32, img_h: u32) -> Plan {
    // source lines 12-15
    if sigma.is_nan() || sigma <= 0.05 || !sigma.is_finite() {
        return Plan::Identity;
    }
    let sigma = sigma.min(img_w.max(img_h) as f32);
    // source lines 17-20 (inside the sigma < 3 branch)
    if sigma < 3.0 {
        let r = (sigma * 3.0).ceil() as i64;
        let k: Vec<f32> = (-r..=r).map(|i| (-(i * i) as f32 / (2.0 * sigma * sigma)).exp()).collect();
        let s: f32 = k.iter().sum();
        let k: Vec<f32> = k.iter().map(|v| v / s).collect();
        return Plan::Kernel(k);
    }
    // source lines 25-29
    let n = 3.0f32;
    let wl = ((12.0 * sigma * sigma / n + 1.0).sqrt()).floor() as i64;
    let wl = if wl % 2 == 0 { wl - 1 } else { wl };
    let wu = wl + 2;
    let m = ((12.0 * sigma * sigma - n * (wl * wl) as f32 - 4.0 * n * wl as f32 - 3.0 * n) / (-4.0 * wl as f32 - 4.0)).round() as i64;
    // source line 32 (the radius of pass i)
    let r = |i: i64| (if i < m { wl } else { wu } - 1) / 2;
    Plan::Boxes([r(0).max(0), r(1).max(0), r(2).max(0)])
}

// ---- harness (not vendored) ----------------------------------------------------------------

/// Impulse response of materialcraft `ops::gaussian` per axis, on an image large enough not to cap sigma.
pub fn impulse(sigma: f64) -> Vec<f64> {
    match plan(sigma as f32, 1 << 20, 1 << 20) {
        Plan::Identity => vec![1.0],
        Plan::Kernel(k) => k.iter().map(|&v| f64::from(v)).collect(),
        Plan::Boxes(r) => super::box_cascade(&r.map(|r| r as usize)),
    }
}
