use super::{
    Component, Decoded, Error, IDCT_FRACTION_BITS, Limits, Samples, ZIGZAG, decode, decode_coefficients, idct_block, idct_line, idct_multiply,
    reconstruct, render_with_idct,
};

fn reference_basis() -> [[f64; 8]; 8] {
    std::array::from_fn(|position| {
        std::array::from_fn(|frequency| {
            let scale = if frequency == 0 { std::f64::consts::FRAC_1_SQRT_2 } else { 1.0 };
            scale * 0.5 * (((2 * position + 1) * frequency) as f64 * std::f64::consts::PI / 16.0).cos()
        })
    })
}

fn reference_block(block: &[i32; 64], quant: &[u16; 64], basis: &[[f64; 8]; 8]) -> [f64; 64] {
    let mut coefficients = [0.0; 64];
    for (index, &coefficient) in block.iter().enumerate() {
        coefficients[ZIGZAG[index]] = f64::from(coefficient) * f64::from(quant[index]);
    }
    let rows: [f64; 64] =
        std::array::from_fn(|index| (0..8).map(|frequency| basis[index % 8][frequency] * coefficients[index / 8 * 8 + frequency]).sum());
    std::array::from_fn(|index| (0..8).map(|frequency| basis[index / 8][frequency] * rows[frequency * 8 + index % 8]).sum())
}

struct Random(u64);

impl Random {
    fn next(&mut self) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 32) as u32
    }
}

fn fixed_samples(block: &[i32; 64], quant: &[u16; 64]) -> [f64; 64] {
    idct_block(block, quant).map(|value| value as f64 / f64::from((1 << IDCT_FRACTION_BITS) * 8))
}

#[test]
fn idct_random_blocks_match_t81_f64_within_one_lsb() {
    let basis = reference_basis();
    let mut random = Random(0x0030_1989_1208);
    for precision in [8, 12] {
        let limit = 1 << (precision + 3);
        let mut maximum_error = 0.0f64;
        for iteration in 0..4096 {
            let block = std::array::from_fn(|_| (random.next() % (limit * 2 + 1)) as i32 - limit as i32);
            let quant =
                std::array::from_fn(|_| if iteration < 2048 { 1 } else { (random.next() % if precision == 8 { 255 } else { 65535 } + 1) as u16 });
            let reference = reference_block(&block, &quant, &basis);
            for (actual, expected) in fixed_samples(&block, &quant).into_iter().zip(reference) {
                let error = (actual - expected).abs();
                maximum_error = maximum_error.max(error);
                assert!(error <= 1.0, "{precision}-bit, block {iteration}: {actual} != {expected}");
                assert!((actual.round() - expected.round()).abs() <= 1.0);
            }
        }
        eprintln!("{precision}-bit IDCT: 4096 random blocks, maximum unrounded error {maximum_error:.6} LSB");
    }
}

#[test]
fn idct_impulses_and_sparse_axes_match_t81() {
    let basis = reference_basis();
    for coefficient in [-32768, -2048, -1, 1, 2047, 32767] {
        for frequency in 0..64 {
            let mut block = [0; 64];
            block[frequency] = coefficient;
            for (actual, expected) in fixed_samples(&block, &[1; 64]).into_iter().zip(reference_block(&block, &[1; 64], &basis)) {
                assert!((actual - expected).abs() < 0.02, "frequency {frequency}: {actual} != {expected}");
            }
        }
        for axis in [0, 1] {
            let block = std::array::from_fn(|index| {
                if ZIGZAG[index] / 8 == 0 && axis == 0 || ZIGZAG[index].is_multiple_of(8) && axis == 1 { coefficient } else { 0 }
            });
            for (actual, expected) in fixed_samples(&block, &[1; 64]).into_iter().zip(reference_block(&block, &[1; 64], &basis)) {
                assert!((actual - expected).abs() < 0.02, "axis {axis}: {actual} != {expected}");
            }
        }
    }
}

fn component(block: [i32; 64]) -> Component {
    Component { id: 1, h: 1, v: 1, tq: 0, cw: 8, ch: 8, bw: 1, bh: 1, coefs: vec![block], quant: None }
}

