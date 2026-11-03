use craft_blur::{Cancel, Edge, Error, Gaussian, Quality, Scratch, Shape};
use std::time::Instant;

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
