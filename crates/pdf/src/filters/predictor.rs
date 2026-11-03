//! PDF TIFF and PNG predictor reversal.

use super::{Error, Mode};

/// PDF DecodeParms values used after Flate or LZW decoding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Predictor {
    pub predictor: u8,
    pub colors: usize,
    pub bits_per_component: u8,
    pub columns: usize,
}

impl Default for Predictor {
    fn default() -> Self {
        Self { predictor: 1, colors: 1, bits_per_component: 8, columns: 1 }
    }
}

pub(super) fn decode(params: Predictor, mut data: Vec<u8>, mode: Mode) -> Result<(Vec<u8>, bool), Error> {
    if params.predictor == 1 {
        return Ok((data, false));
    }
    if !matches!(params.predictor, 2 | 10..=15)
        || !matches!(params.bits_per_component, 1 | 2 | 4 | 8 | 16)
        || params.colors == 0
        || params.columns == 0
    {
        return Err(Error::Corrupt);
    }
    let bits = usize::from(params.bits_per_component);
    let samples = params.colors.checked_mul(params.columns).ok_or(Error::Corrupt)?;
    let row = samples.checked_mul(bits).and_then(|value| value.checked_add(7)).ok_or(Error::Corrupt)? / 8;
    let bpp = params.colors.checked_mul(bits).and_then(|value| value.checked_add(7)).ok_or(Error::Corrupt)? / 8;
    if params.predictor == 2 {
        let incomplete = !data.len().is_multiple_of(row);
        if incomplete && mode == Mode::Strict {
            return Err(Error::Truncated);
        }
        for line in data.chunks_mut(row) {
            let available = (line.len().saturating_mul(8) / bits).min(samples);
            for index in params.colors..available {
                let value = sample(line, index, bits).wrapping_add(sample(line, index - params.colors, bits));
                set_sample(line, index, bits, value);
            }
        }
        return Ok((data, incomplete));
    }
    let encoded_row = row.checked_add(1).ok_or(Error::Corrupt)?;
    let incomplete = !data.len().is_multiple_of(encoded_row);
    if incomplete && mode == Mode::Strict {
        return Err(Error::Truncated);
    }
    let mut output = Vec::new();
    output.try_reserve_exact(data.len()).map_err(|_| Error::Allocation)?;
    for line in data.chunks(encoded_row) {
        let Some((&tag, bytes)) = line.split_first() else {
            break;
        };
        if tag > 4 {
            return Err(Error::Corrupt);
        }
        let start = output.len();
        for (index, byte) in bytes.iter().copied().enumerate() {
            let left = if index >= bpp { output[start + index - bpp] } else { 0 };
            let up = if start >= row { output[start - row + index] } else { 0 };
            let upper_left = if start >= row && index >= bpp { output[start - row + index - bpp] } else { 0 };
            let prediction = match tag {
                0 => 0,
                1 => left,
                2 => up,
                3 => ((u16::from(left) + u16::from(up)) / 2) as u8,
                4 => paeth(left, up, upper_left),
                _ => return Err(Error::Corrupt),
            };
            output.push(byte.wrapping_add(prediction));
        }
    }
    Ok((output, incomplete))
}

fn sample(row: &[u8], index: usize, bits: usize) -> u16 {
    if bits == 16 {
        u16::from_be_bytes([row[2 * index], row[2 * index + 1]])
    } else {
        u16::from(row[index * bits / 8] >> (8 - bits - (index * bits % 8))) & ((1u16 << bits) - 1)
    }
}

fn set_sample(row: &mut [u8], index: usize, bits: usize, value: u16) {
    if bits == 16 {
        row[2 * index..2 * index + 2].copy_from_slice(&value.to_be_bytes());
    } else {
        let shift = 8 - bits - (index * bits % 8);
        let mask = (((1u16 << bits) - 1) << shift) as u8;
        let slot = &mut row[index * bits / 8];
        *slot = (*slot & !mask) | (((value << shift) as u8) & mask);
    }
}

