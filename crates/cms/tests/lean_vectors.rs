//! Conformance of `craft-cms` sRGB transfer paths to the Lean-generated vectors in
//! `tests/vectors/srgb.csv` (IEC 61966-2-1; see `lean/TRUST.md`).
//!
//! Each reference value `v` is a certified lower bound with true value in `[v, v + 1.1e-30]`
//! (`lean/CraftSRGB/Cert.lean`), so after parsing to `f64` the reference is correct to rounding.
use craft_cms::curve::srgb_trc;
use craft_cms::{Builtin, Intent, Transform};

const TABLE: &str = include_str!("vectors/srgb.csv");

struct Row {
    input: f64,
    out: f64,
}

fn frac(s: &str) -> f64 {
    let (n, d) = s.split_once('/').expect("fraction");
    n.parse::<f64>().unwrap() / d.parse::<f64>().unwrap()
}

fn rows(kind: &str) -> Vec<Row> {
    TABLE
        .lines()
        .skip(1)
        .filter_map(|l| {
            let f: Vec<&str> = l.split(',').collect();
            assert_eq!(f.len(), 4, "bad row {l}");
            (f[0] == kind).then(|| Row { input: frac(f[2]), out: f[3].parse().unwrap() })
        })
        .collect()
}

fn knee(name: &str) -> f64 {
    TABLE.lines().find_map(|l| l.strip_prefix(&format!("knee,{name},"))).map(|r| r.rsplit(',').next().unwrap().parse().unwrap()).expect("knee row")
}

/// Distance between `x` and the next `f32` above it, at least the smallest normal spacing.
fn ulp32(x: f32) -> f64 {
    let a = x.abs().max(f32::MIN_POSITIVE);
    (f32::from_bits(a.to_bits() + 1) - a) as f64
}

fn ulp64(x: f64) -> f64 {
    let a = x.abs().max(f64::MIN_POSITIVE);
    f64::from_bits(a.to_bits() + 1) - a
}

#[test]
fn row_counts() {
    assert_eq!(rows("decode8").len(), 256);
    assert_eq!(rows("decode_grid").len(), 4096);
    assert_eq!(rows("encode_grid").len(), 4096);
    assert_eq!(TABLE.lines().count() - 1, 8453);
}

/// f64 decode and encode against the vectors: at most 6 (decode) and 4 (encode) ulp (f64); measured 5.00 and 3.00.
#[test]
fn curve_f64_matches_vectors() {
    let c = srgb_trc();
    let (mut dec, mut enc) = (0f64, 0f64);
    for r in rows("decode8").iter().chain(rows("decode_grid").iter()) {
        dec = dec.max((c.eval64(r.input) - r.out).abs() / ulp64(r.out));
    }
    for r in rows("encode_grid") {
        enc = enc.max((c.eval_inverse64(r.input) - r.out).abs() / ulp64(r.out));
    }
    println!("f64 max error: decode {dec:.2} ulp, encode {enc:.2} ulp");
    assert!(dec <= 6.0, "decode {dec} ulp");
    assert!(enc <= 4.0, "encode {enc} ulp");
}

/// f32 entry points: the `f32` input is itself rounded, so the reference is evaluated at the
/// rounded input (decode: exact reference derivative applied; here we bound against the vector at
/// the f64 input with `K` f32 ulp, K = 3 covering the 2.4 amplification of the rounded input).
#[test]
fn curve_f32_matches_vectors() {
    let c = srgb_trc();
    let (mut dec, mut enc) = (0f64, 0f64);
    for r in rows("decode8").iter().chain(rows("decode_grid").iter()) {
        let got = c.eval(r.input as f32);
        dec = dec.max((got as f64 - r.out).abs() / ulp32(r.out as f32));
    }
    for r in rows("encode_grid") {
        let got = c.eval_inverse(r.input as f32);
        enc = enc.max((got as f64 - r.out).abs() / ulp32(r.out as f32));
    }
    println!("f32 max error: decode {dec:.2} ulp, encode {enc:.2} ulp");
    assert!(dec <= 3.0, "decode {dec} ulp");
    assert!(enc <= 3.0, "encode {enc} ulp");
}

/// All 256 codes: `encode(decode(c)) == c` after 8-bit rounding, in f64 and f32 (Lean proves exact
/// equality over the reals: `eight_bit_roundtrip`).
#[test]
fn eight_bit_roundtrip_exact() {
    let c = srgb_trc();
    let (mut e64, mut e32) = (0f64, 0f64);
    for code in 0..=255u32 {
        let x = code as f64 / 255.0;
        let back = c.eval_inverse64(c.eval64(x));
        e64 = e64.max((back - x).abs());
        assert_eq!((back * 255.0).round() as u32, code, "f64 code {code}");
        let back32 = c.eval_inverse(c.eval(x as f32));
        e32 = e32.max((back32 as f64 - x).abs());
        assert_eq!((back32 * 255.0).round() as u32, code, "f32 code {code}");
    }
    println!("8-bit round trip max |err|: f64 {e64:.3e}, f32 {e32:.3e}");
    assert!(e64 < 1e-15);
    assert!(e32 < 1e-6);
}

