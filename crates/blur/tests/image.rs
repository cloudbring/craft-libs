use craft_blur::{Cancel, Edge, Error, Gaussian, Quality, Scratch, Separable, Shape};
use proptest::prelude::*;
use std::sync::atomic::{AtomicUsize, Ordering};

#[path = "../src/lines/tests/lightcraft.rs"]
mod lightcraft;

fn compare<const CHANNELS: usize>(width: usize, height: usize, sigma: f32, samples: &[f32]) -> Result<(), Error>
where
    [f32; CHANNELS]: Default,
{
    let shape = Shape::new(width, height, CHANNELS)?;
    let mut pixels: Vec<_> = (0..shape.samples()).map(|index| samples[index % samples.len()]).collect();
    let source = lightcraft::Image { width, height, data: pixels.as_chunks::<CHANNELS>().0.to_vec() };
    let expected = lightcraft::Gaussian::new(sigma).filter_image(&source, |pixel| pixel.map(f64::from), |pixel| pixel.map(|value| value as f32));
    let filter = Gaussian::new(sigma, Quality::Recursive)?;
    if sigma < 2.5 {
        assert!(matches!(filter.separable(), Separable::Kernel(_)));
    }
    filter.apply(&mut pixels, shape, Edge::Clamp, &mut Scratch::default(), Cancel::Never)?;
    assert!(pixels.iter().zip(expected.data.iter().flatten()).all(|(actual, expected)| actual.to_bits() == expected.to_bits()));
    Ok(())
}

#[test]
fn verbatim_lightcraft_image_path_is_bit_identical() -> Result<(), Error> {
    for (width, height) in [(1, 1), (1, 65), (2, 3), (7, 9), (17, 19), (31, 63), (31, 64), (31, 65), (19, 129)] {
        for sigma in [0.5, 1.0, 2.0, 2.499, 8.0, 16.0, 64.0] {
            let samples = [-16.0, 0.0, 0.1, -0.25, 100.0, 1.0, 1e-6];
            compare::<1>(width, height, sigma, &samples)?;
            compare::<2>(width, height, sigma, &samples)?;
            compare::<3>(width, height, sigma, &samples)?;
            compare::<4>(width, height, sigma, &samples)?;
        }
    }
    Ok(())
}

#[test]
fn sampled_tiles_and_large_halos_match_line_api() -> Result<(), Error> {
    for channels in 1..=4 {
        for height in [63, 64, 65, 127, 128, 129, 193, 257] {
            let shape = Shape::new(9, height, channels)?;
            let input: Vec<_> = (0..shape.samples()).map(|index| (index % 91) as f32 - 21.0).collect();
            let mut scratch = Scratch::default();
            for sigma in [0.0, 0.5, 1.0, 2.499, 4.0, 64.0] {
                let filter = Gaussian::new(sigma, Quality::Exact)?;
                let mut actual = input.clone();
                filter.apply(&mut actual, shape, Edge::Clamp, &mut scratch, Cancel::Never)?;
                let mut expected = input.clone();
                filter.filter_rows(&mut expected, shape, Edge::Clamp, &mut scratch, Cancel::Never)?;
                let mut rows: Vec<_> = expected.chunks_mut(shape.width() * channels).collect();
                filter.filter_columns(&mut rows, channels, Edge::Clamp, &mut scratch, Cancel::Never)?;
                assert_eq!(actual, expected, "channels={channels}, height={height}, sigma={sigma}");
            }
        }
    }
    Ok(())
}

#[test]
fn sampled_apply_cancels_within_eight_output_rows() -> Result<(), Error> {
    let shape = Shape::new(17, 129, 1)?;
    let input: Vec<_> = (0..shape.samples()).map(|index| (index % 29) as f32).collect();
    for quality in [Quality::Exact, Quality::Recursive] {
        let filter = Gaussian::new(1.0, quality)?;
        let mut horizontal = input.clone();
        filter.filter_rows(&mut horizontal, shape, Edge::Clamp, &mut Scratch::default(), Cancel::Never)?;
        let calls = AtomicUsize::new(0);
        let horizontal_batches = shape.height().div_ceil(8);
        let hook = || calls.fetch_add(1, Ordering::Relaxed) > horizontal_batches + 1;
        let mut actual = input.clone();
        assert_eq!(filter.apply(&mut actual, shape, Edge::Clamp, &mut Scratch::default(), Cancel::Hook(&hook)), Err(Error::Cancelled));
        assert_eq!(&actual[8 * shape.width()..], &horizontal[8 * shape.width()..]);
        assert_ne!(&actual[..8 * shape.width()], &horizontal[..8 * shape.width()]);
        assert_eq!(calls.load(Ordering::Relaxed), horizontal_batches + 3);
    }
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(96))]
    #[test]
    fn random_planes_match_verbatim_image_kernel(
        width in 1usize..40, height in 1usize..150, sigma_index in 0usize..4,
        samples in proptest::collection::vec(-100.0f32..100.0, 1..128)
    ) {
        let sigma = [1.0, 2.0, 8.0, 64.0][sigma_index];
        compare::<1>(width, height, sigma, &samples)?;
        compare::<2>(width, height, sigma, &samples)?;
        compare::<3>(width, height, sigma, &samples)?;
        compare::<4>(width, height, sigma, &samples)?;
    }
}
