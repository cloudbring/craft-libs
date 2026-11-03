use craft_blur::{Cancel, Edge, Error, Gaussian, LINE_BATCH, Quality, Scratch, Separable, Shape, box_blur, box_widths, gaussian_kernel};
use proptest::prelude::*;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

const SIGMAS: [f32; 9] = [0.5, 1.0, 2.0, 2.5, 4.0, 8.0, 16.0, 32.0, 64.0];
const QUALITIES: [Quality; 3] = [Quality::Exact, Quality::Recursive, Quality::BoxApprox { passes: 3 }];

fn impulse(sigma: f32, quality: Quality) -> Result<Vec<f32>, Error> {
    let half = (32.0 * sigma).ceil().max(64.0) as usize;
    let mut input = vec![0.0; 2 * half + 1];
    input[half] = 1.0;
    let shape = Shape::new(input.len(), 1, 1)?;
    Gaussian::new(sigma, quality)?.filter_rows(&mut input, shape, Edge::Clamp, &mut Scratch::default(), Cancel::Never)?;
    Ok(input)
}

fn variance(kernel: &[f32]) -> f64 {
    let center = (kernel.len() / 2) as f64;
    let mass: f64 = kernel.iter().map(|value| f64::from(*value)).sum();
    kernel.iter().enumerate().map(|(index, value)| f64::from(*value) * (index as f64 - center).powi(2)).sum::<f64>() / mass
}

#[test]
fn exact_and_recursive_variance_dc_symmetry() -> Result<(), Error> {
    for sigma in SIGMAS {
        for quality in [Quality::Exact, Quality::Recursive] {
            let kernel = impulse(sigma, quality)?;
            let relative = (variance(&kernel) / f64::from(sigma).powi(2) - 1.0).abs();
            assert!(relative < 0.005, "sigma={sigma}, {quality:?}, variance error={relative}");
            assert!((kernel.iter().map(|value| f64::from(*value)).sum::<f64>() - 1.0).abs() < 1e-6);
            assert!(kernel.iter().zip(kernel.iter().rev()).all(|(left, right)| (left - right).abs() < 1e-7));
        }
    }
    Ok(())
}

#[test]
fn recursive_shape_bound_against_exact_is_enforced() -> Result<(), Error> {
    for sigma in [2.5, 3.0, 4.0, 5.0, 6.0, 7.9, 8.0, 8.1, 12.0, 16.0, 32.0, 64.0] {
        let exact = impulse(sigma, Quality::Exact)?;
        let recursive = impulse(sigma, Quality::Recursive)?;
        let error = exact.iter().zip(recursive).map(|(left, right)| (left - right).abs()).fold(0.0f32, f32::max);
        assert!(error <= 1e-2, "sigma={sigma}, max absolute impulse error={error}");
    }
    assert!(matches!(Gaussian::new(4.0, Quality::Recursive)?.separable(), Separable::Kernel(_)));
    assert!(matches!(Gaussian::new(8.0, Quality::Recursive)?.separable(), Separable::Recursive(_)));
    Ok(())
}

#[test]
fn sampled_kernel_has_no_old_33_pixel_limit() -> Result<(), Error> {
    for sigma in [0.01, 0.1, 0.5, 1.0, 33.0, 64.0, 128.0] {
        let kernel = gaussian_kernel(sigma)?;
        assert_eq!(kernel.len(), 2 * (3.0 * sigma).ceil() as usize + 1);
        assert!((variance(&kernel) / f64::from(sigma).powi(2) - 1.0).abs() < 0.005);
    }
    assert_eq!(gaussian_kernel(0.0)?, vec![1.0]);
    Ok(())
}

#[test]
fn kovesi_variance_bound_and_subpixel_rejection() -> Result<(), Error> {
    for sigma in [1.0, 2.0, 2.5, 4.0, 8.0, 16.0, 32.0, 64.0] {
        let output = impulse(sigma, Quality::BoxApprox { passes: 3 })?;
        let actual = (variance(&output) / f64::from(sigma).powi(2) - 1.0).abs();
        let bound = 1.0 / (3.0 * f64::from(sigma));
        assert!(actual <= bound + 1e-6, "sigma={sigma}, {actual} > {bound}");
    }
    for step in 1..=6400 {
        let sigma = step as f32 / 100.0;
        let widths = box_widths(sigma)?;
        assert!(widths.iter().all(|width| width % 2 == 1));
        let variance: f64 = widths.iter().map(|width| ((*width as f64).powi(2) - 1.0) / 12.0).sum();
        let actual = (variance / f64::from(sigma).powi(2) - 1.0).abs();
        let bound = 1.0 / (3.0 * f64::from(sigma));
        assert_eq!(Gaussian::new(sigma, Quality::BoxApprox { passes: 3 }).is_ok(), actual <= bound + 1e-14);
    }
    assert!(matches!(Gaussian::new(0.5, Quality::BoxApprox { passes: 3 }), Err(Error::BoxTolerance { .. })));
    Ok(())
}

