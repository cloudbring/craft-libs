use craft_blur::{Cancel, Edge, Error, Gaussian, Quality, Rect, Scratch, Shape};

fn input(shape: Shape) -> Vec<f32> {
    (0..shape.samples()).map(|index| 0.125 + ((index * 37 + index / 11) % 997) as f32 / 1024.0).collect()
}

fn crop(pixels: &[f32], shape: Shape, rect: Rect) -> Vec<f32> {
    let mut output = Vec::new();
    for row in rect.y..rect.y + rect.height {
        let start = (row * shape.width() + rect.x) * shape.channels();
        output.extend_from_slice(&pixels[start..start + rect.width * shape.channels()]);
    }
    output
}

fn padded_axis(filter: &Gaussian, pixels: &[f32], shape: Shape, horizontal: bool) -> Result<Vec<f32>, Error> {
    let padding = (24.0 * filter.sigma()).ceil() as usize + 32;
    let length = if horizontal { shape.width() } else { shape.height() };
    let count = if horizontal { shape.height() } else { shape.width() };
    let padded_shape = Shape::new(length + 2 * padding, count, shape.channels())?;
    let mut padded = vec![0.0; padded_shape.samples()];
    for line in 0..count {
        for position in 0..padded_shape.width() {
            let original = (position as i128 - padding as i128).rem_euclid(length as i128) as usize;
            let (row, column) = if horizontal { (line, original) } else { (original, line) };
            for channel in 0..shape.channels() {
                padded[(line * padded_shape.width() + position) * shape.channels() + channel] =
                    pixels[(row * shape.width() + column) * shape.channels() + channel];
            }
        }
    }
    filter.filter_rows(&mut padded, padded_shape, Edge::Clamp, &mut Scratch::default(), Cancel::Never)?;
    let mut output = vec![0.0; shape.samples()];
    for line in 0..count {
        for position in 0..length {
            let (row, column) = if horizontal { (line, position) } else { (position, line) };
            for channel in 0..shape.channels() {
                output[(row * shape.width() + column) * shape.channels() + channel] =
                    padded[(line * padded_shape.width() + position + padding) * shape.channels() + channel];
            }
        }
    }
    Ok(output)
}

#[test]
fn wrap_matches_periodic_padding_bit_for_bit() -> Result<(), Error> {
    let mut cases = 0;
    let mut samples = 0;
    for channels in 1..=4 {
        for (width, height) in [(3, 2), (17, 9), (47, 31)] {
            let shape = Shape::new(width, height, channels)?;
            let source = input(shape);
            for (sigma, quality) in [
                (0.0, Quality::Exact),
                (1.0, Quality::Exact),
                (16.0, Quality::Exact),
                (4.0, Quality::BoxApprox { passes: 3 }),
                (4.0, Quality::Recursive),
                (2.5, Quality::RecursiveRaw),
                (4.0, Quality::RecursiveRaw),
                (8.0, Quality::Recursive),
                (16.0, Quality::Recursive),
                (64.0, Quality::Recursive),
                (128.0, Quality::Recursive),
            ] {
                let filter = Gaussian::new(sigma, quality)?;
                let horizontal = padded_axis(&filter, &source, shape, true)?;
                let expected = padded_axis(&filter, &horizontal, shape, false)?;
                let mut actual = source.clone();
                filter.apply(&mut actual, shape, Edge::Wrap, &mut Scratch::default(), Cancel::Never)?;
                let differing = actual.iter().zip(&expected).filter(|(actual, expected)| actual.to_bits() != expected.to_bits()).count();
                let error = actual.iter().zip(&expected).map(|(actual, expected)| (actual - expected).abs()).fold(0.0f32, f32::max);
                assert_eq!(differing, 0, "{width}x{height} {channels}ch sigma={sigma}: max error {error}");
                cases += 1;
                samples += actual.len();
            }
        }
    }
    eprintln!("Wrap padding equality: {cases} cases, {samples} samples, 0 differing bits");
    Ok(())
}

