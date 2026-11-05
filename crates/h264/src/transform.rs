//! Scaling (dequantisation) and inverse transforms (8.5.10 - 8.5.13).

use crate::params::ScalingMatrices;
use crate::tables::{norm_adjust4, norm_adjust8};

/// LevelScale4x4 / LevelScale8x8 for all six scaling lists and qP % 6, raster order.
#[derive(Clone)]
pub struct LevelScale {
    /// [list 0..6 (IntraY, IntraCb, IntraCr, InterY, InterCb, InterCr)][qp % 6][pos]
    pub ls4: [[[i32; 16]; 6]; 6],
    /// [list 0..6 (IntraY, InterY, IntraCb, InterCb, IntraCr, InterCr)][qp % 6][pos]
    pub ls8: [[[i32; 64]; 6]; 6],
}

impl LevelScale {
    pub fn new(m: &ScalingMatrices) -> Box<Self> {
        let mut s = Box::new(LevelScale { ls4: [[[0; 16]; 6]; 6], ls8: [[[0; 64]; 6]; 6] });
        for l in 0..6 {
            for q in 0..6 {
                for p in 0..16 {
                    s.ls4[l][q][p] = m.m4[l][p] as i32 * norm_adjust4(q, p) as i32;
                }
                for p in 0..64 {
                    s.ls8[l][q][p] = m.m8[l][p] as i32 * norm_adjust8(q, p) as i32;
                }
            }
        }
        s
    }
}

/// Scale one 4x4 AC coefficient (8.5.12.1, not the DC of Intra16x16 / chroma blocks).
#[inline(always)]
pub fn scale4(c: i32, ls: i32, qp: i32) -> i32 {
    if qp >= 24 { c.wrapping_mul(ls) << (qp / 6 - 4) } else { c.wrapping_mul(ls).wrapping_add(1 << (3 - qp / 6)) >> (4 - qp / 6) }
}

/// Scale one 8x8 coefficient (8.5.13.1).
#[inline(always)]
pub fn scale8(c: i32, ls: i32, qp: i32) -> i32 {
    if qp >= 36 { c.wrapping_mul(ls) << (qp / 6 - 6) } else { c.wrapping_mul(ls).wrapping_add(1 << (5 - qp / 6)) >> (6 - qp / 6) }
}

/// Intra16x16 luma DC: inverse Hadamard + scaling (8.5.10). `c` is raster order (row*4+col);
/// output replaces `c` with dcY (raster: row i -> 4x4 block row, col j -> block column).
pub fn luma_dc_dequant(c: &mut [i32; 16], qp: i32, ls00: i32) {
    let mut t = [0i32; 16];
    // rows
    for i in 0..4 {
        let r = &c[i * 4..i * 4 + 4];
        let s01 = r[0] + r[1];
        let d01 = r[0] - r[1];
        let s23 = r[2] + r[3];
        let d23 = r[2] - r[3];
        t[i * 4] = s01 + s23;
        t[i * 4 + 1] = s01 - s23;
        t[i * 4 + 2] = d01 - d23;
        t[i * 4 + 3] = d01 + d23;
    }
    // columns
    for j in 0..4 {
        let s01 = t[j] + t[4 + j];
        let d01 = t[j] - t[4 + j];
        let s23 = t[8 + j] + t[12 + j];
        let d23 = t[8 + j] - t[12 + j];
        let f = [s01 + s23, s01 - s23, d01 - d23, d01 + d23];
        for i in 0..4 {
            c[i * 4 + j] =
                if qp >= 36 { f[i].wrapping_mul(ls00) << (qp / 6 - 6) } else { f[i].wrapping_mul(ls00).wrapping_add(1 << (5 - qp / 6)) >> (6 - qp / 6) };
        }
    }
}

/// 4:2:0 chroma DC: 2x2 transform + scaling (8.5.11). `c` = [c00, c01, c10, c11].
pub fn chroma_dc_dequant_420(c: &mut [i32; 4], qp: i32, ls00: i32) {
    let s0 = c[0].wrapping_add(c[1]);
    let d0 = c[0].wrapping_sub(c[1]);
    let s1 = c[2].wrapping_add(c[3]);
    let d1 = c[2].wrapping_sub(c[3]);
    let f = [s0.wrapping_add(s1), d0.wrapping_add(d1), s0.wrapping_sub(s1), d0.wrapping_sub(d1)];
    for k in 0..4 {
        c[k] = (f[k].wrapping_mul(ls00) << (qp / 6)) >> 5;
    }
}