fn index(position: isize, length: usize, edge: Edge) -> Option<usize> {
    if position >= 0 && position < length as isize {
        return Some(position as usize);
    }
    match edge {
        Edge::Clamp => Some(position.clamp(0, length as isize - 1) as usize),
        Edge::Mirror => {
            let mut reflected = position;
            while reflected < 0 || reflected >= length as isize {
                reflected = if reflected < 0 { -reflected - 1 } else { 2 * length as isize - 1 - reflected };
            }
            Some(reflected as usize)
        }
        _ => None,
    }
}

fn reference_axis(input: &[f32], shape: Shape, kernel: &[f32], edge: Edge, horizontal: bool) -> Vec<f32> {
    let mut output = vec![0.0; input.len()];
    for row in 0..shape.height() {
        for column in 0..shape.width() {
            for channel in 0..shape.channels() {
                let mut sum = 0.0;
                for (tap, weight) in kernel.iter().enumerate() {
                    let offset = tap as isize - (kernel.len() / 2) as isize;
                    let sample_row = index(row as isize + if horizontal { 0 } else { offset }, shape.height(), edge);
                    let sample_column = index(column as isize + if horizontal { offset } else { 0 }, shape.width(), edge);
                    if let Some((sample_row, sample_column)) = sample_row.zip(sample_column) {
                        sum += f64::from(input[(sample_row * shape.width() + sample_column) * shape.channels() + channel]) * f64::from(*weight);
                    }
                }
                output[(row * shape.width() + column) * shape.channels() + channel] = sum as f32;
            }
        }
    }
    output
}

fn reference(input: &[f32], shape: Shape, kernels: &[Vec<f32>], edge: Edge) -> Vec<f32> {
    let mut output = input.to_vec();
    for horizontal in [true, false] {
        for kernel in kernels {
            output = reference_axis(&output, shape, kernel, edge, horizontal);
        }
    }
    output
}

fn close(actual: &[f32], expected: &[f32], tolerance: f32) {
    assert_eq!(actual.len(), expected.len());
    for (actual, expected) in actual.iter().zip(expected) {
        assert!((actual - expected).abs() <= tolerance, "{actual} != {expected}");
    }
}

#[test]
fn small_images_and_all_edges_match_brute_force() -> Result<(), Error> {
    for channels in 1..=4 {
        for (width, height) in [(1, 1), (1, 3), (3, 1), (3, 4), (9, 11)] {
            let shape = Shape::new(width, height, channels)?;
            let input: Vec<_> = (0..shape.samples()).map(|index| (index % 13) as f32 / 13.0).collect();
            for edge in [Edge::Clamp, Edge::Mirror, Edge::Zero, Edge::Transparent] {
                if edge == Edge::Transparent && channels != 2 && channels != 4 {
                    continue;
                }
                for sigma in [0.5, 2.0, 4.0] {
                    let filter = Gaussian::new(sigma, Quality::Exact)?;
                    let mut actual = input.clone();
                    filter.apply(&mut actual, shape, edge, &mut Scratch::default(), Cancel::Never)?;
                    close(&actual, &reference(&input, shape, &[gaussian_kernel(sigma)?], edge), 1e-6);
                }
                for radius in [0, 1, 4, 16] {
                    let mut actual = input.clone();
                    box_blur(&mut actual, shape, radius, edge, &mut Scratch::default(), Cancel::Never)?;
                    let kernel = vec![1.0 / (2 * radius + 1) as f32; 2 * radius + 1];
                    close(&actual, &reference(&input, shape, &[kernel], edge), 1e-6);
                }
                let sigma = 4.0;
                let kernels = box_widths(sigma)?.map(|width| vec![1.0 / width as f32; width]);
                let mut actual = input.clone();
                Gaussian::new(sigma, Quality::BoxApprox { passes: 3 })?.apply(&mut actual, shape, edge, &mut Scratch::default(), Cancel::Never)?;
                close(&actual, &reference(&input, shape, &kernels, edge), 1e-6);
            }
        }
    }
    Ok(())
}

