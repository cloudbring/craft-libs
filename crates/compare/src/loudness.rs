//! Integrated loudness per ITU-R BS.1770-4: K-weighting, 400 ms gating blocks with 75 % overlap,
//! absolute gate at -70 LUFS and relative gate 10 LU below the ungated-by-relative mean.
//!
//! Checked against the EBU Tech 3341 (2023) minimum-requirement signals 1 to 6, synthesised in the
//! tests (see `tests` below for the expected values and the 0.1 LU tolerance).
//!
//! The two K-weighting biquads are derived from their analogue prototypes for any sample rate
//! (the same bilinear-transform parameters libebur128 uses); at 48 kHz they reproduce the
//! BS.1770-4 table coefficients to 1e-12. Input is interleaved `f32`; the LFE channel is excluded
//! by giving it weight 0.

use crate::{CompareError, Verdict};

/// Channel weights `G_i` for L, R.
pub const STEREO: [f64; 2] = [1.0, 1.0];
/// L, R, C, Ls, Rs.
pub const FIVE_ZERO: [f64; 5] = [1.0, 1.0, 1.0, SURROUND, SURROUND];
/// L, R, C, LFE (not measured), Ls, Rs.
pub const FIVE_ONE: [f64; 6] = [1.0, 1.0, 1.0, 0.0, SURROUND, SURROUND];
/// BS.1770-4 surround channel weight, +1.5 dB.
pub const SURROUND: f64 = 1.41;

/// Absolute gate, LUFS.
pub const ABSOLUTE_GATE: f64 = -70.0;
/// Relative gate offset, LU.
pub const RELATIVE_GATE: f64 = -10.0;
const OFFSET: f64 = -0.691;

#[derive(Clone, Copy, Debug, PartialEq)]
struct Biquad {
    b: [f64; 3],
    a: [f64; 2],
}

impl Biquad {
    /// Stage 1: high shelf, about +4 dB above 1.5 kHz (head-related pre-filter).
    fn shelf(rate: f64) -> Self {
        let (f0, gain_db, q) = (1_681.974_450_955_533, 3.999_843_853_973_347, 0.707_175_236_955_419_6);
        let k = (std::f64::consts::PI * f0 / rate).tan();
        let vh = 10f64.powf(gain_db / 20.0);
        let vb = vh.powf(0.499_666_774_154_541_6);
        let a0 = 1.0 + k / q + k * k;
        Self {
            b: [(vh + vb * k / q + k * k) / a0, 2.0 * (k * k - vh) / a0, (vh - vb * k / q + k * k) / a0],
            a: [2.0 * (k * k - 1.0) / a0, (1.0 - k / q + k * k) / a0],
        }
    }

    /// Stage 2: RLB high-pass, 38 Hz.
    fn high_pass(rate: f64) -> Self {
        let (f0, q) = (38.135_470_876_024_44, 0.500_327_037_323_877_3);
        let k = (std::f64::consts::PI * f0 / rate).tan();
        let a0 = 1.0 + k / q + k * k;
        Self { b: [1.0, -2.0, 1.0], a: [2.0 * (k * k - 1.0) / a0, (1.0 - k / q + k * k) / a0] }
    }
}

#[derive(Default, Clone, Copy)]
struct State {
    x: [f64; 2],
    y: [f64; 2],
}

impl State {
    fn run(&mut self, f: &Biquad, x: f64) -> f64 {
        let y = f.b[0] * x + f.b[1] * self.x[0] + f.b[2] * self.x[1] - f.a[0] * self.y[0] - f.a[1] * self.y[1];
        self.x = [x, self.x[0]];
        self.y = [y, self.y[0]];
        y
    }
}

/// `(shelf, high_pass)` coefficients as `([b0,b1,b2],[a1,a2])` pairs, for inspection and tests.
pub fn k_weighting_coefficients(rate: u32) -> [([f64; 3], [f64; 2]); 2] {
    let r = f64::from(rate);
    let (s, h) = (Biquad::shelf(r), Biquad::high_pass(r));
    [(s.b, s.a), (h.b, h.a)]
}