#[inline(always)]
fn clip_u8(v: i32) -> u8 {
    v.clamp(0, 255) as u8
}

/// Inverse 4x4 transform of raster-order `d` and add to `dst` (8.5.12.2, 8.5.14).
///
/// Rows first, then columns (the spec's order: the `>> 1` steps make it matter). The column
/// pass and the reconstruction run across all columns at once so they vectorise.
pub fn idct4_add(d: &[i32; 16], dst: &mut [u8], stride: usize) {
    let o = idct4_core(d);
    for (i, row) in o.iter().enumerate() {
        let Some(line) = dst.get_mut(i * stride..).and_then(|d| d.first_chunk_mut::<4>()) else {
            return;
        };
        for (p, &v) in line.iter_mut().zip(row) {
            *p = clip_u8(*p as i32 + ((v + 32) >> 6));
        }
    }
}

/// Row then column pass of the inverse 4x4 transform, before the final `(x + 32) >> 6`.
/// Coefficients are clamped to the spec range `[-2^15, 2^15 - 1]` first (8.5.12.2), which bounds every
/// intermediate by 401408 + 32 (`lean/TRUST.md`), so none of the `i32` operations can overflow.
#[inline(always)]
fn idct4_core(d: &[i32; 16]) -> [[i32; 4]; 4] {
    let d = d.map(|coefficient| coefficient.clamp(-32768, 32767));
    let mut t = [[0i32; 4]; 4];
    for (i, row) in t.iter_mut().enumerate() {
        let r = &d[i * 4..i * 4 + 4];
        let e = r[0] + r[2];
        let f = r[0] - r[2];
        let g = (r[1] >> 1) - r[3];
        let h = r[1] + (r[3] >> 1);
        *row = [e + h, f + g, f - g, e - h];
    }
    let mut o = [[0i32; 4]; 4];
    for j in 0..4 {
        let e = t[0][j] + t[2][j];
        let f = t[0][j] - t[2][j];
        let g = (t[1][j] >> 1) - t[3][j];
        let h = t[1][j] + (t[3][j] >> 1);
        o[0][j] = e + h;
        o[1][j] = f + g;
        o[2][j] = f - g;
        o[3][j] = e - h;
    }
    o
}

/// `idct4_add` / `idct8_add` when all AC coefficients are zero: every output sample is
/// `(dc + 32) >> 6`, so the transform reduces to adding that to the `N`x`N` block.
pub fn idct_dc_add<const N: usize>(dc: i32, dst: &mut [u8], stride: usize) {
    let v = (dc.clamp(-32768, 32767) + 32) >> 6;
    for i in 0..N {
        let Some(line) = dst.get_mut(i * stride..).and_then(|d| d.first_chunk_mut::<N>()) else {
            return;
        };
        for p in line {
            *p = clip_u8(*p as i32 + v);
        }
    }
}

#[inline(always)]
fn idct8_1d(d: [i32; 8]) -> [i32; 8] {
    let a0 = d[0] + d[4];
    let a4 = d[0] - d[4];
    let a2 = (d[2] >> 1) - d[6];
    let a6 = d[2] + (d[6] >> 1);
    let b0 = a0 + a6;
    let b2 = a4 + a2;
    let b4 = a4 - a2;
    let b6 = a0 - a6;
    let a1 = -d[3] + d[5] - d[7] - (d[7] >> 1);
    let a3 = d[1] + d[7] - d[3] - (d[3] >> 1);
    let a5 = -d[1] + d[7] + d[5] + (d[5] >> 1);
    let a7 = d[3] + d[5] + d[1] + (d[1] >> 1);
    let b1 = a1 + (a7 >> 2);
    let b7 = a7 - (a1 >> 2);
    let b3 = a3 + (a5 >> 2);
    let b5 = (a3 >> 2) - a5;
    [b0 + b7, b2 + b5, b4 + b3, b6 + b1, b6 - b1, b4 - b3, b2 - b5, b0 - b7]
}