#[test]
fn recursive_edges_match_brute_force_impulse_convolution() -> Result<(), Error> {
    for sigma in [8.0, 16.0] {
        let kernel = impulse(sigma, Quality::Recursive)?;
        for (width, height) in [(1, 1), (2, 3), (9, 11)] {
            let shape = Shape::new(width, height, 4)?;
            let input: Vec<_> = (0..shape.samples()).map(|index| (index % 17) as f32 / 17.0).collect();
            for edge in [Edge::Clamp, Edge::Mirror, Edge::Zero, Edge::Transparent] {
                let mut actual = input.clone();
                Gaussian::new(sigma, Quality::Recursive)?.apply(&mut actual, shape, edge, &mut Scratch::default(), Cancel::Never)?;
                close(&actual, &reference(&input, shape, std::slice::from_ref(&kernel), edge), 2e-6);
            }
        }
    }
    Ok(())
}

#[test]
fn constants_and_transparent_premultiplication() -> Result<(), Error> {
    for channels in 1..=4 {
        for quality in QUALITIES {
            for sigma in [1.0, 8.0, 64.0] {
                let filter = Gaussian::new(sigma, quality)?;
                for edge in [Edge::Clamp, Edge::Mirror] {
                    for (width, height) in [(1, 1), (2, 3), (19, 17)] {
                        let shape = Shape::new(width, height, channels)?;
                        let mut values = vec![2.25; shape.samples()];
                        filter.apply(&mut values, shape, edge, &mut Scratch::default(), Cancel::Never)?;
                        close(&values, &vec![2.25; values.len()], 1e-6);
                    }
                }
            }
        }
    }
    let shape = Shape::new(9, 11, 4)?;
    let mut rgba = vec![0.0; shape.samples()];
    rgba[0..4].copy_from_slice(&[0.25, 0.0, 0.0, 0.5]);
    Gaussian::new(2.0, Quality::Exact)?.apply(&mut rgba, shape, Edge::Transparent, &mut Scratch::default(), Cancel::Never)?;
    assert!(rgba.as_chunks::<4>().0.iter().all(|pixel| pixel[0] == 0.5 * pixel[3] && pixel[1] == 0.0 && pixel[2] == 0.0));
    assert!(rgba.iter().skip(3).step_by(4).sum::<f32>() < 0.5);
    Ok(())
}

#[test]
fn cancellation_stops_before_the_next_batch() -> Result<(), Error> {
    for quality in QUALITIES {
        let shape = Shape::new(17, LINE_BATCH * 3, 1)?;
        let input: Vec<_> = (0..shape.samples()).map(|index| (index % 3) as f32).collect();
        let mut actual = input.clone();
        let calls = AtomicUsize::new(0);
        let hook = || calls.fetch_add(1, Ordering::Relaxed) >= 1;
        let filter = Gaussian::new(8.0, quality)?;
        assert_eq!(filter.filter_rows(&mut actual, shape, Edge::Clamp, &mut Scratch::default(), Cancel::Hook(&hook)), Err(Error::Cancelled));
        assert_eq!(calls.load(Ordering::Relaxed), 2);
        assert_ne!(&actual[..17 * LINE_BATCH], &input[..17 * LINE_BATCH]);
        assert_eq!(&actual[17 * LINE_BATCH..], &input[17 * LINE_BATCH..]);
        let flag = AtomicBool::new(true);
        let mut unchanged = input.clone();
        assert_eq!(filter.apply(&mut unchanged, shape, Edge::Clamp, &mut Scratch::default(), Cancel::Flag(&flag)), Err(Error::Cancelled));
        assert_eq!(unchanged, input);
        let shape = Shape::new(24, 17, 1)?;
        let input: Vec<_> = (0..shape.samples()).map(|index| (index / 24 % 3) as f32).collect();
        let mut actual = input.clone();
        let mut rows: Vec<&mut [f32]> = actual.as_chunks_mut::<24>().0.iter_mut().map(|row| &mut row[..]).collect();
        let calls = AtomicUsize::new(0);
        let hook = || calls.fetch_add(1, Ordering::Relaxed) >= 2;
        assert_eq!(filter.filter_columns(&mut rows, 1, Edge::Clamp, &mut Scratch::default(), Cancel::Hook(&hook)), Err(Error::Cancelled));
        assert_eq!(calls.load(Ordering::Relaxed), 3);
        assert!(actual.as_chunks::<24>().0.iter().zip(input.as_chunks::<24>().0).all(|(actual, input)| actual[8..] == input[8..]));
        assert_ne!(actual, input);
    }
    Ok(())
}

