use std::time::Instant;

use crate::kernel::hadamard8_coeffs;
use crate::{sad, sad8, sad16, satd, satd_av1, satd_hevc, satd_vp9, sse};

#[allow(clippy::all, dead_code, unused_imports)]
#[rustfmt::skip]
#[path = "../tests/filmcraft_dc23d047_distortion.inc"]
mod filmcraft_dc23d047;

const SIDES: [usize; 8] = [4, 8, 12, 16, 24, 32, 48, 64];
const FILMCRAFT_SHAPES: [(usize, usize); 9] = [(4, 4), (4, 8), (4, 16), (8, 4), (8, 8), (8, 16), (16, 4), (16, 8), (16, 16)];
const BENCH_SHAPES: [(usize, usize); 25] = [
    (4, 4),
    (4, 8),
    (8, 4),
    (8, 8),
    (8, 16),
    (16, 8),
    (16, 16),
    (4, 16),
    (16, 4),
    (8, 32),
    (32, 8),
    (16, 32),
    (32, 16),
    (32, 32),
    (32, 64),
    (64, 32),
    (16, 64),
    (64, 16),
    (64, 64),
    (12, 12),
    (24, 24),
    (48, 48),
    (48, 64),
    (64, 48),
    (24, 32),
];

fn fwht(values: &mut [i32]) {
    let mut distance = 1;
    while distance < values.len() {
        let mut start = 0;
        while start < values.len() {
            for offset in 0..distance {
                let left = values[start + offset];
                let right = values[start + offset + distance];
                values[start + offset] = left + right;
                values[start + offset + distance] = left - right;
            }
            start += distance * 2;
        }
        distance *= 2;
    }
}

fn hadamard_coeffs(n: usize, first: &[u8], first_stride: usize, second: &[u8], second_stride: usize) -> Vec<i32> {
    let mut block = vec![0i32; n * n];
    for y in 0..n {
        for x in 0..n {
            block[y * n + x] = i32::from(first[y * first_stride + x]) - i32::from(second[y * second_stride + x]);
        }
        fwht(&mut block[y * n..y * n + n]);
    }
    for x in 0..n {
        let mut column = vec![0i32; n];
        for y in 0..n {
            column[y] = block[y * n + x];
        }
        fwht(&mut column);
        for y in 0..n {
            block[y * n + x] = column[y];
        }
    }
    block
}

fn array64(values: Vec<i32>) -> [i32; 64] {
    values.try_into().expect("8x8 Hadamard is 64 coefficients")
}

fn abs_sum(values: &[i32]) -> u32 {
    values.iter().map(|value| value.unsigned_abs()).sum()
}

fn reference_sad(first: &[u8], first_stride: usize, second: &[u8], second_stride: usize, width: usize, height: usize) -> u32 {
    let mut total = 0u32;
    for row in 0..height {
        for column in 0..width {
            total += u32::from(first[row * first_stride + column].abs_diff(second[row * second_stride + column]));
        }
    }
    total
}

fn reference_sse(first: &[u8], first_stride: usize, second: &[u8], second_stride: usize, width: usize, height: usize) -> u64 {
    let mut total = 0u64;
    for row in 0..height {
        for column in 0..width {
            let difference = i32::from(first[row * first_stride + column]) - i32::from(second[row * second_stride + column]);
            total += u64::from(difference.unsigned_abs()).pow(2);
        }
    }
    total
}

fn reference_satd4_raw(first: &[u8], first_stride: usize, second: &[u8], second_stride: usize, width: usize, height: usize) -> u32 {
    let mut total = 0u32;
    for row in (0..height).step_by(4) {
        for column in (0..width).step_by(4) {
            total += abs_sum(&hadamard_coeffs(
                4,
                &first[row * first_stride + column..],
                first_stride,
                &second[row * second_stride + column..],
                second_stride,
            ));
        }
    }
    total
}

fn reference_satd(first: &[u8], first_stride: usize, second: &[u8], second_stride: usize, width: usize, height: usize) -> u32 {
    reference_satd4_raw(first, first_stride, second, second_stride, width, height) / 2
}

