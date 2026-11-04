//! Each app's OLD Gaussian blur (the one craft-blur replaces), vendored at the origin/main SHA in
//! its file header. `impulse(sigma)` returns the 1-D impulse response of one axis, centre at
//! `len / 2`, built by that app's own width/kernel-selection code.
#![allow(clippy::all, clippy::pedantic, dead_code, unused)]

#[allow(clippy::all, dead_code, unused)]
pub mod effectcraft;
#[allow(clippy::all, dead_code, unused)]
pub mod filmcraft;
#[allow(clippy::all, dead_code, unused)]
pub mod lightcraft;
#[allow(clippy::all, dead_code, unused)]
pub mod materialcraft;
#[allow(clippy::all, dead_code, unused)]
pub mod photocraft;
#[allow(clippy::all, dead_code, unused)]
pub mod texturecraft;
#[allow(clippy::all, dead_code, unused)]
pub mod vectorcraft;

/// Convolution of running-mean boxes with the given radii (`r = 0` is the identity).
pub fn box_cascade(radii: &[usize]) -> Vec<f64> {
    let mut out = vec![1.0];
    for &r in radii {
        if r == 0 {
            continue;
        }
        let w = 2 * r + 1;
        let mut next = vec![0.0; out.len() + w - 1];
        for (i, &a) in out.iter().enumerate() {
            for j in 0..w {
                next[i + j] += a / w as f64;
            }
        }
        out = next;
    }
    out
}

/// Second central moment of a response centred at `len / 2`, normalised by its mass.
pub fn variance(h: &[f64]) -> f64 {
    let c = (h.len() / 2) as f64;
    let mass: f64 = h.iter().sum();
    let mean: f64 = h.iter().enumerate().map(|(i, v)| v * (i as f64 - c)).sum::<f64>() / mass;
    h.iter().enumerate().map(|(i, v)| v * (i as f64 - c).powi(2)).sum::<f64>() / mass - mean * mean
}
