//! FilmCraft's 128-bit kernels, plus the sizes and SATD scales the other encoders need.
//!
//! The functions `load`, `valid`, `sad_n`, `difference`, `swap_adjacent`, `swap_pairs`,
//! `satd_strip` and `sse_n` are the FilmCraft `dc23d047` kernel. `valid` stays capped at
//! 16 because a 16×16 SAD fills a `u16` accumulator (`65280`). Taller SAD/SSE blocks are
//! bands of at most 16 rows. SATD does not use that accumulator, so its geometry check
//! goes to [`crate::MAX_SIDE`].

use wide::{i16x8, i32x4, u8x16, u16x8};

use crate::MAX_SIDE;

#[inline]
fn fits(plane: &[u8], stride: usize, width: usize, height: usize) -> bool {
    width > 0
        && width <= MAX_SIDE
        && height > 0
        && height <= MAX_SIDE
        && stride >= width
        && (height - 1).checked_mul(stride).and_then(|offset| offset.checked_add(width)).is_some_and(|end| end <= plane.len())
}

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

fn column(plane: &[u8], column: usize) -> &[u8] {
    plane.get(column..).unwrap_or(&[])
}

fn origin(plane: &[u8], stride: usize, x: usize, y: usize) -> &[u8] {
    match y.checked_mul(stride).and_then(|row| row.checked_add(x)) {
        Some(start) => plane.get(start..).unwrap_or(&[]),
        None => &[],
    }
}

fn sad_tail(width: usize, first: &[u8], first_stride: usize, second: &[u8], second_stride: usize, height: usize) -> u32 {
    match width {
        1 => sad_n::<1>(first, first_stride, second, second_stride, height),
        2 => sad_n::<2>(first, first_stride, second, second_stride, height),
        3 => sad_n::<3>(first, first_stride, second, second_stride, height),
        4 => sad_n::<4>(first, first_stride, second, second_stride, height),
        5 => sad_n::<5>(first, first_stride, second, second_stride, height),
        6 => sad_n::<6>(first, first_stride, second, second_stride, height),
        7 => sad_n::<7>(first, first_stride, second, second_stride, height),
        _ => u32::MAX,
    }
}

fn sse_tail(width: usize, first: &[u8], first_stride: usize, second: &[u8], second_stride: usize, height: usize) -> u64 {
    match width {
        1 => sse_n::<1>(first, first_stride, second, second_stride, height),
        2 => sse_n::<2>(first, first_stride, second, second_stride, height),
        3 => sse_n::<3>(first, first_stride, second, second_stride, height),
        4 => sse_n::<4>(first, first_stride, second, second_stride, height),
        5 => sse_n::<5>(first, first_stride, second, second_stride, height),
        6 => sse_n::<6>(first, first_stride, second, second_stride, height),
        7 => sse_n::<7>(first, first_stride, second, second_stride, height),
        _ => u64::MAX,
    }
}

fn sad_band(first: &[u8], first_stride: usize, second: &[u8], second_stride: usize, width: usize, height: usize) -> u32 {
    let mut total = 0u32;
    let mut x = 0;
    while x + 16 <= width {
        let part = sad_n::<16>(column(first, x), first_stride, column(second, x), second_stride, height);
        if part == u32::MAX {
            return u32::MAX;
        }
        total += part;
        x += 16;
    }
    if x + 8 <= width {
        let part = sad_n::<8>(column(first, x), first_stride, column(second, x), second_stride, height);
        if part == u32::MAX {
            return u32::MAX;
        }
        total += part;
        x += 8;
    }
    if x < width {
        let part = sad_tail(width - x, column(first, x), first_stride, column(second, x), second_stride, height);
        if part == u32::MAX {
            return u32::MAX;
        }
        total += part;
    }
    total
}

