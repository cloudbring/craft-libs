#![forbid(unsafe_code)]

use craft_scopes::{Accumulator, ColorStandard, Config, Error, FalseColour, Frame, Origin, Scale, ScopeKind};

const KINDS: [ScopeKind; 9] = [
    ScopeKind::WaveformRgb,
    ScopeKind::WaveformLuma,
    ScopeKind::WaveformYcEnvelope,
    ScopeKind::WaveformChroma,
    ScopeKind::ParadeRgb,
    ScopeKind::ParadeYuv,
    ScopeKind::ParadeRgbWhite,
    ScopeKind::Histogram,
    ScopeKind::Vectorscope,
];

fn config(kind: ScopeKind) -> Config {
    Config::new(kind, 64, if kind == ScopeKind::Histogram { 1 } else { 65 })
}

#[test]
fn red_lands_on_red_target() -> Result<(), Box<dyn std::error::Error>> {
    let pixels = [1.0, 0.0, 0.0].repeat(32 * 19);
    let frame = Frame::interleaved(32, 19, &pixels)?;
    for standard in [ColorStandard::Bt601, ColorStandard::Bt709, ColorStandard::Bt2020] {
        let mut options = Config::new(ScopeKind::Vectorscope, 255, 255);
        options.standard = standard;
        let (red, blue) = standard.coefficients();
        let target_cb = -red / (2.0 * (1.0 - blue));
        let (column, row) = options.vector_cell(target_cb, 0.5).ok_or("red target missing")?;
        let mut scope = Accumulator::new(options)?;
        scope.accumulate(frame)?;
        assert_eq!(scope.counts()[row * 255 + column], 32 * 19);
        assert_eq!(scope.counts().iter().sum::<u64>(), 32 * 19);
        if standard == ColorStandard::Bt709 {
            assert_eq!((column, row), (103, 21));
        }
    }
    Ok(())
}

#[test]
fn grey_ramp_is_diagonal_and_histogram_flat() -> Result<(), Box<dyn std::error::Error>> {
    let pixels: Vec<f32> = (0..256).flat_map(|value| [value as f32 / 255.0; 3]).collect();
    let frame = Frame::interleaved(256, 1, &pixels)?;
    for kind in [ScopeKind::WaveformRgb, ScopeKind::WaveformLuma, ScopeKind::ParadeRgb] {
        for origin in [Origin::Bottom, Origin::Top] {
            let mut options = Config::new(kind, 256, 256);
            options.origin = origin;
            let mut scope = Accumulator::new(options)?;
            scope.accumulate(frame)?;
            for plane in scope.counts().as_chunks::<{ 256 * 256 }>().0 {
                assert_eq!(plane.iter().sum::<u64>(), 256);
                for column in 0..256 {
                    let row = if origin == Origin::Bottom { column } else { 255 - column };
                    assert_eq!(plane[row * 256 + column], 1);
                }
            }
        }
    }
    let mut histogram = Accumulator::new(Config::new(ScopeKind::Histogram, 256, 1))?;
    histogram.accumulate(frame)?;
    assert!(histogram.counts().iter().all(|count| *count == 1));
    Ok(())
}

#[test]
fn planar_line_parallel_and_reuse_match_serial() -> Result<(), Box<dyn std::error::Error>> {
    let planes: [Vec<f32>; 3] = std::array::from_fn(|channel| (0..64 * 13).map(|index| ((index * 17 + channel * 3) % 255) as f32 / 255.0).collect());
    let packed: Vec<f32> = (0..64 * 13).flat_map(|index| [planes[0][index], planes[1][index], planes[2][index]]).collect();
    let planar = Frame::planar(64, 13, [&planes[0], &planes[1], &planes[2]])?;
    let interleaved = Frame::interleaved(64, 13, &packed)?;
    for kind in KINDS {
        let mut serial = Accumulator::new(config(kind))?;
        serial.accumulate(interleaved)?;
        let mut result = Accumulator::new(config(kind))?;
        let pointer = result.counts().as_ptr();
        let mut first = Accumulator::new(config(kind))?;
        let mut second = Accumulator::new(config(kind))?;
        for _ in 0..3 {
            first.clear();
            second.clear();
            std::thread::scope(|scope| {
                let worker = scope.spawn(|| first.accumulate_rows(planar, 0..5));
                second.accumulate_rows(planar, 5..13)?;
                worker.join().map_err(|_| Error::Configuration)?
            })?;
            result.clear();
            result.merge(&second)?;
            result.merge(&first)?;
            assert_eq!(serial.counts(), result.counts());
            assert_eq!(result.counts().as_ptr(), pointer);
        }
    }
    Ok(())
}