#[test]
fn wrap_is_shift_equivariant_and_preserves_constants() -> Result<(), Error> {
    for sigma in [2.5, 8.0, 64.0, 128.0, 128.01, 512.0, 65_536.0] {
        let filter = Gaussian::new(sigma, Quality::RecursiveRaw)?;
        for length in [1, 2, 3, 9, 37, 257] {
            let shape = Shape::new(length, 1, 1)?;
            let mut constant = vec![7.5; length];
            filter.apply(&mut constant, shape, Edge::Wrap, &mut Scratch::default(), Cancel::Never)?;
            assert_eq!(constant, vec![7.5; length], "sigma={sigma}, length={length}");
            let mut original = input(shape);
            let mut shifted = original.clone();
            shifted.rotate_right(1);
            filter.apply(&mut original, shape, Edge::Wrap, &mut Scratch::default(), Cancel::Never)?;
            filter.apply(&mut shifted, shape, Edge::Wrap, &mut Scratch::default(), Cancel::Never)?;
            shifted.rotate_left(1);
            let error = original.iter().zip(shifted).map(|(actual, expected)| (actual - expected).abs()).fold(0.0f32, f32::max);
            assert!(error <= 2e-6, "sigma={sigma}, length={length}, shift error={error}");
        }
    }
    Ok(())
}

#[test]
fn wrap_boxes_match_direct_modulo_convolution() -> Result<(), Error> {
    for radius in [0, 1, 7, 100] {
        let shape = Shape::new(3, 2, 4)?;
        let source = input(shape);
        let mut horizontal = source.clone();
        let mut expected = source.clone();
        let width = 2 * radius + 1;
        for row in 0..shape.height() {
            for column in 0..shape.width() {
                for channel in 0..shape.channels() {
                    let sum: f64 = (0..width)
                        .map(|tap| {
                            let wrapped = (column as i128 + tap as i128 - radius as i128).rem_euclid(shape.width() as i128) as usize;
                            f64::from(source[(row * shape.width() + wrapped) * shape.channels() + channel])
                        })
                        .sum();
                    horizontal[(row * shape.width() + column) * shape.channels() + channel] = (sum / width as f64) as f32;
                }
            }
        }
        for row in 0..shape.height() {
            for column in 0..shape.width() {
                for channel in 0..shape.channels() {
                    let sum: f64 = (0..width)
                        .map(|tap| {
                            let wrapped = (row as i128 + tap as i128 - radius as i128).rem_euclid(shape.height() as i128) as usize;
                            f64::from(horizontal[(wrapped * shape.width() + column) * shape.channels() + channel])
                        })
                        .sum();
                    expected[(row * shape.width() + column) * shape.channels() + channel] = (sum / width as f64) as f32;
                }
            }
        }
        let mut actual = source;
        craft_blur::box_blur(&mut actual, shape, radius, Edge::Wrap, &mut Scratch::default(), Cancel::Never)?;
        assert_eq!(actual, expected);
    }
    Ok(())
}

#[test]
fn regions_match_full_planes_with_documented_halos() -> Result<(), Error> {
    let mut maximum = 0.0f32;
    let mut cases = 0;
    for channels in 1..=4 {
        let shape = Shape::new(193, 157, channels)?;
        let source: Vec<_> = input(shape).into_iter().map(|value| (value - 0.5) * 2.0).collect();
        for (sigma, quality) in [
            (0.0, Quality::Exact),
            (1.0, Quality::Exact),
            (4.0, Quality::Recursive),
            (2.5, Quality::RecursiveRaw),
            (8.0, Quality::Recursive),
            (16.0, Quality::Recursive),
            (64.0, Quality::Recursive),
            (4.0, Quality::BoxApprox { passes: 3 }),
        ] {
            let filter = Gaussian::new(sigma, quality)?;
            for edge in [Edge::Clamp, Edge::Mirror, Edge::Zero, Edge::Wrap, Edge::Transparent] {
                if edge == Edge::Transparent && channels != 2 && channels != 4 {
                    continue;
                }
                let mut full = source.clone();
                filter.apply(&mut full, shape, edge, &mut Scratch::default(), Cancel::Never)?;
                for rect in [
                    Rect { x: 0, y: 0, width: 13, height: 9 },
                    Rect { x: 179, y: 145, width: 14, height: 12 },
                    Rect { x: 87, y: 71, width: 17, height: 15 },
                    Rect { x: 0, y: 31, width: 193, height: 1 },
                ] {
                    let region = filter.blur_region(&source, shape, rect, edge, &mut Scratch::default(), Cancel::Never)?;
                    let expected = crop(&full, shape, rect);
                    let error = region.iter().zip(&expected).map(|(actual, expected)| (actual - expected).abs()).fold(0.0f32, f32::max);
                    let norm = source.iter().copied().map(f32::abs).fold(0.0, f32::max);
                    assert!(f64::from(error) <= filter.region_error_bound() * f64::from(norm) + 2e-6, "sigma={sigma} {edge:?} {rect:?}: {error}");
                    assert!(error <= 2e-5, "sigma={sigma} {edge:?} {rect:?}: {error}");
                    if quality == Quality::Exact || sigma == 4.0 && quality == Quality::Recursive {
                        assert_eq!(region, expected);
                    }
                    maximum = maximum.max(error);
                    cases += 1;
                }
            }
        }
    }
    eprintln!("Region/full-plane: {cases} cases, maximum absolute error={maximum:e}");
    Ok(())
}