#[test]
fn invalid_inputs_are_errors_and_empty_planes_are_valid() -> Result<(), Error> {
    for sigma in [f32::NAN, f32::INFINITY, -1.0] {
        assert_eq!(gaussian_kernel(sigma), Err(Error::InvalidSigma));
        assert!(Gaussian::new(sigma, Quality::Recursive).is_err());
    }
    assert_eq!(gaussian_kernel(f32::MAX), Err(Error::SizeOverflow));
    assert_eq!(box_widths(f32::MAX), Err(Error::SizeOverflow));
    assert_eq!(Shape::new(1, 1, 0), Err(Error::InvalidChannels));
    assert_eq!(Shape::new(1, 1, 5), Err(Error::InvalidChannels));
    assert_eq!(Shape::new(usize::MAX, 2, 1), Err(Error::SizeOverflow));
    assert!(matches!(Gaussian::new(1.0, Quality::BoxApprox { passes: 2 }), Err(Error::InvalidPasses)));
    let filter = Gaussian::new(8.0, Quality::Recursive)?;
    let mut scratch = Scratch::default();
    for (width, height) in [(0, 0), (0, 3), (3, 0)] {
        filter.apply(&mut [], Shape::new(width, height, 1)?, Edge::Clamp, &mut scratch, Cancel::Never)?;
    }
    let mut input = vec![1.0; 4];
    assert_eq!(filter.apply(&mut input, Shape::new(3, 1, 1)?, Edge::Clamp, &mut scratch, Cancel::Never), Err(Error::InvalidDimensions));
    assert_eq!(filter.apply(&mut input, Shape::new(4, 1, 1)?, Edge::Transparent, &mut scratch, Cancel::Never), Err(Error::InvalidChannels));
    assert_eq!(box_blur(&mut input, Shape::new(4, 1, 1)?, usize::MAX, Edge::Clamp, &mut scratch, Cancel::Never), Err(Error::SizeOverflow));
    let (left, right) = input.split_at_mut(1);
    assert_eq!(filter.filter_columns(&mut [left, right], 1, Edge::Clamp, &mut scratch, Cancel::Never), Err(Error::InvalidDimensions));
    Ok(())
}

#[test]
fn worker_stripes_and_scratch_reuse_match_serial() -> Result<(), Error> {
    let shape = Shape::new(19, 17, 3)?;
    let input: Vec<_> = (0..shape.samples()).map(|index| (index % 29) as f32).collect();
    let mut scratch = Scratch::default();
    for quality in QUALITIES {
        let filter = Gaussian::new(8.0, quality)?;
        let mut expected = input.clone();
        filter.apply(&mut expected, shape, Edge::Clamp, &mut scratch, Cancel::Never)?;
        let mut actual = input.clone();
        for rows in actual.chunks_mut(19 * 3 * LINE_BATCH) {
            let batch = Shape::new(19, rows.len() / (19 * 3), 3)?;
            filter.filter_rows(rows, batch, Edge::Clamp, &mut scratch, Cancel::Never)?;
        }
        let mut stripes: Vec<Vec<&mut [f32]>> = (0..3).map(|_| Vec::new()).collect();
        for row in actual.as_chunks_mut::<{ 19 * 3 }>().0 {
            for (stripe, columns) in stripes.iter_mut().zip(row.chunks_mut(8 * 3)) {
                stripe.push(columns);
            }
        }
        std::thread::scope(|scope| {
            let handles: Vec<_> = stripes
                .iter_mut()
                .map(|stripe| {
                    let filter = &filter;
                    scope.spawn(move || filter.filter_columns(stripe, 3, Edge::Clamp, &mut Scratch::default(), Cancel::Never))
                })
                .collect();
            for handle in handles {
                assert!(matches!(handle.join(), Ok(Ok(()))));
            }
        });
        assert_eq!(actual, expected);
    }
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(96))]
    #[test]
    fn random_planes_match_brute_force(
        width in 1usize..12, height in 1usize..12, channels in 1usize..5,
        sigma in 0.1f32..4.0, radius in 0usize..12, edge_index in 0usize..3,
        samples in proptest::collection::vec(-2.0f32..2.0, 1..64)
    ) {
        let shape = Shape::new(width, height, channels)?;
        let edge = [Edge::Clamp, Edge::Mirror, Edge::Zero][edge_index];
        let input: Vec<_> = (0..shape.samples()).map(|index| samples[index % samples.len()]).collect();
        let mut actual = input.clone();
        Gaussian::new(sigma, Quality::Exact)?.apply(&mut actual, shape, edge, &mut Scratch::default(), Cancel::Never)?;
        close(&actual, &reference(&input, shape, &[gaussian_kernel(sigma)?], edge), 1e-6);
        actual.copy_from_slice(&input);
        box_blur(&mut actual, shape, radius, edge, &mut Scratch::default(), Cancel::Never)?;
        close(&actual, &reference(&input, shape, &[vec![1.0 / (2 * radius + 1) as f32; 2 * radius + 1]], edge), 1e-6);
    }
}