fn reference_satd8_raw(first: &[u8], first_stride: usize, second: &[u8], second_stride: usize, width: usize, height: usize) -> Vec<u32> {
    let mut tiles = Vec::new();
    for row in (0..height).step_by(8) {
        for column in (0..width).step_by(8) {
            tiles.push(abs_sum(&hadamard_coeffs(
                8,
                &first[row * first_stride + column..],
                first_stride,
                &second[row * second_stride + column..],
                second_stride,
            )));
        }
    }
    tiles
}

fn reference_satd_av1(first: &[u8], first_stride: usize, second: &[u8], second_stride: usize, width: usize, height: usize) -> u32 {
    if width.is_multiple_of(8) && height.is_multiple_of(8) {
        reference_satd8_raw(first, first_stride, second, second_stride, width, height).into_iter().map(|raw| raw / 4).sum()
    } else {
        reference_satd(first, first_stride, second, second_stride, width, height)
    }
}

fn reference_satd_hevc(first: &[u8], first_stride: usize, second: &[u8], second_stride: usize, width: usize, height: usize) -> u32 {
    if width.is_multiple_of(8) && height.is_multiple_of(8) {
        reference_satd8_raw(first, first_stride, second, second_stride, width, height).into_iter().map(|raw| (raw + 2) >> 2).sum()
    } else {
        reference_satd(first, first_stride, second, second_stride, width, height)
    }
}

fn butterfly_abs(quads: &[&[i32]; 4], shift: u32) -> u32 {
    let mut sum = 0u32;
    let rows = quads[0].iter().zip(quads[1]).zip(quads[2].iter().zip(quads[3]));
    for ((&left, &right), (&below, &corner)) in rows {
        let b0 = (left + right) >> shift;
        let b1 = (left - right) >> shift;
        let b2 = (below + corner) >> shift;
        let b3 = (below - corner) >> shift;
        sum += (b0 + b2).unsigned_abs();
        sum += (b1 + b3).unsigned_abs();
        sum += (b0 - b2).unsigned_abs();
        sum += (b1 - b3).unsigned_abs();
    }
    sum
}

fn reference_vp9_16_coeffs(first: &[u8], first_stride: usize, second: &[u8], second_stride: usize) -> [i32; 256] {
    let mut quads = [[0i32; 64]; 4];
    for (index, quad) in quads.iter_mut().enumerate() {
        let row = (index >> 1) * 8;
        let column = (index & 1) * 8;
        *quad =
            array64(hadamard_coeffs(8, &first[row * first_stride + column..], first_stride, &second[row * second_stride + column..], second_stride));
    }
    let mut out = [0i32; 256];
    let (band0, rest) = out.split_at_mut(64);
    let (band1, rest) = rest.split_at_mut(64);
    let (band2, band3) = rest.split_at_mut(64);
    let rows = quads[0].iter().zip(&quads[1]).zip(quads[2].iter().zip(&quads[3]));
    let dests = band0.iter_mut().zip(band1).zip(band2.iter_mut().zip(band3));
    for (((dest0, dest1), (dest2, dest3)), ((&left, &right), (&below, &corner))) in dests.zip(rows) {
        let b0 = (left + right) >> 1;
        let b1 = (left - right) >> 1;
        let b2 = (below + corner) >> 1;
        let b3 = (below - corner) >> 1;
        *dest0 = b0 + b2;
        *dest1 = b1 + b3;
        *dest2 = b0 - b2;
        *dest3 = b1 - b3;
    }
    out
}

fn reference_vp9_16(first: &[u8], first_stride: usize, second: &[u8], second_stride: usize) -> u32 {
    abs_sum(&reference_vp9_16_coeffs(first, first_stride, second, second_stride))
}

fn reference_vp9_32(first: &[u8], first_stride: usize, second: &[u8], second_stride: usize) -> u32 {
    let mut quads = [[0i32; 256]; 4];
    for (index, quad) in quads.iter_mut().enumerate() {
        let row = (index >> 1) * 16;
        let column = (index & 1) * 16;
        *quad = reference_vp9_16_coeffs(&first[row * first_stride + column..], first_stride, &second[row * second_stride + column..], second_stride);
    }
    let refs = [&quads[0][..], &quads[1][..], &quads[2][..], &quads[3][..]];
    butterfly_abs(&refs, 2)
}