#[test]
fn region_validates_bounds_layout_channels_and_cancellation() -> Result<(), Error> {
    let filter = Gaussian::new(8.0, Quality::Recursive)?;
    assert_eq!(filter.region_halo(), 64);
    assert!(filter.region_error_bound().is_finite());
    let shape = Shape::new(19, 13, 1)?;
    let source = input(shape);
    let mut scratch = Scratch::default();
    let rect = Rect { x: 0, y: 0, width: 1, height: 1 };
    assert_eq!(filter.blur_region(&source, shape, rect, Edge::Transparent, &mut scratch, Cancel::Never), Err(Error::InvalidChannels));
    assert_eq!(filter.blur_region(&source[..1], shape, rect, Edge::Clamp, &mut scratch, Cancel::Never), Err(Error::InvalidDimensions));
    for rect in
        [Rect { x: 19, y: 0, width: 1, height: 1 }, Rect { x: usize::MAX, y: 0, width: 1, height: 1 }, Rect { x: 0, y: 13, width: 1, height: 1 }]
    {
        assert_eq!(filter.blur_region(&source, shape, rect, Edge::Clamp, &mut scratch, Cancel::Never), Err(Error::InvalidDimensions));
    }
    let empty = Rect { x: 19, y: 13, width: 0, height: 0 };
    assert_eq!(filter.blur_region(&source, shape, empty, Edge::Wrap, &mut scratch, Cancel::Never)?, Vec::<f32>::new());
    assert_eq!(filter.blur_region(&source, shape, rect, Edge::Clamp, &mut scratch, Cancel::Hook(&|| true)), Err(Error::Cancelled));
    let all = Rect { x: 0, y: 0, width: shape.width(), height: shape.height() };
    let mut expected = source.clone();
    filter.apply(&mut expected, shape, Edge::Wrap, &mut scratch, Cancel::Never)?;
    assert_eq!(filter.blur_region(&source, shape, all, Edge::Wrap, &mut scratch, Cancel::Never)?, expected);
    assert_eq!(source, input(shape));
    Ok(())
}

#[test]
fn recursive_region_bounds_cover_adversarial_halo_edges() -> Result<(), Error> {
    let shape = Shape::new(513, 385, 1)?;
    let rect = Rect { x: 239, y: 177, width: 17, height: 13 };
    let mut maximum = 0.0f32;
    for sigma in [2.5, 8.0, 16.0, 32.0] {
        let filter = Gaussian::new(sigma, Quality::RecursiveRaw)?;
        let halo = filter.region_halo();
        for pattern in 0..3 {
            let source: Vec<_> = (0..shape.samples())
                .map(|index| {
                    let row = index / shape.width();
                    let column = index % shape.width();
                    match pattern {
                        0 if column.abs_diff(rect.x) > halo || row.abs_diff(rect.y) > halo => 1.0,
                        0 => -1.0,
                        1 if column == rect.x.saturating_sub(halo + 1) => 1.0,
                        1 => 0.0,
                        _ => {
                            if (column + row) % 2 == 0 {
                                1.0
                            } else {
                                -1.0
                            }
                        }
                    }
                })
                .collect();
            for edge in [Edge::Clamp, Edge::Mirror, Edge::Wrap, Edge::Zero] {
                let mut full = source.clone();
                filter.apply(&mut full, shape, edge, &mut Scratch::default(), Cancel::Never)?;
                let region = filter.blur_region(&source, shape, rect, edge, &mut Scratch::default(), Cancel::Never)?;
                let expected = crop(&full, shape, rect);
                let error = region.iter().zip(expected).map(|(actual, expected)| (actual - expected).abs()).fold(0.0f32, f32::max);
                assert!(f64::from(error) <= filter.region_error_bound() + 2e-6, "sigma={sigma}, pattern={pattern}, {edge:?}: {error}");
                maximum = maximum.max(error);
            }
        }
        eprintln!("sigma={sigma}: halo={halo}, normalized analytic region bound={:e}", filter.region_error_bound());
    }
    eprintln!("Adversarial region/full-plane maximum absolute error={maximum:e}");
    Ok(())
}

