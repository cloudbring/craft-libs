use craft_blur::{Cancel, Edge, Error, Gaussian, Quality, Scratch, Separable, Shape};

#[path = "../src/lines/tests/lightcraft.rs"]
mod lightcraft;

fn compare<const CHANNELS: usize>() -> Result<(usize, usize), Error>
where
    [f32; CHANNELS]: Default,
{
    let mut cases = 0;
    let mut samples = 0;
    let mut scratch = Scratch::default();
    for seed in [1_u32, 17, 7919, u32::MAX] {
        let mut state = seed;
        for (width, height) in [(1, 1), (1, 19), (19, 1), (2, 3), (7, 9), (8, 8), (17, 19), (31, 11), (65, 67), (129, 131)] {
            let shape = Shape::new(width, height, CHANNELS)?;
            let input: Vec<_> = (0..shape.samples())
                .map(|_| {
                    state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                    (state >> 8) as f32 / 16_777_216.0 * 20.0 - 10.0
                })
                .collect();
            let source = lightcraft::Image { width, height, data: input.as_chunks::<CHANNELS>().0.to_vec() };
            for sigma in [0.0, 0.01, 0.1, 0.5, 1.0, 2.0, 2.499, 2.5, 3.0, 3.555, 3.556, 4.0, 5.0, 6.0, 7.0, 7.99, 8.0, 16.0, 32.0, 64.0, 65_536.0] {
                let expected =
                    lightcraft::Gaussian::new(sigma).filter_image(&source, |pixel| pixel.map(f64::from), |pixel| pixel.map(|value| value as f32));
                let mut actual = input.clone();
                let filter = Gaussian::new(sigma, Quality::RecursiveRaw)?;
                assert_eq!(filter.quality(), Quality::RecursiveRaw);
                assert_eq!(matches!(filter.separable(), Separable::Recursive(_)), sigma >= 2.5);
                filter.apply(&mut actual, shape, Edge::Clamp, &mut scratch, Cancel::Never)?;
                let mut split = input.clone();
                filter.filter_rows(&mut split, shape, Edge::Clamp, &mut scratch, Cancel::Never)?;
                let mut rows: Vec<_> = split.chunks_exact_mut(width * CHANNELS).collect();
                filter.filter_columns(&mut rows, CHANNELS, Edge::Clamp, &mut scratch, Cancel::Never)?;
                for output in [&actual, &split] {
                    let differing_bits =
                        output.iter().zip(expected.data.iter().flatten()).filter(|(actual, expected)| actual.to_bits() != expected.to_bits()).count();
                    assert_eq!(differing_bits, 0, "seed={seed}, {width}x{height}, channels={CHANNELS}, sigma={sigma}");
                    cases += 1;
                    samples += output.len();
                }
            }
        }
    }
    Ok((cases, samples))
}

#[test]
fn public_recursive_raw_matches_verbatim_lightcraft_v3() -> Result<(), Error> {
    let results = [compare::<1>()?, compare::<2>()?, compare::<3>()?, compare::<4>()?];
    eprintln!(
        "LightCraft V3 differential: {} cases, {} samples, 0 differing bits",
        results.iter().map(|result| result.0).sum::<usize>(),
        results.iter().map(|result| result.1).sum::<usize>()
    );
    Ok(())
}