/// The knee mismatch, measured on craft-cms: decode jumps by `R - L` at 0.04045 (Lean:
/// `decode_jump_bounds`, 2.3295e-9 to 2.3296e-9), and craft-cms's inverse switches branch at `R`,
/// not at 0.0031308, so it differs from the IEC encode only on `(0.0031308, R]` by at most
/// `encDip` (Lean `encode_dip_bounds`, 2.8518e-8).
#[test]
fn knee_discontinuity_matches_lean() {
    let c = srgb_trc();
    let (l, r) = (knee("L"), knee("R"));
    let a = 0.04045f64;
    // craft-cms takes the power branch at `x >= d` (the knee point itself), Lean/IEC takes the
    // linear branch at `x <= 0.04045`; they differ at the single point x = 0.04045 by R - L.
    let below = c.eval64(f64::from_bits(a.to_bits() - 1));
    let jump = c.eval64(a) - below;
    assert!((c.eval64(a) - r).abs() < 1e-14, "cms at the knee point is the power branch R");
    println!("measured decode jump (cms at 0.04045 minus the last f64 below) {jump:.6e}; Lean R-L {:.6e}", r - l);
    assert!((jump - (r - l)).abs() < 1e-14, "jump {jump}");
    assert!((2.3295e-9..2.3296e-9).contains(&jump));
    // craft-cms inverse vs the IEC encode (power branch above 0.0031308) just above the IEC knee.
    let (b, eb) = (0.0031308f64, knee("Eb"));
    let y = b * (1.0 + 1e-12);
    let iec = 1.055 * y.powf(1.0 / 2.4) - 0.055;
    let cms = c.eval_inverse64(y);
    println!("cms encode(b+) {cms:.12}, IEC {iec:.12}, Eb {eb:.12}");
    assert!((cms - iec).abs() <= 2.8518e-8 + 1e-12);
    assert!((cms - iec).abs() > 2.8e-8);
    // At the decode knee the round trip errs by about 2.959e-8 only through the IEC encode (EDa).
    let eda = knee("EDa");
    assert!((a - eda - 2.95915e-8).abs() < 1e-12);
}

fn linear_to_srgb_transforms() -> (Transform, Transform) {
    let s = Builtin::Srgb.profile();
    let l = Builtin::LinearSrgb.profile();
    (Transform::new(s, l, Intent::RelativeColorimetric, false).unwrap(), Transform::new(l, s, Intent::RelativeColorimetric, false).unwrap())
}

/// Whole-transform paths (sRGB profile to linear sRGB profile and back): exact f32 pipeline,
/// the table fast path, and u8 buffers. Gray ramps only (primaries identical, so the matrices
/// cancel up to float rounding).
#[test]
fn transform_paths_against_vectors() {
    let (dec_t, enc_t) = linear_to_srgb_transforms();
    let (mut dex, mut dfa, mut enx, mut enf) = (0f64, 0f64, 0f64, 0f64);
    for r in rows("decode_grid") {
        let inp = [r.input as f32; 3];
        let mut o = [0f32; 3];
        dec_t.eval(&inp, &mut o).unwrap();
        dex = dex.max((o[1] as f64 - r.out).abs());
        dec_t.eval_fast(&inp, &mut o).unwrap();
        dfa = dfa.max((o[1] as f64 - r.out).abs());
    }
    for r in rows("encode_grid") {
        let inp = [r.input as f32; 3];
        let mut o = [0f32; 3];
        enc_t.eval(&inp, &mut o).unwrap();
        enx = enx.max((o[1] as f64 - r.out).abs());
        enc_t.eval_fast(&inp, &mut o).unwrap();
        enf = enf.max((o[1] as f64 - r.out).abs());
    }
    println!("transform max abs error: decode exact {dex:.3e} fast {dfa:.3e}; encode exact {enx:.3e} fast {enf:.3e}");
    assert!(dex < 1e-6, "decode exact {dex}");
    assert!(enx < 1e-6, "encode exact {enx}");
    assert!(dfa < 1e-4, "decode fast {dfa}");
    assert!(enf < 1e-4, "encode fast {enf}");

    // u8 sRGB -> linear f32 -> u8 sRGB through the float transforms: every code survives.
    let mut bad = 0;
    for code in 0..=255u32 {
        let v = code as f32 / 255.0;
        let mut lin = [0f32; 3];
        dec_t.eval(&[v; 3], &mut lin).unwrap();
        let mut back = [0f32; 3];
        enc_t.eval(&lin, &mut back).unwrap();
        if (back[1] * 255.0).round() as u32 != code {
            bad += 1;
        }
    }
    assert_eq!(bad, 0, "codes that do not survive the f32 transform round trip");
}