fn reference_satd_vp9(first: &[u8], first_stride: usize, second: &[u8], second_stride: usize, width: usize, height: usize) -> u32 {
    let score = |step: usize, block: fn(&[u8], usize, &[u8], usize) -> u32| -> u32 {
        let mut total = 0u32;
        for row in (0..height).step_by(step) {
            for column in (0..width).step_by(step) {
                total += block(&first[row * first_stride + column..], first_stride, &second[row * second_stride + column..], second_stride);
            }
        }
        total
    };
    if width.is_multiple_of(32) && height.is_multiple_of(32) {
        score(32, reference_vp9_32)
    } else if width.is_multiple_of(16) && height.is_multiple_of(16) {
        score(16, reference_vp9_16)
    } else if width.is_multiple_of(8) && height.is_multiple_of(8) {
        reference_satd8_raw(first, first_stride, second, second_stride, width, height).into_iter().sum()
    } else {
        reference_satd4_raw(first, first_stride, second, second_stride, width, height)
    }
}

fn filmcraft_sad(width: usize, height: usize) -> bool {
    matches!(width, 4 | 8 | 16) && (1..=16).contains(&height)
}

fn filmcraft_satd(width: usize, height: usize) -> bool {
    width.is_multiple_of(4) && height.is_multiple_of(4) && (4..=16).contains(&width) && (4..=16).contains(&height)
}

fn check(first: &[u8], first_stride: usize, second: &[u8], second_stride: usize, width: usize, height: usize) {
    assert_eq!(
        sad(first, first_stride, second, second_stride, width, height),
        reference_sad(first, first_stride, second, second_stride, width, height),
        "sad {width}x{height}"
    );
    assert_eq!(
        sse(first, first_stride, second, second_stride, width, height),
        reference_sse(first, first_stride, second, second_stride, width, height),
        "sse {width}x{height}"
    );
    if width == 8 {
        assert_eq!(sad8(first, first_stride, second, second_stride, height), sad(first, first_stride, second, second_stride, 8, height));
    }
    if width == 16 {
        assert_eq!(sad16(first, first_stride, second, second_stride, height), sad(first, first_stride, second, second_stride, 16, height));
    }
    if filmcraft_sad(width, height) {
        assert_eq!(
            sad(first, first_stride, second, second_stride, width, height),
            filmcraft_dc23d047::sad(first, first_stride, second, second_stride, width, height)
        );
        assert_eq!(
            sse(first, first_stride, second, second_stride, width, height),
            filmcraft_dc23d047::sse(first, first_stride, second, second_stride, width, height)
        );
        if width == 8 {
            assert_eq!(
                sad8(first, first_stride, second, second_stride, height),
                filmcraft_dc23d047::sad8(first, first_stride, second, second_stride, height)
            );
        }
        if width == 16 {
            assert_eq!(
                sad16(first, first_stride, second, second_stride, height),
                filmcraft_dc23d047::sad16(first, first_stride, second, second_stride, height)
            );
        }
    }
    if width.is_multiple_of(4) && height.is_multiple_of(4) {
        assert_eq!(
            satd(first, first_stride, second, second_stride, width, height),
            reference_satd(first, first_stride, second, second_stride, width, height),
            "satd {width}x{height}"
        );
        assert_eq!(
            satd_av1(first, first_stride, second, second_stride, width, height),
            reference_satd_av1(first, first_stride, second, second_stride, width, height),
            "satd_av1 {width}x{height}"
        );
        assert_eq!(
            satd_hevc(first, first_stride, second, second_stride, width, height),
            reference_satd_hevc(first, first_stride, second, second_stride, width, height),
            "satd_hevc {width}x{height}"
        );
        assert_eq!(
            satd_vp9(first, first_stride, second, second_stride, width, height),
            reference_satd_vp9(first, first_stride, second, second_stride, width, height),
            "satd_vp9 {width}x{height}"
        );
        if !(width.is_multiple_of(8) && height.is_multiple_of(8)) {
            assert_eq!(
                satd_hevc(first, first_stride, second, second_stride, width, height),
                satd(first, first_stride, second, second_stride, width, height)
            );
            assert_eq!(
                satd_av1(first, first_stride, second, second_stride, width, height),
                satd(first, first_stride, second, second_stride, width, height)
            );
        }
    }
    if filmcraft_satd(width, height) {
        assert_eq!(
            satd(first, first_stride, second, second_stride, width, height),
            filmcraft_dc23d047::satd(first, first_stride, second, second_stride, width, height),
            "filmcraft satd {width}x{height}"
        );
    }
}

