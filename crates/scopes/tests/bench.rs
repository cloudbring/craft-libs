#![forbid(unsafe_code)]

use craft_scopes::{Accumulator, ColorStandard, Config, FalseColour, Frame, ScopeKind};
use std::{hint::black_box, time::Instant};

fn pixels() -> Vec<f32> {
    let mut state = 109_u64;
    (0..3840 * 2160 * 3)
        .map(|_| {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            (state >> 40) as f32 / 16_777_215.0
        })
        .collect()
}

fn bench(kind: ScopeKind) -> Result<(), Box<dyn std::error::Error>> {
    let pixels = pixels();
    let frame = Frame::interleaved(3840, 2160, &pixels)?;
    let mut accumulator = Accumulator::new(Config::new(kind, 256, if kind == ScopeKind::Histogram { 1 } else { 256 }))?;
    accumulator.accumulate(black_box(frame))?;
    let start = Instant::now();
    for _ in 0..3 {
        accumulator.clear();
        accumulator.accumulate(black_box(frame))?;
        black_box(accumulator.counts());
    }
    eprintln!("{kind:?}: {:.3} ms/4K frame (3 iterations, single thread, allocation excluded)", start.elapsed().as_secs_f64() * 1000.0 / 3.0);
    Ok(())
}

macro_rules! benchmark {
    ($name:ident, $kind:ident) => {
        #[test]
        #[ignore = "4K microbenchmark; run release with --ignored --nocapture --test-threads=1"]
        fn $name() -> Result<(), Box<dyn std::error::Error>> {
            bench(ScopeKind::$kind)
        }
    };
}

benchmark!(waveform_rgb, WaveformRgb);
benchmark!(waveform_luma, WaveformLuma);
benchmark!(waveform_yc_envelope, WaveformYcEnvelope);
benchmark!(waveform_chroma, WaveformChroma);
benchmark!(parade_rgb, ParadeRgb);
benchmark!(parade_yuv, ParadeYuv);
benchmark!(parade_rgb_white, ParadeRgbWhite);
benchmark!(histogram, Histogram);
benchmark!(vectorscope, Vectorscope);

#[test]
#[ignore = "4K microbenchmark; run release with --ignored --nocapture --test-threads=1"]
fn false_colour() -> Result<(), Box<dyn std::error::Error>> {
    let pixels = pixels();
    let frame = Frame::interleaved(3840, 2160, &pixels)?;
    let bands = FalseColour::new(&[0.0, 0.1, 0.18, 0.5, 0.9, 1.0], ColorStandard::Bt709)?;
    let mut output = vec![0; 3840 * 2160];
    bands.map_rows(frame, 0..2160, &mut output)?;
    let start = Instant::now();
    for _ in 0..3 {
        bands.map_rows(black_box(frame), 0..2160, black_box(&mut output))?;
    }
    eprintln!("FalseColour: {:.3} ms/4K frame (3 iterations, single thread, allocation excluded)", start.elapsed().as_secs_f64() * 1000.0 / 3.0);
    Ok(())
}
