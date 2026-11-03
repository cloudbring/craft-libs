//! Integer-exact 8-bit distortion for encoder search and mode decision.
//!
//! Only 128-bit `wide` vectors are used: SSE2 on x86-64, NEON on AArch64,
//! SIMD128 on wasm when enabled, and `wide`'s scalar fallback otherwise.
//! SATD always sums halved 4x4 Hadamards, including for 8x8 blocks: changing
//! to an 8x8 transform would change the encoder's decisions.
//! Invalid or unsupported block geometry returns the maximum cost, never a
//! partial-block score. Encoder callers provide validated, padded pictures.

use wide::{i16x8, i32x4, u8x16, u16x8};

#[inline]
fn valid(plane: &[u8], stride: usize, width: usize, height: usize) -> bool {
    width > 0
        && width <= 16
        && height > 0
        && height <= 16
        && stride >= width
        && (height - 1).checked_mul(stride).and_then(|offset| offset.checked_add(width)).is_some_and(|end| end <= plane.len())
}

#[inline(always)]
fn load<const WIDTH: usize>(row: &[u8]) -> u8x16 {
    let mut lanes = [0; 16];
    lanes[..WIDTH].copy_from_slice(&row[..WIDTH]);
    u8x16::new(lanes)
}

#[inline]
pub fn sad(first: &[u8], first_stride: usize, second: &[u8], second_stride: usize, width: usize, height: usize) -> u32 {
    match width {
        4 => sad_n::<4>(first, first_stride, second, second_stride, height),
        8 => sad8(first, first_stride, second, second_stride, height),
        16 => sad16(first, first_stride, second, second_stride, height),
        _ => u32::MAX,
    }
}

#[inline]
pub fn sad8(first: &[u8], first_stride: usize, second: &[u8], second_stride: usize, height: usize) -> u32 {
    sad_n::<8>(first, first_stride, second, second_stride, height)
}

#[inline]
pub fn sad16(first: &[u8], first_stride: usize, second: &[u8], second_stride: usize, height: usize) -> u32 {
    sad_n::<16>(first, first_stride, second, second_stride, height)
}

#[inline(always)]
fn sad_n<const WIDTH: usize>(first: &[u8], first_stride: usize, second: &[u8], second_stride: usize, height: usize) -> u32 {
    if !valid(first, first_stride, WIDTH, height) || !valid(second, second_stride, WIDTH, height) {
        return u32::MAX;
    }
    let mut total = u16x8::ZERO;
    for row in 0..height {
        let left = load::<WIDTH>(&first[row * first_stride..]);
        let right = load::<WIDTH>(&second[row * second_stride..]);
        let difference = left.max(right) - left.min(right);
        total += u16x8::from_u8x16_low(difference);
        if WIDTH == 16 {
            total += u16x8::from_u8x16_high(difference);
        }
    }
    u32::from(total.reduce_add())
}

#[inline(always)]
fn difference<const WIDTH: usize>(first: &[u8], second: &[u8]) -> i16x8 {
    i16x8::from_u8x16_low(load::<WIDTH>(first)) - i16x8::from_u8x16_low(load::<WIDTH>(second))
}

#[inline(always)]
fn swap_adjacent(value: i16x8) -> i16x8 {
    let lanes = value.to_array();
    i16x8::new([lanes[1], lanes[0], lanes[3], lanes[2], lanes[5], lanes[4], lanes[7], lanes[6]])
}

#[inline(always)]
fn swap_pairs(value: i16x8) -> i16x8 {
    let lanes = value.to_array();
    i16x8::new([lanes[2], lanes[3], lanes[0], lanes[1], lanes[6], lanes[7], lanes[4], lanes[5]])
}

/// Two side-by-side 4x4 transforms (or one with zero-filled upper lanes).
/// Each butterfly fits i16; the last uses |p+q|+|p-q| = 2 max(|p|,|q|).
#[inline(always)]
fn satd_strip<const WIDTH: usize>(first: &[u8], first_stride: usize, second: &[u8], second_stride: usize) -> u32 {
    let rows: [i16x8; 4] = std::array::from_fn(|row| difference::<WIDTH>(&first[row * first_stride..], &second[row * second_stride..]));
    let sum01 = rows[0] + rows[1];
    let diff01 = rows[0] - rows[1];
    let sum23 = rows[2] + rows[3];
    let diff23 = rows[2] - rows[3];
    let mut total = i16x8::ZERO;
    for row in [sum01 + sum23, sum01 - sum23, diff01 + diff23, diff01 - diff23] {
        let adjacent = swap_adjacent(row);
        let sums = (row + adjacent).abs();
        let differences = (row - adjacent).abs();
        total += sums.max(swap_pairs(sums)) + differences.max(swap_pairs(differences));
    }
    let lanes = total.to_array();
    lanes[0] as u32 + lanes[4] as u32
}

/// Sum of halved 4x4 Hadamards, for dimensions divisible by four, up to 16x16.
pub fn satd(first: &[u8], first_stride: usize, second: &[u8], second_stride: usize, width: usize, height: usize) -> u32 {
    if !width.is_multiple_of(4) || !height.is_multiple_of(4) || !valid(first, first_stride, width, height) || !valid(second, second_stride, width, height) {
        return u32::MAX;
    }
    let mut total = 0;
    for row in (0..height).step_by(4) {
        for column in (0..width).step_by(8) {
            let left = &first[row * first_stride + column..];
            let right = &second[row * second_stride + column..];
            total += if width - column >= 8 {
                satd_strip::<8>(left, first_stride, right, second_stride)
            } else {
                satd_strip::<4>(left, first_stride, right, second_stride)
            };
        }
    }
    total
}

/// Sum of squared errors (also called SSD), with widened products and u64 output.
pub fn sse(first: &[u8], first_stride: usize, second: &[u8], second_stride: usize, width: usize, height: usize) -> u64 {
    match width {
        4 => sse_n::<4>(first, first_stride, second, second_stride, height),
        8 => sse_n::<8>(first, first_stride, second, second_stride, height),
        16 => sse_n::<16>(first, first_stride, second, second_stride, height),
        _ => u64::MAX,
    }
}

#[inline(always)]
fn sse_n<const WIDTH: usize>(first: &[u8], first_stride: usize, second: &[u8], second_stride: usize, height: usize) -> u64 {
    if !valid(first, first_stride, WIDTH, height) || !valid(second, second_stride, WIDTH, height) {
        return u64::MAX;
    }
    let mut total = i32x4::ZERO;
    for row in 0..height {
        let left = load::<WIDTH>(&first[row * first_stride..]);
        let right = load::<WIDTH>(&second[row * second_stride..]);
        let low = i16x8::from_u8x16_low(left) - i16x8::from_u8x16_low(right);
        total += low.dot(low);
        if WIDTH == 16 {
            let high = i16x8::from_u8x16_high(left) - i16x8::from_u8x16_high(right);
            total += high.dot(high);
        }
    }
    total.reduce_add() as u64
}