/// Inverse 8x8 transform of raster-order `d` and add to `dst` (8.5.13.2): rows, then columns,
/// the column pass and reconstruction across all columns at once (they vectorise).
pub fn idct8_add(d: &[i32; 64], dst: &mut [u8], stride: usize) {
    let o = idct8_core(d);
    for (i, row) in o.iter().enumerate() {
        let Some(line) = dst.get_mut(i * stride..).and_then(|d| d.first_chunk_mut::<8>()) else {
            return;
        };
        for (p, &v) in line.iter_mut().zip(row) {
            *p = clip_u8(*p as i32 + ((v + 32) >> 6));
        }
    }
}

/// Row then column pass of the inverse 8x8 transform, before the final `(x + 32) >> 6`.
/// With the coefficient clamp every intermediate is within 2097161 + 32 (`lean/TRUST.md`).
#[inline(always)]
fn idct8_core(d: &[i32; 64]) -> [[i32; 8]; 8] {
    let d = d.map(|coefficient| coefficient.clamp(-32768, 32767));
    let mut t = [[0i32; 8]; 8];
    for (row, src) in t.iter_mut().zip(d.as_chunks::<8>().0) {
        *row = idct8_1d(*src);
    }
    let mut o = [[0i32; 8]; 8];
    for j in 0..8 {
        let col = idct8_1d(std::array::from_fn(|i| t[i][j]));
        for i in 0..8 {
            o[i][j] = col[i];
        }
    }
    o
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn huge_coefficients_are_clamped_before_inverse_transform() {
        for coefficients in [[i32::MAX; 16], [i32::MIN; 16]] {
            let bounded = coefficients.map(|value| value.clamp(-32768, 32767));
            let (mut actual, mut expected) = ([0u8; 64], [0u8; 64]);
            idct4_add(&coefficients, &mut actual, 8);
            idct4_add(&bounded, &mut expected, 8);
            assert_eq!(actual, expected);
        }
        for coefficients in [[i32::MAX; 64], [i32::MIN; 64]] {
            let bounded = coefficients.map(|value| value.clamp(-32768, 32767));
            let (mut actual, mut expected) = ([0u8; 64], [0u8; 64]);
            idct8_add(&coefficients, &mut actual, 8);
            idct8_add(&bounded, &mut expected, 8);
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn huge_dc_only_coefficients_match_bounded_full_transforms() {
        for coefficient in [i32::MAX, i32::MIN] {
            let mut actual4 = [128u8; 16];
            let mut expected4 = actual4;
            let mut bounded4 = [0i32; 16];
            bounded4[0] = coefficient.clamp(-32768, 32767);
            idct_dc_add::<4>(coefficient, &mut actual4, 4);
            idct4_add(&bounded4, &mut expected4, 4);
            assert_eq!(actual4, expected4);

            let mut actual8 = [128u8; 64];
            let mut expected8 = actual8;
            let mut bounded8 = [0i32; 64];
            bounded8[0] = coefficient.clamp(-32768, 32767);
            idct_dc_add::<8>(coefficient, &mut actual8, 8);
            idct8_add(&bounded8, &mut expected8, 8);
            assert_eq!(actual8, expected8);
        }
    }

    /// The straightforward per-column formulation (8.5.12.2 / 8.5.13.2) the vectorised
    /// transforms must equal.
    fn reference_add(d: &[i32], n: usize, dst: &mut [u8], stride: usize) {
        let one = |v: &[i32]| -> Vec<i32> {
            if n == 4 {
                let (e, f, g, h) = (v[0] + v[2], v[0] - v[2], (v[1] >> 1) - v[3], v[1] + (v[3] >> 1));
                vec![e + h, f + g, f - g, e - h]
            } else {
                idct8_1d(v.try_into().unwrap()).to_vec()
            }
        };
        let rows: Vec<Vec<i32>> = (0..n).map(|i| one(&d[i * n..i * n + n])).collect();
        for j in 0..n {
            let col = one(&(0..n).map(|i| rows[i][j]).collect::<Vec<_>>());
            for i in 0..n {
                let p = &mut dst[i * stride + j];
                *p = (*p as i32 + ((col[i] + 32) >> 6)).clamp(0, 255) as u8;
            }
        }
    }

    #[test]
    fn vectorised_transforms_match_reference() {
        let mut rng = 0x1234_5678_9abc_def1u64;
        let mut next = move || {
            rng ^= rng << 13;
            rng ^= rng >> 7;
            rng ^= rng << 17;
            rng
        };
        for iter in 0..5000 {
            let range = [64u64, 1024, 8192, 1 << 15][iter % 4];
            let d4: [i32; 16] = std::array::from_fn(|_| (next() % (2 * range + 1)) as i32 - range as i32);
            let d8: [i32; 64] = std::array::from_fn(|_| (next() % (2 * range + 1)) as i32 - range as i32);
            let base: Vec<u8> = (0..24 * 8).map(|_| next() as u8).collect();
            let (mut a, mut b) = (base.clone(), base.clone());
            idct4_add(&d4, &mut a[3..], 24);
            reference_add(&d4, 4, &mut b[3..], 24);
            assert_eq!(a, b, "4x4 iteration {iter}");
            let (mut a, mut b) = (base.clone(), base);
            idct8_add(&d8, &mut a[5..], 24);
            reference_add(&d8, 8, &mut b[5..], 24);
            assert_eq!(a, b, "8x8 iteration {iter}");
        }
    }

    #[test]
    fn idct4_dc_only_matches_shortcut() {
        let mut d = [0i32; 16];
        d[0] = 640; // -> +10 everywhere
        let mut a = [100u8; 16];
        let mut b = [100u8; 16];
        idct4_add(&d, &mut a, 4);
        idct_dc_add::<4>(640, &mut b, 4);
        assert_eq!(a, b);
        assert!(a.iter().all(|&v| v == 110));
    }

    #[test]
    fn dc_shortcut_matches_full_transforms() {
        let mut rng = 0x0bad_cafe_dead_beefu64;
        let mut next = move || {
            rng ^= rng << 13;
            rng ^= rng >> 7;
            rng ^= rng << 17;
            rng
        };
        for iter in 0..5000 {
            let range = [64u64, 1024, 8192, 1 << 15][iter % 4];
            let dc = (next() % (2 * range + 1)) as i32 - range as i32;
            let base: Vec<u8> = (0..24 * 8).map(|_| next() as u8).collect();
            let mut d4 = [0i32; 16];
            d4[0] = dc;
            let (mut a, mut b) = (base.clone(), base.clone());
            idct4_add(&d4, &mut a[3..], 24);
            idct_dc_add::<4>(dc, &mut b[3..], 24);
            assert_eq!(a, b, "4x4 iteration {iter}");
            let mut d8 = [0i32; 64];
            d8[0] = dc;
            let (mut a, mut b) = (base.clone(), base);
            idct8_add(&d8, &mut a[5..], 24);
            idct_dc_add::<8>(dc, &mut b[5..], 24);
            assert_eq!(a, b, "8x8 iteration {iter}");
        }
    }

    #[test]
    fn idct4_hand_example() {
        // d = [[64, 64, 0, 0], 0...]: row0: e=64,f=64,g=32,h=64 -> [128, 96, 32, 0]; columns: each col only row0 set
        // col j: e=f=t0j, g=0, h=0 -> all rows = t0j. Result (t+32)>>6 = [2, 2, 1, 0] for every row.
        let mut d = [0i32; 16];
        d[0] = 64;
        d[1] = 64;
        let mut out = [0u8; 16];
        idct4_add(&d, &mut out, 4);
        for i in 0..4 {
            assert_eq!(&out[i * 4..i * 4 + 4], &[2, 2, 1, 0]);
        }
    }

    #[test]
    fn idct8_dc() {
        let mut d = [0i32; 64];
        d[0] = 64 * 5;
        let mut out = [10u8; 64];
        idct8_add(&d, &mut out, 8);
        assert!(out.iter().all(|&v| v == 15));
    }

    #[test]
    fn luma_dc_flat() {
        // single DC coefficient spreads equally to all 16 blocks
        let mut c = [0i32; 16];
        c[0] = 4;
        luma_dc_dequant(&mut c, 36, 16 * 16);
        assert!(c.iter().all(|&v| v == 4 * 256));
    }

    #[test]
    fn chroma_dc() {
        let mut c = [1, 0, 0, 0];
        chroma_dc_dequant_420(&mut c, 6, 16 * 16);
        // f = [1,1,1,1] -> ((1*256) << 1) >> 5 = 16
        assert_eq!(c, [16, 16, 16, 16]);
    }

    #[test]
    fn scale_functions() {
        assert_eq!(scale4(2, 160, 24), 2 * 160);
        assert_eq!(scale4(2, 160, 12), (2 * 160 + 2) >> 2);
        assert_eq!(scale8(1, 320, 36), 320);
        assert_eq!(scale8(1, 320, 30), (320 + 1) >> 1);
    }

    /// Corrupt streams can carry huge coefficient levels: dequantisation wraps (as release
    /// builds always did) instead of panicking with "attempt to multiply with overflow" in
    /// debug builds (found by mutation fuzzing an H.264 transport stream).
    #[test]
    fn huge_levels_dequantise_without_overflow_panics() {
        let mut c = [i32::MAX / 2, i32::MAX / 3, -i32::MAX / 2, 7];
        chroma_dc_dequant_420(&mut c, 51, 16 * 25);
        let _ = scale4(i32::MAX, 16 * 25, 51);
        let _ = scale4(i32::MAX, 16 * 25, 0);
        let _ = scale8(i32::MIN, 16 * 25, 51);
        let _ = scale8(i32::MIN, 16 * 25, 0);
    }

    // ---- Lean-model conformance (ticket cloudbring/artcraft-algorithms#220, `lean/TRUST.md`) ----

    const LEAN_IDCT: &str = include_str!("../tests/vectors/lean-idct.csv");

    struct LeanRow {
        n: usize,
        case: &'static str,
        coeffs: Vec<i32>,
        pre: Vec<i32>,
        res: Vec<i32>,
    }

    fn ints(field: &str) -> Vec<i32> {
        field.split(' ').map(|v| v.parse().expect("integer")).collect()
    }

    fn lean_rows() -> Vec<LeanRow> {
        LEAN_IDCT
            .lines()
            .skip(1)
            .map(|line| {
                let f: Vec<&'static str> = line.split(',').collect();
                assert_eq!(f.len(), 5, "bad row: {line}");
                LeanRow { n: f[0].parse().expect("n"), case: f[1], coeffs: ints(f[2]), pre: ints(f[3]), res: ints(f[4]) }
            })
            .collect()
    }

    /// The Lean `Int` model (`CraftIDCT.bf4`/`bf8`, row pass, column pass, `(v + 32) >> 6`) generated
    /// the CSV; the Rust `i32` transform must reproduce its pre-rounding output and residual exactly,
    /// both from `idct*_core` and through the public `idct*_add` with several predictions.
    #[test]
    fn rust_transform_equals_lean_int_model() {
        let rows = lean_rows();
        assert!(rows.len() >= 300, "expected the full vector set, got {}", rows.len());
        let (mut max4, mut max8) = (0i32, 0i32);
        for r in &rows {
            let n = r.n;
            let (pre, res): (Vec<i32>, Vec<i32>) = if n == 4 {
                let d: [i32; 16] = r.coeffs.clone().try_into().expect("16 coefficients");
                let o = idct4_core(&d);
                let pre: Vec<i32> = o.iter().flatten().copied().collect();
                max4 = max4.max(pre.iter().map(|v| v.abs()).max().unwrap_or(0));
                for base in [0u8, 77, 128, 255] {
                    let mut dst = [base; 16];
                    idct4_add(&d, &mut dst, 4);
                    for (k, &px) in dst.iter().enumerate() {
                        let want = (base as i32 + r.res[k]).clamp(0, 255) as u8;
                        assert_eq!(px, want, "idct4_add case {} base {base} pos {k}", r.case);
                    }
                }
                (pre.clone(), pre.iter().map(|v| (v + 32) >> 6).collect())
            } else {
                assert_eq!(n, 8);
                let d: [i32; 64] = r.coeffs.clone().try_into().expect("64 coefficients");
                let o = idct8_core(&d);
                let pre: Vec<i32> = o.iter().flatten().copied().collect();
                max8 = max8.max(pre.iter().map(|v| v.abs()).max().unwrap_or(0));
                for base in [0u8, 77, 128, 255] {
                    let mut dst = [base; 64];
                    idct8_add(&d, &mut dst, 8);
                    for (k, &px) in dst.iter().enumerate() {
                        let want = (base as i32 + r.res[k]).clamp(0, 255) as u8;
                        assert_eq!(px, want, "idct8_add case {} base {base} pos {k}", r.case);
                    }
                }
                (pre.clone(), pre.iter().map(|v| (v + 32) >> 6).collect())
            };
            assert_eq!(pre, r.pre, "pre-rounding output, {n}x{n} case {}", r.case);
            assert_eq!(res, r.res, "residual, {n}x{n} case {}", r.case);
        }
        // The worst-case patterns found by Lean are in the set and reach its maxima.
        assert_eq!(max4, 401_408);
        assert_eq!(max8, 1_782_272);
    }

    /// Independent `i64` evaluation of the same butterflies (shifts are arithmetic = floor).
    fn model_1d(v: &[i64]) -> Vec<i64> {
        if v.len() == 4 {
            let (e, f, g, h) = (v[0] + v[2], v[0] - v[2], (v[1] >> 1) - v[3], v[1] + (v[3] >> 1));
            vec![e + h, f + g, f - g, e - h]
        } else {
            let a = [v[0] + v[4], v[0] - v[4], (v[2] >> 1) - v[6], v[2] + (v[6] >> 1)];
            let (b0, b2, b4, b6) = (a[0] + a[3], a[1] + a[2], a[1] - a[2], a[0] - a[3]);
            let a1 = -v[3] + v[5] - v[7] - (v[7] >> 1);
            let a3 = v[1] + v[7] - v[3] - (v[3] >> 1);
            let a5 = -v[1] + v[7] + v[5] + (v[5] >> 1);
            let a7 = v[3] + v[5] + v[1] + (v[1] >> 1);
            let (b1, b7, b3, b5) = (a1 + (a7 >> 2), a7 - (a1 >> 2), a3 + (a5 >> 2), (a3 >> 2) - a5);
            vec![b0 + b7, b2 + b5, b4 + b3, b6 + b1, b6 - b1, b4 - b3, b2 - b5, b0 - b7]
        }
    }

    fn model_2d(d: &[i32], n: usize) -> Vec<i64> {
        let rows: Vec<Vec<i64>> = (0..n).map(|i| model_1d(&d[i * n..i * n + n].iter().map(|&v| v as i64).collect::<Vec<_>>())).collect();
        let mut out = vec![0i64; n * n];
        for j in 0..n {
            let col = model_1d(&(0..n).map(|i| rows[i][j]).collect::<Vec<_>>());
            for i in 0..n {
                out[i * n + j] = col[i];
            }
        }
        out
    }

    /// Debug builds have overflow checks on (`cargo test` default profile), so any `i32` wrap inside
    /// `idct*_core` would panic here.  All 2^16 corner matrices of the 4x4 box (the extremes the Lean
    /// monotonicity argument says dominate) and 30000 random 8x8 corner matrices are compared with the
    /// `i64` model.
    #[test]
    fn corner_inputs_do_not_overflow_and_equal_i64_model() {
        for c in 0u32..65536 {
            let d: [i32; 16] = std::array::from_fn(|p| if (c >> p) & 1 == 1 { 32767 } else { -32768 });
            let want = model_2d(&d, 4);
            let got = idct4_core(&d);
            for (k, &v) in got.iter().flatten().enumerate() {
                assert_eq!(v as i64, want[k], "4x4 corner {c:#x} pos {k}");
            }
        }
        let mut rng = 0x9e37_79b9_7f4a_7c15u64;
        for _ in 0..30000 {
            let d: [i32; 64] = std::array::from_fn(|_| {
                rng ^= rng << 13;
                rng ^= rng >> 7;
                rng ^= rng << 17;
                if rng & 1 == 1 { 32767 } else { -32768 }
            });
            let want = model_2d(&d, 8);
            let got = idct8_core(&d);
            for (k, &v) in got.iter().flatten().enumerate() {
                assert_eq!(v as i64, want[k], "8x8 random corner pos {k}");
            }
        }
    }
}