fn xorshift(seed: &mut u32) -> u8 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 17;
    *seed ^= *seed << 5;
    *seed as u8
}

fn random_pair(seed: &mut u32, width: usize, height: usize, iteration: usize) -> (Vec<u8>, usize, Vec<u8>, usize) {
    let first_stride = width + iteration % 17;
    let second_stride = width + iteration % 23;
    let first_offset = iteration % 16;
    let second_offset = (iteration / 16) % 16;
    let first_len = first_offset + (height - 1) * first_stride + width;
    let second_len = second_offset + (height - 1) * second_stride + width;
    let first: Vec<u8> = (0..first_len).map(|_| xorshift(seed)).collect();
    let second: Vec<u8> = (0..second_len).map(|_| xorshift(seed)).collect();
    (first[first_offset..].to_vec(), first_stride, second[second_offset..].to_vec(), second_stride)
}

fn fill(width: usize, height: usize, pixel: impl Fn(usize, usize) -> u8) -> Vec<u8> {
    let mut plane = vec![0; width * height];
    for y in 0..height {
        for x in 0..width {
            plane[y * width + x] = pixel(x, y);
        }
    }
    plane
}

#[test]
fn filmcraft_copy_is_the_dc23d047_kernel() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/filmcraft_dc23d047_distortion.inc");
    let text = std::fs::read_to_string(path).expect("verbatim kernel");
    assert_eq!(text.len(), 6251);
    assert!(text.contains("Only 128-bit `wide` vectors are used"));
    assert!(!text.contains("mod tests"));
    assert_eq!(&text.as_bytes()[text.len() - 3..], b"}\n\n");
}

#[test]
fn anchors_match_the_codec_definitions() {
    let tens = [10u8; 64 * 64];
    let zeros = [0u8; 64 * 64];
    let mut impulse = [0u8; 64 * 64];
    impulse[0] = 1;
    assert_eq!(sad(&tens, 4, &zeros, 4, 4, 4), 160);
    assert_eq!(sse(&tens, 4, &zeros, 4, 4, 4), 1_600);
    assert_eq!(satd(&tens, 4, &zeros, 4, 4, 4), 80);
    assert_eq!(satd_av1(&tens, 4, &zeros, 4, 4, 4), 80);
    assert_eq!(satd_hevc(&tens, 4, &zeros, 4, 4, 4), 80);
    assert_eq!(satd_vp9(&tens, 4, &zeros, 4, 4, 4), 160);
    assert_eq!(satd(&tens, 8, &zeros, 8, 8, 8), 320);
    assert_eq!(satd_av1(&tens, 8, &zeros, 8, 8, 8), 160);
    assert_eq!(satd_hevc(&tens, 8, &zeros, 8, 8, 8), 160);
    assert_eq!(satd_vp9(&tens, 8, &zeros, 8, 8, 8), 640);
    assert_eq!(satd(&impulse, 8, &zeros, 8, 8, 8), 8);
    assert_eq!(satd_av1(&impulse, 8, &zeros, 8, 8, 8), 16);
    assert_eq!(satd_hevc(&impulse, 8, &zeros, 8, 8, 8), 16);
    assert_eq!(satd_vp9(&impulse, 8, &zeros, 8, 8, 8), 64);
    assert_eq!(satd_vp9(&tens, 16, &zeros, 16, 16, 16), 1_280);
    assert_eq!(satd_vp9(&impulse, 16, &zeros, 16, 16, 16), 0);
    assert_eq!(satd_vp9(&tens, 32, &zeros, 32, 32, 32), 1_280);
    assert_eq!(satd_vp9(&tens, 64, &zeros, 64, 64, 64), 5_120);
    assert_eq!(satd(&tens, 64, &zeros, 64, 64, 64), 20_480);
    assert_eq!(satd_av1(&tens, 64, &zeros, 64, 64, 64), 10_240);
    assert_eq!(satd_hevc(&tens, 64, &zeros, 64, 64, 64), 10_240);
    let full = [255u8; 64 * 64];
    assert_eq!(sad16(&full, 16, &zeros, 16, 16), 65_280);
    assert_eq!(sse(&full, 16, &zeros, 16, 16, 16), 16_646_400);
    assert_eq!(sad(&full, 64, &zeros, 64, 64, 64), 1_044_480);
    assert_eq!(sse(&full, 64, &zeros, 64, 64, 64), 266_342_400);
    assert_eq!(filmcraft_dc23d047::sad(&full, 32, &zeros, 32, 32, 32), u32::MAX);
    assert_eq!(filmcraft_dc23d047::satd(&full, 32, &zeros, 32, 32, 32), u32::MAX);
    assert_ne!(sad(&full, 32, &zeros, 32, 32, 32), u32::MAX);
}