#[test]
fn idct_dc_shortcut_matches_full_transform() {
    for dc in -32768..=32767 {
        let mut block = [0; 64];
        block[0] = dc;
        let full = idct_block(&block, &[1; 64]);
        assert_eq!(full, [i64::from(dc) << IDCT_FRACTION_BITS; 64]);
        for maximum in [255.0, 4095.0] {
            let expected = (dc as f32 / 8.0 + (maximum + 1.0) / 2.0).clamp(0.0, maximum);
            assert_eq!(reconstruct(&component(block), &[1; 64], maximum), vec![expected; 64]);
        }
    }
    for dc in [i32::MIN, -32768, -2048, -1, 0, 1, 2047, 32767, i32::MAX] {
        for quantizer in [0, 1, 3, 255, 4095, u16::MAX] {
            let mut block = [0; 64];
            block[0] = dc;
            let expected = (i64::from(dc) * i64::from(quantizer)) << IDCT_FRACTION_BITS;
            assert_eq!(idct_block(&block, &[quantizer; 64]), [expected; 64]);
            for maximum in [255.0, 4095.0] {
                let sample = ((i64::from(dc) * i64::from(quantizer)) as f32 / 8.0 + (maximum + 1.0) / 2.0).clamp(0.0, maximum);
                assert_eq!(reconstruct(&component(block), &[quantizer; 64], maximum), vec![sample; 64]);
            }
        }
    }
}

#[test]
fn idct_fixed_point_rounding_is_nearest_ties_up() {
    let half = 1 << 55;
    for (value, expected) in [(-5, -2), (-4, -2), (-3, -1), (-2, -1), (-1, 0), (0, 0), (1, 1), (2, 1), (3, 2), (4, 2), (5, 3)] {
        assert_eq!(idct_multiply::<false>(value, half), expected);
        assert_eq!(idct_multiply::<true>(value, half), expected);
    }
}

#[test]
fn idct_narrow_path_and_dispatch_boundaries_match_t81() {
    let basis = reference_basis();
    for bound in [16383, 16384, 16385] {
        for signs in 0..256 {
            let block = std::array::from_fn(|index| if signs & (1 << (index % 8)) == 0 { -bound } else { bound });
            for (actual, expected) in fixed_samples(&block, &[1; 64]).into_iter().zip(reference_block(&block, &[1; 64], &basis)) {
                assert!((actual - expected).abs() < 0.02, "bound {bound}, signs {signs}: {actual} != {expected}");
            }
        }
    }
    let bound = (1i64 << 25) - 1;
    for signs in 0..256 {
        let input = std::array::from_fn(|index| if signs & (1 << index) == 0 { -bound } else { bound });
        for (narrow, wide) in idct_line::<false>(input).into_iter().zip(idct_line::<true>(input)) {
            assert!(narrow.abs_diff(wide) <= 8);
        }
    }
}

#[test]
fn idct_extreme_line_signs_keep_i64_headroom() {
    for bound in [1i64 << 55, (1 << 58) + 8] {
        for signs in 0..256 {
            let input = std::array::from_fn(|index| if signs & (1 << index) == 0 { -bound } else { bound });
            for output in idct_line::<true>(input) {
                assert!(output.abs() <= 8 * bound + 8);
            }
        }
    }
}

fn assert_extreme_block(block: [i32; 64], quant: &[u16; 64], basis: &[[f64; 8]; 8]) {
    let reference = reference_block(&block, quant, basis);
    for maximum in [255.0, 4095.0] {
        let actual = reconstruct(&component(block), quant, maximum);
        for (actual, expected) in actual.into_iter().zip(reference) {
            let expected = (expected + f64::from((maximum + 1.0) / 2.0)).clamp(0.0, f64::from(maximum));
            assert!(actual.is_finite() && (0.0..=maximum).contains(&actual));
            assert!((f64::from(actual) - expected).abs() <= 1.0, "extreme: {actual} != {expected}");
        }
    }
}

#[test]
fn idct_extreme_blocks_saturate_without_overflow() {
    let basis = reference_basis();
    for coefficient in [i32::MIN, -32768, -2048, 2047, 32767, i32::MAX] {
        for quantizer in [0, 1, 255, u16::MAX] {
            assert_extreme_block([coefficient; 64], &[quantizer; 64], &basis);
        }
    }
    for position in 0..64 {
        for sign in [-1.0, 1.0] {
            let block = std::array::from_fn(|index| {
                let natural = ZIGZAG[index];
                if basis[position / 8][natural / 8] * basis[position % 8][natural % 8] * sign < 0.0 { i32::MIN } else { i32::MAX }
            });
            assert_extreme_block(block, &[u16::MAX; 64], &basis);
        }
    }
    let mut random = Random(0x30_ffff_ffff);
    for _ in 0..512 {
        let block = std::array::from_fn(|_| random.next() as i32);
        let quant = std::array::from_fn(|_| random.next() as u16);
        assert_extreme_block(block, &quant, &basis);
    }
}

/// The previous decoder's f32 matrix passes, including its DC shortcut, retained only as an
/// output-regression oracle. This intentionally does not use the factorized implementation.
fn previous_basis() -> [[f32; 8]; 8] {
    let mut basis = [[0.0; 8]; 8];
    for (position, row) in basis.iter_mut().enumerate() {
        for (frequency, value) in row.iter_mut().enumerate() {
            let scale = if frequency == 0 { std::f32::consts::FRAC_1_SQRT_2 } else { 1.0 };
            *value = scale / 2.0 * (((2 * position + 1) * frequency) as f32 * std::f32::consts::PI / 16.0).cos();
        }
    }
    basis
}