#[test]
#[ignore = "release Wrap/padding and sparse-tile timings; run --release --test wrap_region -- --ignored --nocapture --test-threads=1"]
fn bench_wrap_padding_and_sparse_region() -> Result<(), Error> {
    use std::time::Instant;

    let shape = Shape::new(2048, 2048, 1)?;
    let filter = Gaussian::new(64.0, Quality::Recursive)?;
    let source = input(shape);
    let padding = (24.0 * filter.sigma()).ceil() as usize + 32;
    let padded_shape = Shape::new(shape.width() + 2 * padding, shape.height() + 2 * padding, 1)?;
    let mut padded = vec![0.0; padded_shape.samples()];
    for row in 0..padded_shape.height() {
        for column in 0..padded_shape.width() {
            let original_row = (row as i128 - padding as i128).rem_euclid(shape.height() as i128) as usize;
            let original_column = (column as i128 - padding as i128).rem_euclid(shape.width() as i128) as usize;
            padded[row * padded_shape.width() + column] = source[original_row * shape.width() + original_column];
        }
    }
    let mut wrap_times = Vec::new();
    let mut padding_times = Vec::new();
    let mut scratch = Scratch::default();
    for iteration in 0..5 {
        let mut wrapped = source.clone();
        let mut extended = padded.clone();
        let started = Instant::now();
        filter.apply(&mut wrapped, shape, Edge::Wrap, &mut scratch, Cancel::Never)?;
        let wrap_time = started.elapsed().as_secs_f64();
        let started = Instant::now();
        filter.apply(&mut extended, padded_shape, Edge::Clamp, &mut scratch, Cancel::Never)?;
        let padding_time = started.elapsed().as_secs_f64();
        assert_eq!(wrapped, crop(&extended, padded_shape, Rect { x: padding, y: padding, width: shape.width(), height: shape.height() }));
        if iteration != 0 {
            wrap_times.push(wrap_time);
            padding_times.push(padding_time);
        }
    }
    wrap_times.sort_by(f64::total_cmp);
    padding_times.sort_by(f64::total_cmp);
    eprintln!(
        "2048² sigma=64: Wrap={:.2}ms, periodic padding={:.2}ms, speedup={:.2}x; 0 differing bits",
        wrap_times[2] * 1000.0,
        padding_times[2] * 1000.0,
        padding_times[2] / wrap_times[2]
    );

    let shape = Shape::new(4096, 4096, 1)?;
    let filter = Gaussian::new(16.0, Quality::Recursive)?;
    let source = input(shape);
    let rect = Rect { x: 2000, y: 2000, width: 64, height: 64 };
    let mut full = source.clone();
    let started = Instant::now();
    filter.apply(&mut full, shape, Edge::Clamp, &mut scratch, Cancel::Never)?;
    let full_time = started.elapsed().as_secs_f64();
    let started = Instant::now();
    let region = filter.blur_region(&source, shape, rect, Edge::Clamp, &mut scratch, Cancel::Never)?;
    let region_time = started.elapsed().as_secs_f64();
    let error = region.iter().zip(crop(&full, shape, rect)).map(|(actual, expected)| (actual - expected).abs()).fold(0.0f32, f32::max);
    assert!(f64::from(error) <= filter.region_error_bound() * 1.125 + 2e-6);
    eprintln!(
        "4096² sigma=16, 64² tile: full={:.2}ms, region={:.3}ms, speedup={:.1}x, max error={error:e}",
        full_time * 1000.0,
        region_time * 1000.0,
        full_time / region_time
    );
    Ok(())
}