fn paeth(left: u8, up: u8, upper_left: u8) -> u8 {
    let estimate = i16::from(left) + i16::from(up) - i16::from(upper_left);
    let left_distance = (estimate - i16::from(left)).abs();
    let up_distance = (estimate - i16::from(up)).abs();
    let upper_left_distance = (estimate - i16::from(upper_left)).abs();
    if left_distance <= up_distance && left_distance <= upper_left_distance {
        left
    } else if up_distance <= upper_left_distance {
        up
    } else {
        upper_left
    }
}

pub(super) fn encode(params: Predictor, data: &[u8]) -> Result<Vec<u8>, Error> {
    if params.predictor == 1 {
        return Ok(data.to_vec());
    }
    if !matches!(params.predictor, 2 | 10..=15)
        || !matches!(params.bits_per_component, 1 | 2 | 4 | 8 | 16)
        || params.colors == 0
        || params.columns == 0
    {
        return Err(Error::Corrupt);
    }
    let bits = usize::from(params.bits_per_component);
    let samples = params.colors.checked_mul(params.columns).ok_or(Error::Corrupt)?;
    let row = samples.checked_mul(bits).and_then(|n| n.checked_add(7)).ok_or(Error::Corrupt)? / 8;
    let bpp = params.colors.checked_mul(bits).and_then(|n| n.checked_add(7)).ok_or(Error::Corrupt)? / 8;
    if !data.len().is_multiple_of(row) {
        return Err(Error::Truncated);
    }
    if params.predictor == 2 {
        let mut result = data.to_vec();
        for line in result.chunks_mut(row) {
            for index in (params.colors..samples).rev() {
                let value = sample(line, index, bits).wrapping_sub(sample(line, index - params.colors, bits));
                set_sample(line, index, bits, value);
            }
        }
        return Ok(result);
    }
    let mut result = Vec::new();
    let mut previous: Option<&[u8]> = None;
    for line in data.chunks(row) {
        let filter = |tag: u8| -> Vec<u8> {
            let mut encoded = vec![tag];
            for (index, &byte) in line.iter().enumerate() {
                let left = if index >= bpp { line[index - bpp] } else { 0 };
                let up = previous.map_or(0, |p| p[index]);
                let upper_left = if index >= bpp { previous.map_or(0, |p| p[index - bpp]) } else { 0 };
                let prediction = match tag {
                    1 => left,
                    2 => up,
                    3 => ((u16::from(left) + u16::from(up)) / 2) as u8,
                    4 => paeth(left, up, upper_left),
                    _ => 0,
                };
                encoded.push(byte.wrapping_sub(prediction));
            }
            encoded
        };
        let encoded = if params.predictor == 15 {
            (0..=4)
                .map(filter)
                .min_by_key(|bytes| bytes[1..].iter().map(|&byte| u64::from((byte as i8).unsigned_abs())).sum::<u64>())
                .ok_or(Error::Corrupt)?
        } else {
            filter(params.predictor - 10)
        };
        result.extend_from_slice(&encoded);
        previous = Some(line);
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn png_hand_computed_rows() -> Result<(), Error> {
        let params = Predictor { predictor: 15, columns: 3, ..Predictor::default() };
        let input = vec![0, 10, 20, 30, 1, 5, 10, 10, 2, 2, 3, 4, 3, 8, 9, 10, 4, 1, 2, 3];
        let (decoded, incomplete) = decode(params, input, Mode::Strict)?;
        assert_eq!(decoded, [10, 20, 30, 5, 15, 25, 7, 18, 29, 11, 23, 36, 12, 25, 39]);
        assert!(!incomplete);
        Ok(())
    }

    #[test]
    fn tiff_packed_and_16_bit() -> Result<(), Error> {
        let packed = Predictor { predictor: 2, bits_per_component: 2, columns: 4, ..Predictor::default() };
        assert_eq!(decode(packed, vec![0b01_01_10_11], Mode::Strict)?.0, [0b01_10_00_11]);
        let wide = Predictor { predictor: 2, bits_per_component: 16, columns: 3, ..Predictor::default() };
        assert_eq!(decode(wide, vec![0, 1, 0, 2, 255, 255], Mode::Strict)?.0, [0, 1, 0, 3, 0, 2]);
        Ok(())
    }
}