fn check(len: usize, channels: usize, weights: &[f64], rate: u32) -> Result<(), CompareError> {
    let bad = |m: String| Err(CompareError::Shape(m));
    if channels == 0 || weights.len() != channels {
        return bad(format!("{channels} channels but {} weights", weights.len()));
    }
    if !len.is_multiple_of(channels) {
        return bad(format!("{len} samples is not a multiple of {channels} channels"));
    }
    if rate < 8_000 {
        return bad(format!("sample rate {rate} Hz is below 8000"));
    }
    Ok(())
}

/// Mean-square energy of every 400 ms block (100 ms hop), K-weighted and channel-weighted:
/// `z_j = sum_i G_i * mean(y_i^2)`. Empty when the input is shorter than one block.
pub fn block_energies(interleaved: &[f32], channels: usize, rate: u32, weights: &[f64]) -> Result<Vec<f64>, CompareError> {
    check(interleaved.len(), channels, weights, rate)?;
    let (shelf, hp) = (Biquad::shelf(f64::from(rate)), Biquad::high_pass(f64::from(rate)));
    let frames = interleaved.len() / channels;
    // Prefix sums of the weighted per-frame energy make each block O(1).
    let mut prefix = Vec::with_capacity(frames + 1);
    prefix.push(0.0f64);
    let mut states = vec![[State::default(); 2]; channels];
    let mut total = 0.0f64;
    for frame in interleaved.chunks_exact(channels) {
        let mut e = 0.0;
        for ((s, &g), &x) in states.iter_mut().zip(weights).zip(frame) {
            let shelved = s[0].run(&shelf, f64::from(x));
            let y = s[1].run(&hp, shelved);
            e += g * y * y;
        }
        total += e;
        prefix.push(total);
    }
    let rate = rate as usize;
    let (block, hop) = ((rate * 4).div_ceil(10), rate.div_ceil(10));
    let mut out = Vec::new();
    let mut start = 0;
    while start + block <= frames {
        out.push((prefix[start + block] - prefix[start]) / block as f64);
        start += hop;
    }
    Ok(out)
}

fn lufs(energy: f64) -> f64 {
    OFFSET + 10.0 * energy.log10()
}

/// Gated integrated loudness of block energies from [`block_energies`]; `None` when no block
/// passes the absolute gate (silence, or shorter than 400 ms).
pub fn gate(energies: &[f64]) -> Option<f64> {
    let above: Vec<f64> = energies.iter().copied().filter(|&z| lufs(z) > ABSOLUTE_GATE).collect();
    if above.is_empty() {
        return None;
    }
    let relative = lufs(above.iter().sum::<f64>() / above.len() as f64) + RELATIVE_GATE;
    let kept: Vec<f64> = above.into_iter().filter(|&z| lufs(z) > relative).collect();
    // `kept` cannot be empty: the loudest block is above any threshold below the mean.
    (!kept.is_empty()).then(|| lufs(kept.iter().sum::<f64>() / kept.len() as f64))
}

/// BS.1770-4 integrated loudness in LUFS of interleaved samples with per-channel weights
/// ([`STEREO`], [`FIVE_ZERO`], [`FIVE_ONE`] or your own; 0 excludes a channel).
///
/// # Errors
/// [`CompareError::Shape`] for zero channels, a weight count that differs from `channels`, a
/// sample count that is not a multiple of `channels`, or a rate below 8 kHz.
pub fn integrated_lufs(interleaved: &[f32], channels: usize, rate: u32, weights: &[f64]) -> Result<Option<f64>, CompareError> {
    Ok(gate(&block_energies(interleaved, channels, rate, weights)?))
}

