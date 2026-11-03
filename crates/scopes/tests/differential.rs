#![forbid(unsafe_code)]

use craft_scopes::{Accumulator, ColorStandard, Config, Frame, Origin, ScopeKind};

#[allow(dead_code, clippy::all)]
mod filmcraft {
    include!("reference/filmcraft.inc");
}

#[allow(dead_code, clippy::all)]
mod effectcraft {
    include!("reference/effectcraft.inc");
}

fn random_frame(seed: u64, width: usize, height: usize, extended: bool) -> Vec<[f32; 3]> {
    let mut state = seed;
    (0..width * height)
        .map(|_| {
            std::array::from_fn(|_| {
                state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                let value = (state >> 40) as f32 / 16_777_215.0;
                if extended { value * 1.5 - 0.25 } else { value }
            })
        })
        .collect()
}

fn compare(actual: &[u64], expected: &[u32], tolerance: u64) {
    assert_eq!(actual.len(), expected.len());
    for (index, (actual, expected)) in actual.iter().zip(expected).enumerate() {
        assert!(actual.abs_diff(u64::from(*expected)) <= tolerance, "bin {index}: {actual} != {expected}");
    }
}

#[test]
fn filmcraft_random_frames_exact() -> Result<(), Box<dyn std::error::Error>> {
    for seed in 1..=16 {
        let pixels = random_frame(seed, 64, 17, true);
        let signal = filmcraft::Signal { w: 64, h: 17, rgb: pixels.clone() };
        let frame = Frame::interleaved(64, 17, pixels.as_flattened())?;
        for (standard, matrix) in [
            (ColorStandard::Bt601, filmcraft::Matrix::Bt601),
            (ColorStandard::Bt709, filmcraft::Matrix::Bt709),
            (ColorStandard::Bt2020, filmcraft::Matrix::Bt2020Ncl),
        ] {
            for clamp in [false, true] {
                let params = filmcraft::Params { matrix, clamp, rows: 65, vector_size: 65 };
                for kind in [
                    ScopeKind::WaveformRgb,
                    ScopeKind::WaveformLuma,
                    ScopeKind::WaveformYcEnvelope,
                    ScopeKind::ParadeRgb,
                    ScopeKind::ParadeYuv,
                    ScopeKind::ParadeRgbWhite,
                ] {
                    let mut config = Config::new(kind, 64, 65);
                    config.standard = standard;
                    config.clamp = clamp;
                    config.range = if clamp { [0.0, 1.0] } else { [-0.1, 1.1] };
                    let mut actual = Accumulator::new(config)?;
                    actual.accumulate(frame)?;
                    let expected = match kind {
                        ScopeKind::WaveformRgb => filmcraft::waveform(&signal, filmcraft::WaveformType::Rgb, &params),
                        ScopeKind::WaveformLuma => filmcraft::waveform(&signal, filmcraft::WaveformType::Luma, &params),
                        ScopeKind::WaveformYcEnvelope => filmcraft::waveform(&signal, filmcraft::WaveformType::Yc, &params),
                        ScopeKind::ParadeRgb => filmcraft::parade(&signal, filmcraft::ParadeType::Rgb, &params),
                        ScopeKind::ParadeYuv => filmcraft::parade(&signal, filmcraft::ParadeType::Yuv, &params),
                        _ => filmcraft::parade(&signal, filmcraft::ParadeType::RgbWhite, &params),
                    };
                    for (trace, expected) in expected.traces.iter().enumerate() {
                        compare(actual.plane(trace).ok_or("missing trace")?, &expected.data, 0);
                    }
                }
                let mut config = Config::new(ScopeKind::Histogram, 256, 1);
                config.standard = standard;
                config.clamp = clamp;
                let mut actual = Accumulator::new(config)?;
                actual.accumulate(frame)?;
                let expected = filmcraft::histogram(&signal, &params);
                for (trace, expected) in [&expected.r, &expected.g, &expected.b, &expected.y].into_iter().enumerate() {
                    compare(actual.plane(trace).ok_or("missing histogram")?, expected, 0);
                }
                assert_eq!(actual.out_of_range(), (expected.below.map(u64::from), expected.above.map(u64::from)));
                config.kind = ScopeKind::Vectorscope;
                config.width = 65;
                config.height = 65;
                let mut actual = Accumulator::new(config)?;
                actual.accumulate(frame)?;
                compare(actual.counts(), &filmcraft::vectorscope_yuv(&signal, &params).data, 0);
            }
        }
    }
    Ok(())
}