fn previous_reconstruct(component: &Component, quant: &[u16; 64], maximum: f32, basis: &[[f32; 8]; 8]) -> Vec<f32> {
    let stride = component.bw * 8;
    let shift = (maximum + 1.0) / 2.0;
    let mut plane = vec![0.0; stride * component.bh * 8];
    for (index, block) in component.coefs.iter().enumerate() {
        let (block_x, block_y) = (index % component.bw, index / component.bw);
        if block[1..].iter().all(|&coefficient| coefficient == 0) {
            let constant = basis[0][0];
            let sample = (constant * (constant * (block[0] as f32 * f32::from(quant[0]))) + shift).clamp(0.0, maximum);
            for row in 0..8 {
                let start = (block_y * 8 + row) * stride + block_x * 8;
                if let Some(output) = plane.get_mut(start..start + 8) {
                    output.fill(sample);
                }
            }
            continue;
        }
        let mut coefficients = [0.0; 64];
        for (frequency, &coefficient) in block.iter().enumerate() {
            coefficients[ZIGZAG[frequency]] = coefficient as f32 * f32::from(quant[frequency]);
        }
        let mut rows = [0.0; 64];
        for row in 0..8 {
            for column in 0..8 {
                rows[row * 8 + column] = (0..8).map(|frequency| basis[column][frequency] * coefficients[row * 8 + frequency]).sum();
            }
        }
        for (row, weights) in basis.iter().enumerate() {
            let start = (block_y * 8 + row) * stride + block_x * 8;
            if let Some(output) = plane.get_mut(start..start + 8) {
                for (column, output) in output.iter_mut().enumerate() {
                    let sample: f32 = weights.iter().enumerate().map(|(frequency, weight)| weight * rows[frequency * 8 + column]).sum();
                    *output = (sample + shift).clamp(0.0, maximum);
                }
            }
        }
    }
    plane
}

fn previous_decode(bytes: &[u8], limits: &Limits) -> Result<Decoded, Error> {
    let (frame, tables, damaged) = decode_coefficients(bytes, limits)?;
    let (width, height) = (frame.width as u32, frame.height as u32);
    let basis = previous_basis();
    let (components, samples) =
        render_with_idct(frame, &tables, None, |component, quant, maximum| previous_reconstruct(component, quant, maximum, &basis))?;
    Ok(Decoded { width, height, components, samples, damaged })
}

#[test]
fn idct_fixture_pixels_differ_by_at_most_one_output_lsb() {
    let mut fixture_count = 0;
    let mut changed_samples = 0;
    let mut maximum_difference = 0;
    for directory in ["tests/data", "fuzz/corpus/decode"] {
        let entries = std::fs::read_dir(format!("{}/{directory}", env!("CARGO_MANIFEST_DIR"))).unwrap_or_else(|error| panic!("{directory}: {error}"));
        let mut paths: Vec<_> = entries
            .map(|entry| entry.unwrap_or_else(|error| panic!("{directory}: {error}")).path())
            .filter(|path| path.extension().is_some_and(|extension| extension == "jpg"))
            .collect();
        paths.sort();
        for path in paths {
            let bytes = std::fs::read(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
            let actual = decode(&bytes, &Limits::default(), None).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
            let previous = previous_decode(&bytes, &Limits::default()).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
            assert!(!actual.damaged, "{}", path.display());
            assert_eq!(
                (actual.width, actual.height, actual.components, actual.damaged),
                (previous.width, previous.height, previous.components, previous.damaged)
            );
            let differences: Vec<_> = match (actual.samples, previous.samples) {
                (Samples::U8(actual), Samples::U8(previous)) => {
                    assert_eq!(actual.len(), previous.len());
                    actual.into_iter().zip(previous).map(|(actual, previous)| u16::from(actual.abs_diff(previous))).collect()
                }
                (Samples::U16(actual), Samples::U16(previous)) => {
                    assert_eq!(actual.len(), previous.len());
                    actual.into_iter().zip(previous).map(|(actual, previous)| actual.abs_diff(previous)).collect()
                }
                _ => panic!("sample precision changed"),
            };
            let maximum = differences.iter().copied().max().unwrap_or(0);
            let changed = differences.iter().filter(|&&difference| difference != 0).count();
            eprintln!("{}: {changed}/{} samples changed, max {maximum} output LSB", path.display(), differences.len());
            assert!(maximum <= 1, "{}: maximum difference {maximum}", path.display());
            fixture_count += 1;
            changed_samples += changed;
            maximum_difference = maximum_difference.max(maximum);
        }
    }
    assert_eq!(fixture_count, 23);
    eprintln!("IDCT fixture regression: {fixture_count} files, {changed_samples} changed samples, maximum {maximum_difference} output LSB");
}