#[test]
fn nonfinite_pixels_are_skipped_without_phantom_bins() -> Result<(), Box<dyn std::error::Error>> {
    let pixels = [f32::NAN, 0.0, 0.0, 0.0, f32::INFINITY, 0.0, 0.0, 0.0, f32::NEG_INFINITY];
    let frame = Frame::interleaved(3, 1, &pixels)?;
    for kind in KINDS {
        let mut scope = Accumulator::new(config(kind))?;
        scope.accumulate(frame)?;
        assert_eq!(scope.skipped(), 3);
        assert!(scope.counts().iter().all(|count| *count == 0));
        scope.clear();
        assert_eq!(scope.skipped(), 0);
    }
    Ok(())
}

#[test]
fn histogram_clipping_diagnostics_and_endpoints() -> Result<(), Box<dyn std::error::Error>> {
    let pixels = [-0.2, -0.2, -0.2, 1.2, 1.2, 1.2, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0];
    let frame = Frame::interleaved(4, 1, &pixels)?;
    for clamp in [false, true] {
        let mut options = Config::new(ScopeKind::Histogram, 256, 1);
        options.clamp = clamp;
        let mut scope = Accumulator::new(options)?;
        scope.accumulate(frame)?;
        assert_eq!(scope.out_of_range(), ([1; 4], [1; 4]));
        for plane in scope.counts().as_chunks::<256>().0 {
            assert_eq!(plane[0], if clamp { 2 } else { 1 });
            assert_eq!(plane[255], if clamp { 2 } else { 1 });
        }
    }
    Ok(())
}

#[test]
fn invalid_input_is_rejected_without_mutation() -> Result<(), Box<dyn std::error::Error>> {
    assert!(matches!(Frame::interleaved(0, 1, &[]), Err(Error::Dimensions)));
    assert!(matches!(Frame::interleaved(usize::MAX, 2, &[]), Err(Error::Dimensions)));
    assert!(matches!(Frame::interleaved(2, 1, &[0.0; 5]), Err(Error::BufferLength)));
    assert!(matches!(Frame::planar(2, 1, [&[0.0; 2], &[0.0; 1], &[0.0; 2]]), Err(Error::BufferLength)));
    assert!(matches!(Accumulator::new(Config::new(ScopeKind::Histogram, usize::MAX, 1)), Err(Error::Dimensions)));
    for (width, height) in [(0, 1), (1, 1), (32, 0), (32, 2)] {
        assert!(Accumulator::new(Config::new(ScopeKind::Histogram, width, height)).is_err());
    }
    for bounds in [[0.0, 0.0], [1.0, 0.0], [f32::NAN, 1.0], [0.0, f32::INFINITY], [-f32::MAX, f32::MAX]] {
        let mut options = config(ScopeKind::WaveformRgb);
        options.range = bounds;
        assert!(Accumulator::new(options).is_err());
    }
    for extent in [0.0, -1.0, f32::NAN, f32::INFINITY] {
        let mut options = config(ScopeKind::Vectorscope);
        options.vector_extent = extent;
        assert!(Accumulator::new(options).is_err());
        assert_eq!(options.vector_cell(0.0, 0.0), None);
    }
    let mut scope = Accumulator::new(config(ScopeKind::WaveformRgb))?;
    let frame = Frame::interleaved(1, 1, &[0.5; 3])?;
    scope.accumulate(frame)?;
    let before = scope.counts().to_vec();
    assert_eq!(scope.accumulate_rows(frame, 0..2), Err(Error::Rows));
    assert_eq!(scope.accumulate_rows(frame, std::ops::Range { start: 1, end: 0 }), Err(Error::Rows));
    assert_eq!(scope.merge(&Accumulator::new(config(ScopeKind::Vectorscope))?), Err(Error::Incompatible));
    assert_eq!(scope.counts(), before);
    assert_eq!(scope.plane(usize::MAX), None);
    assert_eq!(scope.scaled_into(Scale::Linear, &mut []), Err(Error::BufferLength));
    Ok(())
}