fn compare_effectcraft(
    pixels: &[[f32; 3]],
    standard: ColorStandard,
    reference: effectcraft::ColorStandard,
    clamp: bool,
    float: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let frame = Frame::interleaved(64, 17, pixels.as_flattened())?;
    for (kind, reference_kind) in [
        (ScopeKind::WaveformRgb, effectcraft::ScopeKind::WaveformRgb),
        (ScopeKind::WaveformLuma, effectcraft::ScopeKind::WaveformLuma),
        (ScopeKind::WaveformChroma, effectcraft::ScopeKind::WaveformYc),
        (ScopeKind::ParadeRgb, effectcraft::ScopeKind::ParadeRgb),
        (ScopeKind::ParadeYuv, effectcraft::ScopeKind::ParadeYuv),
        (ScopeKind::Histogram, effectcraft::ScopeKind::Histogram),
        (ScopeKind::Vectorscope, effectcraft::ScopeKind::VectorscopeYuv),
    ] {
        let parade = matches!(kind, ScopeKind::ParadeRgb | ScopeKind::ParadeYuv);
        let width = match kind {
            ScopeKind::Histogram => 256,
            ScopeKind::Vectorscope => 65,
            _ => 64,
        };
        let mut config = Config::new(kind, width, if kind == ScopeKind::Histogram { 1 } else { 65 });
        config.standard = standard;
        config.clamp = clamp;
        config.origin = Origin::Top;
        config.range = if float { [-0.25, 1.25] } else { [0.0, 1.0] };
        config.vector_extent = 0.5;
        let options = effectcraft::ScopeOpts { standard: reference, clamp, float, width: if parade { 192 } else { width as u32 }, height: 65 };
        let expected = effectcraft::compute(pixels, 64, reference_kind, &options);
        let mut actual = Accumulator::new(config)?;
        actual.accumulate(frame)?;
        for trace in 0..kind.traces().len().min(3) {
            let source_plane = match (kind, trace) {
                (ScopeKind::WaveformChroma, 1) => 2,
                (ScopeKind::ParadeYuv, 0 | 2) => 0,
                (ScopeKind::ParadeYuv, 1) => 2,
                _ => trace,
            };
            let source_width = options.width as usize;
            let mut extracted = Vec::new();
            for row in 0..config.height {
                let offset = row * source_width + if parade { trace * width } else { 0 };
                extracted.extend_from_slice(&expected.counts[source_plane][offset..offset + width]);
            }
            compare(actual.plane(trace).ok_or("missing plane")?, &extracted, 0);
        }
    }
    Ok(())
}

#[test]
fn effectcraft_random_nominal_frames_exact() -> Result<(), Box<dyn std::error::Error>> {
    for seed in 1..=16 {
        let pixels = random_frame(seed, 64, 17, false);
        for (standard, reference) in [
            (ColorStandard::Bt601, effectcraft::ColorStandard::Rec601),
            (ColorStandard::Bt709, effectcraft::ColorStandard::Rec709),
            (ColorStandard::Bt2020, effectcraft::ColorStandard::Rec2020),
        ] {
            for clamp in [false, true] {
                for float in [false, true] {
                    compare_effectcraft(&pixels, standard, reference, clamp, float)?;
                }
            }
        }
    }
    Ok(())
}

#[test]
fn effectcraft_extended_clamped_frames_exact() -> Result<(), Box<dyn std::error::Error>> {
    for seed in 1..=16 {
        let pixels = random_frame(seed, 64, 17, true);
        for float in [false, true] {
            compare_effectcraft(&pixels, ColorStandard::Bt709, effectcraft::ColorStandard::Rec709, true, float)?;
        }
    }
    Ok(())
}

#[test]
fn yc_implementations_are_not_equivalent() -> Result<(), Box<dyn std::error::Error>> {
    let pixels = [[1.0, 0.0, 0.0]; 64 * 17];
    let frame = Frame::interleaved(64, 17, pixels.as_flattened())?;
    let mut envelope = Accumulator::new(Config::new(ScopeKind::WaveformYcEnvelope, 64, 256))?;
    let mut amplitude = Accumulator::new(Config::new(ScopeKind::WaveformChroma, 64, 256))?;
    envelope.accumulate(frame)?;
    amplitude.accumulate(frame)?;
    assert_ne!(envelope.plane(1), amplitude.plane(1));
    assert_eq!(envelope.plane(0), amplitude.plane(0));
    Ok(())
}

#[test]
fn effectcraft_column_rounding_differs_by_at_most_one_count() -> Result<(), Box<dyn std::error::Error>> {
    for width in [22, 37, 79] {
        for seed in 1..=16 {
            let pixels = random_frame(seed, width, 1, false);
            let frame = Frame::interleaved(width, 1, pixels.as_flattened())?;
            let mut config = Config::new(ScopeKind::WaveformRgb, width, 65);
            config.origin = Origin::Top;
            let mut actual = Accumulator::new(config)?;
            actual.accumulate(frame)?;
            let options = effectcraft::ScopeOpts { width: width as u32, height: 65, ..Default::default() };
            let expected = effectcraft::compute(&pixels, width, effectcraft::ScopeKind::WaveformRgb, &options);
            for trace in 0..3 {
                compare(actual.plane(trace).ok_or("missing trace")?, &expected.counts[trace], 1);
            }
            let signal = filmcraft::Signal { w: width, h: 1, rgb: pixels.clone() };
            let params = filmcraft::Params { rows: 65, ..Default::default() };
            let expected = filmcraft::waveform(&signal, filmcraft::WaveformType::Rgb, &params);
            config.origin = Origin::Bottom;
            let mut actual = Accumulator::new(config)?;
            actual.accumulate(frame)?;
            for (trace, grid) in expected.traces.iter().enumerate() {
                compare(actual.plane(trace).ok_or("missing trace")?, &grid.data, 0);
            }
        }
    }
    Ok(())
}

#[test]
fn exact_column_mapping_fixes_effectcraft_concentrated_column_drift() -> Result<(), Box<dyn std::error::Error>> {
    let pixels = [[0.5; 3]; 22 * 3];
    let frame = Frame::interleaved(22, 3, pixels.as_flattened())?;
    let mut options = Config::new(ScopeKind::WaveformRgb, 22, 65);
    options.origin = Origin::Top;
    let mut actual = Accumulator::new(options)?;
    actual.accumulate(frame)?;
    let reference_options = effectcraft::ScopeOpts { width: 22, height: 65, ..Default::default() };
    let expected = effectcraft::compute(&pixels, 22, effectcraft::ScopeKind::WaveformRgb, &reference_options);
    assert_eq!(actual.counts()[32 * 22 + 13], 3);
    assert_eq!(expected.counts[0][32 * 22 + 13], 0);
    assert_eq!(expected.counts[0][32 * 22 + 12], 6);
    Ok(())
}