#[test]
fn eight_by_eight_coefficients_match_the_scalar_hadamard() {
    let mut seed = 0xdec0_u32;
    for _ in 0..64 {
        let first: Vec<u8> = (0..64).map(|_| xorshift(&mut seed)).collect();
        let second: Vec<u8> = (0..64).map(|_| xorshift(&mut seed)).collect();
        let produced = hadamard8_coeffs(&first, 8, &second, 8).expect("8x8 coeffs");
        let expected = hadamard_coeffs(8, &first, 8, &second, 8);
        assert_eq!(produced.as_slice(), expected.as_slice());
    }
}

#[test]
fn all_byte_pairs_on_filmcraft_shapes() {
    for left in 0..=255 {
        for right in 0..=255 {
            let first = [left; 256];
            let second = [right; 256];
            for (width, height) in FILMCRAFT_SHAPES {
                check(&first, 16, &second, 16, width, height);
            }
        }
    }
}

#[test]
fn random_strides_match_the_scalar_reference_and_filmcraft() {
    let mut seed = 0x1234_5678u32;
    for (width, height) in FILMCRAFT_SHAPES {
        for iteration in 0..128 {
            let (first, first_stride, second, second_stride) = random_pair(&mut seed, width, height, iteration);
            check(&first, first_stride, &second, second_stride, width, height);
        }
    }
    for &width in &SIDES {
        for &height in &SIDES {
            for iteration in 0..16 {
                let (first, first_stride, second, second_stride) = random_pair(&mut seed, width, height, iteration);
                check(&first, first_stride, &second, second_stride, width, height);
            }
        }
    }
    for (width, height) in
        [(1, 1), (1, 64), (64, 1), (3, 5), (7, 13), (15, 15), (17, 16), (24, 7), (6, 64), (64, 9), (2, 2), (5, 8), (9, 16), (31, 32), (33, 4)]
    {
        for iteration in 0..8 {
            let (first, first_stride, second, second_stride) = random_pair(&mut seed, width, height, iteration);
            check(&first, first_stride, &second, second_stride, width, height);
        }
    }
}

#[test]
fn extremes_checkerboards_and_impulses() {
    for &width in &SIDES {
        for &height in &SIDES {
            let zero = fill(width, height, |_, _| 0);
            let full = fill(width, height, |_, _| 255);
            let checker = fill(width, height, |x, y| if (x + y) % 2 == 0 { 255 } else { 0 });
            let inverse = fill(width, height, |x, y| if (x + y) % 2 == 0 { 0 } else { 255 });
            for first in [&zero, &full, &checker, &inverse] {
                for second in [&zero, &full, &checker, &inverse] {
                    check(first, width, second, width, width, height);
                }
            }
            let positions = if width * height <= 64 {
                (0..height).flat_map(|y| (0..width).map(move |x| (x, y))).collect::<Vec<_>>()
            } else {
                vec![(0, 0), (width - 1, 0), (0, height - 1), (width - 1, height - 1), (width / 2, height / 2)]
            };
            for (x, y) in positions {
                let mut impulse = zero.clone();
                impulse[y * width + x] = 255;
                check(&impulse, width, &zero, width, width, height);
                check(&full, width, &impulse, width, width, height);
            }
        }
    }
}

