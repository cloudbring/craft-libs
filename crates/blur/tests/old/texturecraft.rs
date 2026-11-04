//! Vendored from storytold/texturecraft @ 4edb8b86bc67f58922d6f16a6489059a78b61a3e (origin/main, 2026-11-02)
//! Source path: crates/compose/src/eval.rs (FIX, box_line; blur_rows' loop is in the harness because the original uses rayon)
//! Functions below the `harness` marker are NOT vendored; everything above it is copied unchanged
//! (comments included) except that some `fn`/`pub(crate) fn` were made `pub fn` so the tests can call them.

/// Fixed-point scale of the blur. Integer running sums make a texel's result independent of
/// the region it is computed in, so tiles composited in any grouping agree exactly.
const FIX: f64 = (1u64 << 24) as f64;

/// One box blur of radius `r` of a line (texels past the ends count as 0).
pub fn box_line(src: &[i64], dst: &mut [i64], r: usize) {
    let n = (2 * r + 1) as i64;
    // Rounded division of a window sum.
    let avg = |acc: i64| (2 * acc + n).div_euclid(2 * n);
    let mut acc: i64 = src.iter().take(r + 1).sum();
    for (i, d) in dst.iter_mut().enumerate() {
        *d = avg(acc);
        acc += src.get(i + r + 1).copied().unwrap_or(0);
        if i >= r {
            acc -= src.get(i - r).copied().unwrap_or(0);
        }
    }
}

// ---- harness (not vendored) ----------------------------------------------------------------
// `blur` (same file) scales the plane by FIX into i64 and `blur_rows` runs `box_line` three times
// with the same radius `r` along every row, then the same along columns. The user-facing
// parameter is the integer radius `r`, not a sigma: the table below uses r = round(sigma).

/// Impulse response of one axis of texturecraft's 3 x `box_line(r)` on an `n`-texel row, in
/// units of the input impulse (the integer fixed-point rounding is part of the response).
pub fn impulse_radius(r: usize) -> Vec<f64> {
    let n = 6 * r + 41;
    let mut row = vec![0i64; n];
    row[n / 2] = FIX as i64;
    let mut tmp = vec![0i64; n];
    for _ in 0..3 {
        box_line(&row, &mut tmp, r);
        row.copy_from_slice(&tmp);
    }
    row.iter().map(|&v| v as f64 / FIX).collect()
}

/// Impulse response at the radius a requested `sigma` maps to (nearest whole radius, at least 1).
pub fn impulse(sigma: f64) -> Vec<f64> {
    impulse_radius((sigma.round() as usize).max(1))
}
