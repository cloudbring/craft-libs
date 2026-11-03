use craft_blur::{Cancel, Edge, Error, Gaussian, Quality, Scratch, Shape};
use std::time::Instant;

#[path = "../src/lines/tests/lightcraft.rs"]
mod lightcraft;

#[test]
#[ignore = "same-binary upstream comparison and performance regression gate; release only"]
fn bench_lightcraft_comparison() -> Result<(), Error> {
    comparison::<1>()?;
    comparison::<3>()?;
    comparison::<4>()
}

fn comparison<const CHANNELS: usize>() -> Result<(), Error>
where
    [f32; CHANNELS]: Default,
{
    let shape = Shape::new(6000, 4000, CHANNELS)?;
    let input: Vec<_> = (0..shape.samples()).map(|index| (index % 1021) as f32 / 1021.0).collect();
    let source = lightcraft::Image { width: 6000, height: 4000, data: input.as_chunks::<CHANNELS>().0.to_vec() };
    let mut scratch = Scratch::default();
    for sigma in [1.0, 8.0, 64.0] {
        let filter = Gaussian::new(sigma, Quality::Recursive)?;
        let upstream = lightcraft::Gaussian::new(sigma);
        let mut craft_times = Vec::new();
        let mut upstream_times = Vec::new();
        for iteration in 0..9 {
            let mut pixels = input.clone();
            let mut craft = || -> Result<f64, Error> {
                let start = Instant::now();
                filter.apply(&mut pixels, shape, Edge::Clamp, &mut scratch, Cancel::Never)?;
                Ok(start.elapsed().as_secs_f64())
            };
            let reference = || {
                let start = Instant::now();
                let expected = upstream.filter_image(&source, |pixel| pixel.map(f64::from), |pixel| pixel.map(|value| value as f32));
                (expected, start.elapsed().as_secs_f64())
            };
            let (craft_time, (expected, upstream_time)) = if iteration % 2 == 0 {
                let craft_time = craft()?;
                (craft_time, reference())
            } else {
                let expected = reference();
                (craft()?, expected)
            };
            assert!(pixels.iter().zip(expected.data.iter().flatten()).all(|(actual, expected)| actual.to_bits() == expected.to_bits()));
            if iteration >= 2 {
                craft_times.push(craft_time);
                upstream_times.push(upstream_time);
            }
        }
        craft_times.sort_by(f64::total_cmp);
        upstream_times.sort_by(f64::total_cmp);
        let ratio = craft_times[3] / upstream_times[3];
        eprintln!(
            "24 MP {CHANNELS}ch sigma={sigma}: craft={:.2} ms, lightcraft={:.2} ms, ratio={ratio:.3}",
            craft_times[3] * 1000.0,
            upstream_times[3] * 1000.0
        );
        if CHANNELS == 1 {
            assert!(ratio <= 1.10, "single-channel performance regression: {ratio:.3} > 1.10");
        }
    }
    Ok(())
}

fn bench(quality: Quality) -> Result<(), Error> {
    let shape = Shape::new(6000, 4000, 1)?;
    let input: Vec<_> = (0..shape.samples()).map(|index| (index % 1021) as f32 / 1021.0).collect();
    let mut scratch = Scratch::default();
    for sigma in [1.0, 8.0, 64.0] {
        let filter = Gaussian::new(sigma, quality)?;
        let mut pixels = input.clone();
        let start = Instant::now();
        filter.apply(&mut pixels, shape, Edge::Clamp, &mut scratch, Cancel::Never)?;
        eprintln!(
            "24 MP, 1 channel, serial, {quality:?}, sigma={sigma}: {:.3}s; checksum={:.6}",
            start.elapsed().as_secs_f64(),
            std::hint::black_box(pixels[12_000_000])
        );
    }
    Ok(())
}

#[test]
#[ignore = "24 MP release timing; run --release --test bench -- --ignored --nocapture --test-threads=1"]
fn bench_exact_24mp() -> Result<(), Error> {
    bench(Quality::Exact)
}

#[test]
#[ignore = "24 MP release timing; run --release --test bench -- --ignored --nocapture --test-threads=1"]
fn bench_recursive_24mp() -> Result<(), Error> {
    bench(Quality::Recursive)
}

#[test]
#[ignore = "24 MP release timing; run --release --test bench -- --ignored --nocapture --test-threads=1"]
fn bench_box_24mp() -> Result<(), Error> {
    bench(Quality::BoxApprox { passes: 3 })
}