/// Verdict on the loudness difference between a reference and a test signal: `Identical` at 0 LU,
/// `Within` up to `tolerance_lu`, else `Exceeds`. A signal with no measurable loudness equals only
/// another such signal; against a measurable one it exceeds with an infinite difference.
pub fn compare_loudness(reference: Option<f64>, test: Option<f64>, tolerance_lu: f64) -> Verdict {
    match (reference, test) {
        (None, None) => Verdict::Identical,
        (Some(a), Some(b)) => Verdict::at_most("integrated loudness (LU)", (a - b).abs(), tolerance_lu),
        _ => Verdict::Exceeds { metric: "integrated loudness (LU)".into(), value: f64::INFINITY, limit: tolerance_lu },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FS: u32 = 48_000;

    fn db(x: f64) -> f64 {
        10f64.powf(x / 20.0)
    }

    /// `secs` of 1 kHz sine at per-channel peak `dbfs` on each channel (`peaks` gives one level per
    /// channel), appended interleaved.
    fn tone(out: &mut Vec<f32>, rate: u32, secs: f64, peaks_dbfs: &[f64]) {
        let n = (secs * f64::from(rate)).round() as usize;
        let start = out.len() / peaks_dbfs.len();
        for i in start..start + n {
            let s = (2.0 * std::f64::consts::PI * 1000.0 * i as f64 / f64::from(rate)).sin();
            out.extend(peaks_dbfs.iter().map(|p| (db(*p) * s) as f32));
        }
    }

    fn stereo(rate: u32, segments: &[(f64, f64)]) -> Vec<f32> {
        let mut v = Vec::new();
        for &(secs, dbfs) in segments {
            tone(&mut v, rate, secs, &[dbfs, dbfs]);
        }
        v
    }

    fn lu(v: &[f32], ch: usize, rate: u32, w: &[f64]) -> f64 {
        integrated_lufs(v, ch, rate, w).unwrap().unwrap()
    }

    #[test]
    fn k_weighting_at_48k_matches_the_bs1770_table() {
        let [(sb, sa), (hb, ha)] = k_weighting_coefficients(48_000);
        let close = |a: f64, b: f64| assert!((a - b).abs() < 1e-9, "{a} vs {b}");
        close(sb[0], 1.535_124_859_586_97);
        close(sb[1], -2.691_696_189_406_38);
        close(sb[2], 1.198_392_810_852_85);
        close(sa[0], -1.690_659_293_182_41);
        close(sa[1], 0.732_480_774_215_85);
        for (got, want) in hb.iter().zip([1.0, -2.0, 1.0]) {
            close(*got, want);
        }
        close(ha[0], -1.990_047_454_833_98);
        close(ha[1], 0.990_072_250_366_21);
    }

    #[test]
    fn tech3341_case_1_stereo_1khz_minus_23_reads_minus_23() {
        let v = stereo(FS, &[(20.0, -23.0)]);
        let l = lu(&v, 2, FS, &STEREO);
        assert!((l + 23.0).abs() <= 0.1, "{l}");
    }

    #[test]
    fn tech3341_case_2_minus_33_reads_minus_33() {
        let l = lu(&stereo(FS, &[(20.0, -33.0)]), 2, FS, &STEREO);
        assert!((l + 33.0).abs() <= 0.1, "{l}");
    }

    #[test]
    fn tech3341_calibration_minus_18_reads_minus_18() {
        let l = lu(&stereo(FS, &[(20.0, -18.0)]), 2, FS, &STEREO);
        assert!((l + 18.0).abs() <= 0.1, "{l}");
    }

    #[test]
    fn tech3341_case_3_relative_gate_ignores_the_quiet_tails() {
        let l = lu(&stereo(FS, &[(10.0, -36.0), (60.0, -23.0), (10.0, -36.0)]), 2, FS, &STEREO);
        assert!((l + 23.0).abs() <= 0.1, "{l}");
    }

    #[test]
    fn tech3341_case_4_absolute_gate_ignores_minus_72() {
        let v = stereo(FS, &[(10.0, -72.0), (10.0, -36.0), (60.0, -23.0), (10.0, -36.0), (10.0, -72.0)]);
        let l = lu(&v, 2, FS, &STEREO);
        assert!((l + 23.0).abs() <= 0.1, "{l}");
    }

    #[test]
    fn tech3341_case_5_loud_middle_section() {
        let l = lu(&stereo(FS, &[(20.0, -26.0), (20.1, -20.0), (20.0, -26.0)]), 2, FS, &STEREO);
        assert!((l + 23.0).abs() <= 0.1, "{l}");
    }

    #[test]
    fn tech3341_case_6_five_channel_weights() {
        let mut v = Vec::new();
        tone(&mut v, FS, 20.0, &[-28.0, -28.0, -24.0, -30.0, -30.0]);
        let l = lu(&v, 5, FS, &FIVE_ZERO);
        assert!((l + 23.0).abs() <= 0.1, "{l}");
        // The same signal with an extra silent LFE slot, which weight 0 ignores.
        let mut six = Vec::new();
        for f in v.as_chunks::<5>().0 {
            six.extend_from_slice(&[f[0], f[1], f[2], 1.0, f[3], f[4]]);
        }
        let l6 = lu(&six, 6, FS, &FIVE_ONE);
        assert!((l6 - l).abs() < 0.001, "{l6} vs {l}");
    }

    #[test]
    fn other_sample_rates_agree_with_48k() {
        for rate in [44_100, 96_000, 32_000] {
            let l = lu(&stereo(rate, &[(20.0, -23.0)]), 2, rate, &STEREO);
            assert!((l + 23.0).abs() <= 0.1, "{rate}: {l}");
        }
    }

    #[test]
    fn mono_zero_dbfs_sine_is_minus_3_01() {
        // BS.1770-4: a 0 dBFS 997 Hz sine in one channel reads -3.01 LKFS; 1 kHz is within 0.01 dB.
        let mut v = Vec::new();
        tone(&mut v, FS, 10.0, &[0.0]);
        let l = lu(&v, 1, FS, &[1.0]);
        assert!((l + 3.01).abs() <= 0.05, "{l}");
    }

    #[test]
    fn silence_and_short_input_have_no_loudness() {
        assert_eq!(integrated_lufs(&vec![0.0; 2 * 48_000 * 5], 2, FS, &STEREO).unwrap(), None);
        assert_eq!(integrated_lufs(&stereo(FS, &[(0.3, -23.0)]), 2, FS, &STEREO).unwrap(), None);
        assert_eq!(integrated_lufs(&[], 2, FS, &STEREO).unwrap(), None);
    }

    #[test]
    fn gain_changes_loudness_one_for_one() {
        let a = lu(&stereo(FS, &[(10.0, -30.0)]), 2, FS, &STEREO);
        let b = lu(&stereo(FS, &[(10.0, -24.0)]), 2, FS, &STEREO);
        assert!((b - a - 6.0).abs() < 1e-3, "{}", b - a);
    }

    #[test]
    fn bad_shapes_are_errors() {
        assert!(integrated_lufs(&[0.0; 3], 2, FS, &STEREO).is_err());
        assert!(integrated_lufs(&[0.0; 4], 2, FS, &[1.0]).is_err());
        assert!(integrated_lufs(&[0.0; 4], 0, FS, &[]).is_err());
        assert!(integrated_lufs(&[0.0; 4], 2, 100, &STEREO).is_err());
    }

    #[test]
    fn loudness_verdicts() {
        assert_eq!(compare_loudness(Some(-23.0), Some(-23.0), 0.5), Verdict::Identical);
        assert!(matches!(compare_loudness(Some(-23.0), Some(-23.3), 0.5), Verdict::Within { .. }));
        assert!(!compare_loudness(Some(-23.0), Some(-24.0), 0.5).passed());
        assert_eq!(compare_loudness(None, None, 0.5), Verdict::Identical);
        assert!(!compare_loudness(None, Some(-30.0), 0.5).passed());
    }
}