fn sse_band(first: &[u8], first_stride: usize, second: &[u8], second_stride: usize, width: usize, height: usize) -> u64 {
    let mut total = 0u64;
    let mut x = 0;
    while x + 16 <= width {
        let part = sse_n::<16>(column(first, x), first_stride, column(second, x), second_stride, height);
        if part == u64::MAX {
            return u64::MAX;
        }
        total += part;
        x += 16;
    }
    if x + 8 <= width {
        let part = sse_n::<8>(column(first, x), first_stride, column(second, x), second_stride, height);
        if part == u64::MAX {
            return u64::MAX;
        }
        total += part;
        x += 8;
    }
    if x < width {
        let part = sse_tail(width - x, column(first, x), first_stride, column(second, x), second_stride, height);
        if part == u64::MAX {
            return u64::MAX;
        }
        total += part;
    }
    total
}

fn rows_of(plane: &[u8], stride: usize, row: usize) -> &[u8] {
    match row.checked_mul(stride) {
        Some(start) => plane.get(start..).unwrap_or(&[]),
        None => &[],
    }
}

fn sad_rows(first: &[u8], first_stride: usize, second: &[u8], second_stride: usize, width: usize, height: usize) -> u32 {
    if !fits(first, first_stride, width, height) || !fits(second, second_stride, width, height) {
        return u32::MAX;
    }
    let mut total = 0u32;
    let mut row = 0;
    while row < height {
        let band = (height - row).min(16);
        let part = sad_band(rows_of(first, first_stride, row), first_stride, rows_of(second, second_stride, row), second_stride, width, band);
        if part == u32::MAX {
            return u32::MAX;
        }
        total += part;
        row += band;
    }
    total
}

fn sse_rows(first: &[u8], first_stride: usize, second: &[u8], second_stride: usize, width: usize, height: usize) -> u64 {
    if !fits(first, first_stride, width, height) || !fits(second, second_stride, width, height) {
        return u64::MAX;
    }
    let mut total = 0u64;
    let mut row = 0;
    while row < height {
        let band = (height - row).min(16);
        let part = sse_band(rows_of(first, first_stride, row), first_stride, rows_of(second, second_stride, row), second_stride, width, band);
        if part == u64::MAX {
            return u64::MAX;
        }
        total += part;
        row += band;
    }
    total
}

/// Sum of absolute differences. Width and height are `1..=64`.
#[inline]
pub fn sad(first: &[u8], first_stride: usize, second: &[u8], second_stride: usize, width: usize, height: usize) -> u32 {
    if (1..=16).contains(&height) {
        match width {
            4 => return sad_n::<4>(first, first_stride, second, second_stride, height),
            8 => return sad_n::<8>(first, first_stride, second, second_stride, height),
            16 => return sad_n::<16>(first, first_stride, second, second_stride, height),
            _ => {}
        }
    }
    sad_rows(first, first_stride, second, second_stride, width, height)
}

/// SAD of an 8-wide block. `height` may be any value in `1..=64`.
#[inline]
pub fn sad8(first: &[u8], first_stride: usize, second: &[u8], second_stride: usize, height: usize) -> u32 {
    sad(first, first_stride, second, second_stride, 8, height)
}

/// SAD of a 16-wide block. `height` may be any value in `1..=64`.
#[inline]
pub fn sad16(first: &[u8], first_stride: usize, second: &[u8], second_stride: usize, height: usize) -> u32 {
    sad(first, first_stride, second, second_stride, 16, height)
}

/// Sum of squared errors (also called SSD). Width and height are `1..=64`.
#[inline]
pub fn sse(first: &[u8], first_stride: usize, second: &[u8], second_stride: usize, width: usize, height: usize) -> u64 {
    if (1..=16).contains(&height) {
        match width {
            4 => return sse_n::<4>(first, first_stride, second, second_stride, height),
            8 => return sse_n::<8>(first, first_stride, second, second_stride, height),
            16 => return sse_n::<16>(first, first_stride, second, second_stride, height),
            _ => {}
        }
    }
    sse_rows(first, first_stride, second, second_stride, width, height)
}

fn satd_halves(first: &[u8], first_stride: usize, second: &[u8], second_stride: usize, width: usize, height: usize) -> u32 {
    if !width.is_multiple_of(4)
        || !height.is_multiple_of(4)
        || !fits(first, first_stride, width, height)
        || !fits(second, second_stride, width, height)
    {
        return u32::MAX;
    }
    let mut total = 0u32;
    for row in (0..height).step_by(4) {
        for column in (0..width).step_by(8) {
            let left = origin(first, first_stride, column, row);
            let right = origin(second, second_stride, column, row);
            let part = if width - column >= 8 {
                satd_strip::<8>(left, first_stride, right, second_stride)
            } else {
                satd_strip::<4>(left, first_stride, right, second_stride)
            };
            match total.checked_add(part) {
                Some(sum) => total = sum,
                None => return u32::MAX,
            }
        }
    }
    total
}

