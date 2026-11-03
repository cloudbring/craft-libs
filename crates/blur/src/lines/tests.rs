use super::super::{Cancel, Edge, Gaussian, Quality, Recursive, Scratch, Separable, Shape};
use proptest::prelude::*;

mod lightcraft;

fn reference<const CHANNELS: usize>(input: &[f32], width: usize, sigma: f32) -> Vec<f32> {
    let filter = lightcraft::Gaussian::new(sigma);
    let mut pixels: Vec<[f32; CHANNELS]> = input.as_chunks::<CHANNELS>().0.to_vec();
    for row in pixels.chunks_mut(width) {
        filter.filter(row, |pixel| pixel.map(f64::from), |pixel| pixel.map(|value| value as f32));
    }
    for column in 0..width {
        let mut line: Vec<_> = pixels.chunks_exact(width).map(|row| row[column]).collect();
        filter.filter(&mut line, |pixel| pixel.map(f64::from), |pixel| pixel.map(|value| value as f32));
        for (row, pixel) in pixels.chunks_exact_mut(width).zip(line) {
            row[column] = pixel;
        }
    }
    pixels.into_iter().flatten().collect()
}

fn compare(input: &[f32], shape: Shape, sigma: f32, raw: bool) -> Result<(), crate::Error> {
    let expected = match shape.channels() {
        1 => reference::<1>(input, shape.width(), sigma),
        2 => reference::<2>(input, shape.width(), sigma),
        3 => reference::<3>(input, shape.width(), sigma),
        _ => reference::<4>(input, shape.width(), sigma),
    };
    let filter = if raw {
        Gaussian { sigma, quality: Quality::Recursive, passes: Separable::Recursive(Recursive::new(sigma)?) }
    } else {
        Gaussian::new(sigma, Quality::Recursive)?
    };
    let mut actual = input.to_vec();
    filter.apply(&mut actual, shape, Edge::Clamp, &mut Scratch::default(), Cancel::Never)?;
    assert_eq!(actual.iter().map(|value| value.to_bits()).collect::<Vec<_>>(), expected.iter().map(|value| value.to_bits()).collect::<Vec<_>>());
    Ok(())
}

#[test]
fn verbatim_lightcraft_recursive_path_is_bit_identical() -> Result<(), crate::Error> {
    for channels in 1..=4 {
        for (width, height) in [(1, 1), (1, 19), (2, 3), (7, 9), (17, 19), (31, 11)] {
            let shape = Shape::new(width, height, channels)?;
            let input: Vec<_> = (0..shape.samples()).map(|index| ((index * 7919 % 997) as f32 - 411.0) / 128.0).collect();
            for sigma in [2.5, 4.0, 8.0, 16.0, 32.0, 64.0] {
                compare(&input, shape, sigma, true)?;
                if sigma >= 8.0 {
                    compare(&input, shape, sigma, false)?;
                }
            }
            for sigma in [0.5, 1.0, 2.0] {
                compare(&input, shape, sigma, false)?;
            }
        }
    }
    Ok(())
}

#[test]
fn raw_upstream_shape_failures_require_the_guard_band() -> Result<(), crate::Error> {
    for sigma in [2.5, 4.0] {
        let shape = Shape::new(513, 1, 1)?;
        let mut exact = vec![0.0; shape.samples()];
        exact[256] = 1.0;
        let raw = reference::<1>(&exact, 513, sigma);
        Gaussian::new(sigma, Quality::Exact)?.apply(&mut exact, shape, Edge::Clamp, &mut Scratch::default(), Cancel::Never)?;
        let error = exact.iter().zip(raw).map(|(left, right)| (left - right).abs()).fold(0.0f32, f32::max);
        assert!(error > 0.01, "sigma={sigma}, raw YvV error={error}");
    }
    Ok(())
}

#[test]
fn scratch_capacities_are_reused_between_batches_and_calls() -> Result<(), crate::Error> {
    for quality in [Quality::Exact, Quality::Recursive, Quality::BoxApprox { passes: 3 }] {
        let filter = Gaussian::new(8.0, quality)?;
        let mut scratch = Scratch::default();
        let mut input = vec![0.25; 31 * 19 * 4];
        let shape = Shape::new(31, 19, 4)?;
        filter.apply(&mut input, shape, Edge::Clamp, &mut scratch, Cancel::Never)?;
        let capacities = [scratch.input.capacity(), scratch.output.capacity(), scratch.forward.capacity()];
        for _ in 0..3 {
            filter.apply(&mut input, shape, Edge::Clamp, &mut scratch, Cancel::Never)?;
            assert_eq!([scratch.input.capacity(), scratch.output.capacity(), scratch.forward.capacity()], capacities);
        }
    }
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(96))]
    #[test]
    fn random_planes_match_lightcraft(
        width in 1usize..24, height in 1usize..24, channels in 1usize..5,
        sigma_index in 0usize..4, samples in proptest::collection::vec(-10.0f32..10.0, 1..128)
    ) {
        let sigma = [8.0, 16.0, 32.0, 64.0][sigma_index];
        let shape = Shape::new(width, height, channels)?;
        let input: Vec<_> = (0..shape.samples()).map(|index| samples[index % samples.len()]).collect();
        compare(&input, shape, sigma, false)?;
    }
}