#[test]
fn density_scaling_is_finite_and_preserves_counts() -> Result<(), Box<dyn std::error::Error>> {
    let mut scope = Accumulator::new(Config::new(ScopeKind::Histogram, 256, 1))?;
    scope.accumulate(Frame::interleaved(3, 1, &[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0])?)?;
    let before = scope.counts().to_vec();
    let mut output = vec![0.0; before.len()];
    for scale in [Scale::Linear, Scale::Logarithmic] {
        scope.scaled_into(scale, &mut output)?;
        assert!(output.iter().all(|value| (0.0..=1.0).contains(value)));
        assert_eq!(output[0], 1.0);
        assert_eq!(output[1], 0.0);
        assert_eq!(scale.intensity(u64::MAX, u64::MAX), 1.0);
        assert_eq!(scale.intensity(10, 0), 0.0);
        assert_eq!(scope.counts(), before);
    }
    assert_eq!(Scale::Linear.intensity(1, 2), 0.5);
    assert!(Scale::Logarithmic.intensity(1, 2) > 0.5);
    Ok(())
}

#[test]
fn false_colour_thresholds_and_parallel_chunks() -> Result<(), Box<dyn std::error::Error>> {
    let bands = FalseColour::new(&[0.0, 0.18, 0.5, 1.0], ColorStandard::Bt709)?;
    assert_eq!(bands.classify([1.0, 0.0, 0.0]), 2);
    assert_eq!(bands.classify([0.0; 3]), 1);
    assert_eq!(bands.classify([1.0; 3]), 4);
    assert_eq!(bands.classify([-0.1; 3]), 0);
    assert_eq!(bands.classify([f32::NAN; 3]), u16::MAX);
    assert!(FalseColour::new(&[1.0, 0.0], ColorStandard::Bt709).is_err());
    assert!(FalseColour::new(&[0.5, 0.5], ColorStandard::Bt709).is_err());
    assert!(FalseColour::new(&[f32::NAN], ColorStandard::Bt709).is_err());
    let pixels = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1.0, 1.0, f32::NAN, 0.0, 0.0];
    let frame = Frame::interleaved(2, 2, &pixels)?;
    let mut output = [99; 4];
    let (first, second) = output.split_at_mut(2);
    std::thread::scope(|scope| {
        let worker = scope.spawn(|| bands.map_rows(frame, 0..1, first));
        bands.map_rows(frame, 1..2, second)?;
        worker.join().map_err(|_| Error::Configuration)?
    })?;
    assert_eq!(output, [1, 2, 4, u16::MAX]);
    assert_eq!(bands.map_rows(frame, 0..1, &mut output), Err(Error::BufferLength));
    assert_eq!(output, [1, 2, 4, u16::MAX]);
    Ok(())
}

#[test]
fn dc_8k_and_overflow_saturate_without_wrapping() -> Result<(), Box<dyn std::error::Error>> {
    let (width, height) = (7680, 4320);
    let samples = (width * height) as u64;
    let plane = vec![0.5; width * height];
    let frame = Frame::planar(width, height, [&plane; 3])?;
    let mut histogram = Accumulator::new(Config::new(ScopeKind::Histogram, 256, 1))?;
    histogram.accumulate(frame)?;
    for trace in histogram.counts().as_chunks::<256>().0 {
        assert_eq!(trace[128], samples);
        assert_eq!(trace.iter().sum::<u64>(), samples);
    }
    let mut vector = Accumulator::new(Config::new(ScopeKind::Vectorscope, 65, 65))?;
    vector.accumulate(frame)?;
    assert_eq!(vector.counts()[32 * 65 + 32], samples);
    let mut waveform = Accumulator::new(Config::new(ScopeKind::WaveformLuma, width, 65))?;
    waveform.accumulate(frame)?;
    assert!(waveform.counts()[32 * width..33 * width].iter().all(|count| *count == height as u64));
    let mut copy = Accumulator::new(histogram.config())?;
    for _ in 0..40 {
        copy.clear();
        copy.merge(&histogram)?;
        histogram.merge(&copy)?;
    }
    assert!(histogram.saturated());
    assert_eq!(histogram.counts()[128], u64::MAX);
    assert_eq!(histogram.counts()[0], 0);
    copy.clear();
    copy.merge(&histogram)?;
    assert!(copy.saturated());
    histogram.clear();
    assert!(!histogram.saturated());
    assert!(histogram.counts().iter().all(|count| *count == 0));
    Ok(())
}