/// H.264 SATD: sum of halved 4×4 Hadamards. Sides are multiples of 4, at most 64.
pub fn satd(first: &[u8], first_stride: usize, second: &[u8], second_stride: usize, width: usize, height: usize) -> u32 {
    satd_halves(first, first_stride, second, second_stride, width, height)
}

fn fwht8_i16(values: &mut [i16; 8]) {
    let mut distance = 1;
    while distance < 8 {
        let mut start = 0;
        while start < 8 {
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

fn fwht_vertical(mut rows: [i16x8; 8]) -> [i16x8; 8] {
    let mut distance = 1;
    while distance < 8 {
        let mut start = 0;
        while start < 8 {
            for offset in 0..distance {
                let left = rows[start + offset];
                let right = rows[start + offset + distance];
                rows[start + offset] = left + right;
                rows[start + offset + distance] = left - right;
            }
            start += distance * 2;
        }
        distance *= 2;
    }
    rows
}

fn hadamard8_rows(first: &[u8], first_stride: usize, second: &[u8], second_stride: usize) -> Option<[i16x8; 8]> {
    if !valid(first, first_stride, 8, 8) || !valid(second, second_stride, 8, 8) {
        return None;
    }
    let mut rows = [i16x8::ZERO; 8];
    for row in 0..8 {
        let mut lanes = difference::<8>(&first[row * first_stride..], &second[row * second_stride..]).to_array();
        fwht8_i16(&mut lanes);
        rows[row] = i16x8::new(lanes);
    }
    Some(fwht_vertical(rows))
}

fn abs_sum(rows: &[i16x8; 8]) -> u32 {
    let mut total = 0i32;
    for row in rows {
        for lane in row.to_array() {
            total += i32::from(lane.unsigned_abs());
        }
    }
    u32::try_from(total).unwrap_or(u32::MAX)
}

fn hadamard8_abs(first: &[u8], first_stride: usize, second: &[u8], second_stride: usize) -> Option<u32> {
    hadamard8_rows(first, first_stride, second, second_stride).map(|rows| abs_sum(&rows))
}

pub(crate) fn hadamard8_coeffs(first: &[u8], first_stride: usize, second: &[u8], second_stride: usize) -> Option<[i32; 64]> {
    let rows = hadamard8_rows(first, first_stride, second, second_stride)?;
    let mut coeffs = [0i32; 64];
    for (row_index, row) in rows.iter().enumerate() {
        for (column, lane) in row.to_array().iter().enumerate() {
            coeffs[row_index * 8 + column] = i32::from(*lane);
        }
    }
    Some(coeffs)
}

/// Quadrant `index` of a square split into four `step`-wide blocks: `(row, column)`.
fn quadrant(index: usize, step: usize) -> (usize, usize) {
    ((index >> 1) * step, (index & 1) * step)
}

fn hadamard16_coeffs(first: &[u8], first_stride: usize, second: &[u8], second_stride: usize) -> Option<[i32; 256]> {
    let mut quads = [[0i32; 64]; 4];
    for (index, quad) in quads.iter_mut().enumerate() {
        let (row, column) = quadrant(index, 8);
        *quad = hadamard8_coeffs(origin(first, first_stride, column, row), first_stride, origin(second, second_stride, column, row), second_stride)?;
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
    Some(out)
}

fn butterfly_abs_sum(quads: &[&[i32]; 4], shift: u32) -> u32 {
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

fn vp9_satd16(first: &[u8], first_stride: usize, second: &[u8], second_stride: usize) -> Option<u32> {
    hadamard16_coeffs(first, first_stride, second, second_stride).map(|coeffs| coeffs.iter().map(|value| value.unsigned_abs()).sum::<u32>())
}

fn vp9_satd32(first: &[u8], first_stride: usize, second: &[u8], second_stride: usize) -> Option<u32> {
    let mut quads = [[0i32; 256]; 4];
    for (index, quad) in quads.iter_mut().enumerate() {
        let (row, column) = quadrant(index, 16);
        *quad = hadamard16_coeffs(origin(first, first_stride, column, row), first_stride, origin(second, second_stride, column, row), second_stride)?;
    }
    let refs = [&quads[0][..], &quads[1][..], &quads[2][..], &quads[3][..]];
    Some(butterfly_abs_sum(&refs, 2))
}

fn tile_squares(width: usize, height: usize, step: usize, mut score: impl FnMut(usize, usize) -> Option<u32>) -> Option<u32> {
    let mut total = 0u32;
    let mut y = 0;
    while y < height {
        let mut x = 0;
        while x < width {
            total = total.checked_add(score(x, y)?)?;
            x += step;
        }
        y += step;
    }
    Some(total)
}

fn tile8(
    first: &[u8],
    first_stride: usize,
    second: &[u8],
    second_stride: usize,
    width: usize,
    height: usize,
    scale: impl Fn(u32) -> u32,
) -> Option<u32> {
    tile_squares(width, height, 8, |x, y| {
        hadamard8_abs(origin(first, first_stride, x, y), first_stride, origin(second, second_stride, x, y), second_stride).map(&scale)
    })
}

fn geometry(first: &[u8], first_stride: usize, second: &[u8], second_stride: usize, width: usize, height: usize) -> bool {
    width.is_multiple_of(4) && height.is_multiple_of(4) && fits(first, first_stride, width, height) && fits(second, second_stride, width, height)
}

/// AV1 SATD. 8×8 Hadamards (divide by 4) when both sides are multiples of 8, otherwise [`satd`].
pub fn satd_av1(first: &[u8], first_stride: usize, second: &[u8], second_stride: usize, width: usize, height: usize) -> u32 {
    if !geometry(first, first_stride, second, second_stride, width, height) {
        return u32::MAX;
    }
    if width.is_multiple_of(8) && height.is_multiple_of(8) {
        tile8(first, first_stride, second, second_stride, width, height, |raw| raw / 4).unwrap_or(u32::MAX)
    } else {
        satd_halves(first, first_stride, second, second_stride, width, height)
    }
}

/// HEVC SATD. 8×8 tiles use `(sum + 2) >> 2`. Other multiples of 4 use the 4×4 score.
pub fn satd_hevc(first: &[u8], first_stride: usize, second: &[u8], second_stride: usize, width: usize, height: usize) -> u32 {
    if !geometry(first, first_stride, second, second_stride, width, height) {
        return u32::MAX;
    }
    if width.is_multiple_of(8) && height.is_multiple_of(8) {
        tile8(first, first_stride, second, second_stride, width, height, |raw| (raw + 2) >> 2).unwrap_or(u32::MAX)
    } else {
        satd_halves(first, first_stride, second, second_stride, width, height)
    }
}

/// VP9 / libvpx SATD. See the crate docs for the 8×8, 16×16, 32×32 and 4×4 scales.
pub fn satd_vp9(first: &[u8], first_stride: usize, second: &[u8], second_stride: usize, width: usize, height: usize) -> u32 {
    if !geometry(first, first_stride, second, second_stride, width, height) {
        return u32::MAX;
    }
    let scored = if width.is_multiple_of(32) && height.is_multiple_of(32) {
        tile_squares(width, height, 32, |x, y| {
            vp9_satd32(origin(first, first_stride, x, y), first_stride, origin(second, second_stride, x, y), second_stride)
        })
    } else if width.is_multiple_of(16) && height.is_multiple_of(16) {
        tile_squares(width, height, 16, |x, y| {
            vp9_satd16(origin(first, first_stride, x, y), first_stride, origin(second, second_stride, x, y), second_stride)
        })
    } else if width.is_multiple_of(8) && height.is_multiple_of(8) {
        tile8(first, first_stride, second, second_stride, width, height, |raw| raw)
    } else {
        let score = satd_halves(first, first_stride, second, second_stride, width, height);
        return if score == u32::MAX { u32::MAX } else { score * 2 };
    };
    scored.unwrap_or(u32::MAX)
}