#[test]
fn rejects_invalid_geometry_without_partial_scores() {
    let pixels = [255u8; 64 * 64];
    for (stride, width, height) in [
        (0, 4, 4),
        (3, 4, 4),
        (16, 0, 4),
        (16, 4, 0),
        (16, 4, 65),
        (16, 65, 4),
        (usize::MAX, 4, 4),
        (16, usize::MAX, 4),
        (16, 4, usize::MAX),
        (8, 16, 4),
    ] {
        assert_eq!(sad(&pixels, stride, &pixels, 16, width, height), u32::MAX, "{stride} {width}x{height}");
        assert_eq!(satd(&pixels, stride, &pixels, 16, width, height), u32::MAX);
        assert_eq!(satd_av1(&pixels, stride, &pixels, 16, width, height), u32::MAX);
        assert_eq!(satd_hevc(&pixels, stride, &pixels, 16, width, height), u32::MAX);
        assert_eq!(satd_vp9(&pixels, stride, &pixels, 16, width, height), u32::MAX);
        assert_eq!(sse(&pixels, stride, &pixels, 16, width, height), u64::MAX);
    }
    for width in [4usize, 8, 16, 32, 64] {
        for height in [4usize, 8, 17, 32, 64] {
            let length = (height - 1) * width + width;
            if length == 0 || length > pixels.len() {
                continue;
            }
            let short = &pixels[..length - 1];
            assert_eq!(sad(short, width, &pixels, width, width, height), u32::MAX);
            assert_eq!(sse(&pixels, width, short, width, width, height), u64::MAX);
            if width.is_multiple_of(4) && height.is_multiple_of(4) {
                assert_eq!(satd(short, width, &pixels, width, width, height), u32::MAX);
            }
        }
    }
    assert_eq!(satd(&pixels, 8, &pixels, 8, 6, 8), u32::MAX);
    assert_eq!(satd(&pixels, 8, &pixels, 8, 8, 6), u32::MAX);
    let exact = (63 * 64) + 64;
    check(&pixels[..exact], 64, &pixels[..exact], 64, 64, 64);
}

#[test]
fn hevc_rounding_follows_the_per_tile_formula() {
    let mut seed = 0x51ed_u32;
    let mut saw_remainder_two = false;
    for _ in 0..256 {
        let first: Vec<u8> = (0..64).map(|_| xorshift(&mut seed)).collect();
        let second: Vec<u8> = (0..64).map(|_| xorshift(&mut seed)).collect();
        let raw = abs_sum(&hadamard_coeffs(8, &first, 8, &second, 8));
        assert_eq!(satd_av1(&first, 8, &second, 8, 8, 8), raw / 4);
        assert_eq!(satd_hevc(&first, 8, &second, 8, 8, 8), (raw + 2) >> 2);
        assert_eq!(satd_vp9(&first, 8, &second, 8, 8, 8), raw);
        if raw % 4 == 2 {
            saw_remainder_two = true;
            assert_ne!(satd_hevc(&first, 8, &second, 8, 8, 8), satd_av1(&first, 8, &second, 8, 8, 8));
        }
    }
    let _ = saw_remainder_two;
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
#[ignore = "release-only distortion microbenchmark; microseconds per 10_000 blocks, scalar versus SIMD"]
fn benchmark_kernels() {
    let first: Vec<u8> = (0..64 * 64).map(|index: usize| (index.wrapping_mul(73).wrapping_add(19)) as u8).collect();
    let second: Vec<u8> = (0..64 * 64).map(|index: usize| (index.wrapping_mul(31).wrapping_add(237)) as u8).collect();
    let iterations = 10_000u32;
    for (width, height) in BENCH_SHAPES {
        for (name, vectorized) in [("scalar", false), ("simd", true)] {
            let started = Instant::now();
            for _ in 0..iterations {
                let left = std::hint::black_box(&first[..]);
                let right = std::hint::black_box(&second[..]);
                if vectorized {
                    std::hint::black_box((
                        sad(left, 64, right, 64, width, height),
                        sse(left, 64, right, 64, width, height),
                        satd(left, 64, right, 64, width, height),
                        satd_av1(left, 64, right, 64, width, height),
                        satd_hevc(left, 64, right, 64, width, height),
                        satd_vp9(left, 64, right, 64, width, height),
                    ));
                } else {
                    std::hint::black_box((
                        reference_sad(left, 64, right, 64, width, height),
                        reference_sse(left, 64, right, 64, width, height),
                        reference_satd(left, 64, right, 64, width, height),
                        reference_satd_av1(left, 64, right, 64, width, height),
                        reference_satd_hevc(left, 64, right, 64, width, height),
                        reference_satd_vp9(left, 64, right, 64, width, height),
                    ));
                }
            }
            let microseconds = started.elapsed().as_secs_f64() * 1_000_000.0;
            eprintln!("bench {width}x{height} {name} {microseconds:.1} µs/10k");
        }
    }
}
