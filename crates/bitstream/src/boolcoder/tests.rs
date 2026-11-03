use super::{BoolDecoder, BoolError};

#[cfg(test)]
mod roundtrip {
    use super::*;

    /// The encoder of section 7.3, to round-trip the decoder.
    struct BoolEncoder {
        out: Vec<u8>,
        range: u32,
        bottom: u32,
        bit_count: i32,
    }

    impl BoolEncoder {
        fn new() -> Self {
            BoolEncoder { out: Vec::new(), range: 255, bottom: 0, bit_count: 24 }
        }
        fn add_one(&mut self) {
            let mut i = self.out.len();
            while i > 0 {
                i -= 1;
                if self.out[i] == 255 {
                    self.out[i] = 0;
                } else {
                    self.out[i] += 1;
                    break;
                }
            }
        }
        fn write(&mut self, p: u8, bit: bool) {
            let split = 1 + (((self.range - 1) * u32::from(p)) >> 8);
            if bit {
                self.bottom = self.bottom.wrapping_add(split);
                self.range -= split;
            } else {
                self.range = split;
            }
            while self.range < 128 {
                self.range <<= 1;
                if self.bottom & (1 << 31) != 0 {
                    self.add_one();
                }
                self.bottom <<= 1;
                self.bit_count -= 1;
                if self.bit_count == 0 {
                    self.out.push((self.bottom >> 24) as u8);
                    self.bottom &= (1 << 24) - 1;
                    self.bit_count = 8;
                }
            }
        }
        fn finish(mut self) -> Vec<u8> {
            let mut c = self.bit_count;
            let mut v = self.bottom;
            if v & (1u32.wrapping_shl((32 - c) as u32)) != 0 {
                self.add_one();
            }
            v <<= c & 7;
            c >>= 3;
            while c > 0 {
                c -= 1;
                v <<= 8;
            }
            for _ in 0..4 {
                self.out.push((v >> 24) as u8);
                v <<= 8;
            }
            self.out
        }
    }

    #[test]
    fn round_trips_bools_at_every_probability() {
        let mut seed = 0x1234_5678u32;
        let mut rnd = || {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            seed
        };
        let bits: Vec<(u8, bool)> = (0..20_000).map(|_| (rnd() as u8, rnd() % 3 == 0)).collect();
        let mut e = BoolEncoder::new();
        for &(p, b) in &bits {
            e.write(p, b);
        }
        let data = e.finish();
        let mut d = BoolDecoder::new(&data);
        for (i, &(p, b)) in bits.iter().enumerate() {
            assert_eq!(d.read_bool(p), b, "bool {i}");
        }
    }

    #[test]
    fn reads_zeros_past_the_end() {
        let mut d = BoolDecoder::new(&[]);
        for _ in 0..1000 {
            assert!(!d.read_bool(128));
        }
    }
}

/// Bit-at-a-time state, wrapping its eight-bit value on nonconforming input.
struct Reference<'a> {
    data: &'a [u8],
    position: usize,
    value: u32,
    range: u32,
}

impl<'a> Reference<'a> {
    fn new(data: &'a [u8]) -> Self {
        let mut decoder = Self { data, position: 0, value: 0, range: 255 };
        for _ in 0..8 {
            decoder.value = (decoder.value << 1) | decoder.next_bit();
        }
        decoder
    }

    fn next_bit(&mut self) -> u32 {
        let bit = self.data.get(self.position / 8).map_or(0, |byte| (byte >> (7 - self.position % 8)) & 1);
        self.position += 1;
        u32::from(bit)
    }

    fn read(&mut self, probability: u8) -> bool {
        let split = 1 + (((self.range - 1) * u32::from(probability)) >> 8);
        let bit = if self.value < split {
            self.range = split;
            false
        } else {
            self.range -= split;
            self.value -= split;
            true
        };
        while self.range < 128 {
            self.range *= 2;
            self.value = ((self.value << 1) | self.next_bit()) & 255;
        }
        bit
    }

    fn literal(&mut self, width: u32) -> u32 {
        (0..width).fold(0, |value, _| (value << 1) | u32::from(self.read(128)))
    }

    fn finish(&self) -> Result<(), BoolError> {
        if self.position > self.data.len() * 8 {
            return Err(BoolError::UnexpectedEof);
        }
        for position in self.position..self.data.len() * 8 {
            if (self.data[position / 8] >> (7 - position % 8)) & 1 != 0 {
                return Err(BoolError::NonzeroPadding);
            }
        }
        Ok(())
    }
}

fn random(seed: &mut u32) -> u32 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 17;
    *seed ^= *seed << 5;
    *seed
}

#[test]
fn every_byte_and_probability_matches_reference() {
    for byte in 0..=255u8 {
        for probability in 0..=255u8 {
            let data = [byte, byte.reverse_bits()];
            for length in 0..=data.len() {
                let mut actual = BoolDecoder::new(&data[..length]);
                let mut reference = Reference::new(&data[..length]);
                for symbol in 0..32 {
                    assert_eq!(actual.read_bool(probability), reference.read(probability), "{byte} {probability} {length} {symbol}");
                }
                assert_eq!(actual.finish_vp9(), reference.finish());
            }
        }
    }
}

#[test]
fn million_random_symbols_per_codec() {
    let mut seed = 0x1234_5678;
    for vp9 in [false, true] {
        let mut data: Vec<u8> = (0..1_000_008).map(|_| random(&mut seed) as u8).collect();
        data[0] &= 127;
        let mut actual = if vp9 { BoolDecoder::new_vp9(&data).unwrap() } else { BoolDecoder::new(&data) };
        let mut reference = Reference::new(&data);
        if vp9 {
            assert!(!reference.read(128));
        }
        for symbol in 0..1_000_000 {
            let probability = random(&mut seed) as u8;
            assert_eq!(actual.read_bool(probability), reference.read(probability), "vp9={vp9} symbol={symbol}");
        }
        assert_eq!(actual.finish_vp9(), reference.finish());
        assert_ne!(reference.finish(), Err(BoolError::UnexpectedEof));
    }
}

#[test]
fn every_refill_boundary_and_truncation_matches_reference() {
    let mut seed = 0xa56f_9083;
    let data: Vec<u8> = (0..1024).map(|_| random(&mut seed) as u8).collect();
    for length in 0..=data.len() {
        let mut actual = BoolDecoder::new(&data[..length]);
        let mut reference = Reference::new(&data[..length]);
        for _ in 0..length * 8 + 64 {
            let probability = random(&mut seed) as u8;
            assert_eq!(actual.read_bool(probability), reference.read(probability));
            if reference.position >= length.saturating_sub(1) * 8 {
                assert_eq!(actual.finish_vp9(), reference.finish(), "length={length} consumed={}", reference.position);
            }
        }
    }
}

#[test]
fn literals_flags_and_signed_fields_match_reference() {
    let mut seed = 0x1837_8495;
    let data: Vec<u8> = (0..8192).map(|_| random(&mut seed) as u8).collect();
    let mut actual = BoolDecoder::new(&data);
    let mut reference = Reference::new(&data);
    for width in (0..=32).cycle().take(2048) {
        assert_eq!(actual.read_literal(width), reference.literal(width));
        assert_eq!(actual.read_bool_half(), reference.read(128));
        assert_eq!(actual.read_flag(), reference.read(128));
        let magnitude = reference.literal(width.min(31)) as i32;
        let signed = if reference.read(128) { -magnitude } else { magnitude };
        assert_eq!(actual.read_signed(width), signed);
        let optional = if reference.read(128) {
            let magnitude = reference.literal(width.min(31)) as i32;
            if reference.read(128) { -magnitude } else { magnitude }
        } else {
            0
        };
        assert_eq!(actual.read_optional_signed(width), optional);
    }
    assert_eq!(actual.read_literal(u32::MAX), reference.literal(32));
}

#[test]
fn trees_and_subtrees_match_reference() {
    let tree = [0, 2, -1, 4, -2, -3];
    let probabilities = [23, 128, 240];
    let mut seed = 0x5f67_8301;
    let data: Vec<u8> = (0..4096).map(|_| random(&mut seed) as u8).collect();
    let mut actual = BoolDecoder::new(&data);
    let mut reference = Reference::new(&data);
    for start in [0, 2, 4].into_iter().cycle().take(20_000) {
        let mut node = start;
        let leaf = loop {
            let next: i8 = tree[node + usize::from(reference.read(probabilities[node / 2]))];
            if next <= 0 {
                break next.unsigned_abs();
            }
            node = next as usize;
        };
        assert_eq!(actual.read_tree_from(&tree, &probabilities, start), leaf);
    }
    assert_eq!(BoolDecoder::new(&[]).read_tree(&[-128, -128], &[128]), 128);
}

#[test]
fn malformed_trees_and_hostile_widths_do_not_panic_or_loop() {
    for (tree, probabilities, start) in [(&[2, 2, 2, 2][..], &[128, 128][..], 0), (&[], &[], 0), (&[1, 1], &[128], usize::MAX)] {
        let mut decoder = BoolDecoder::new(&[0; 128]);
        assert_eq!(decoder.read_tree_from(tree, probabilities, start), 0);
        assert_eq!(decoder.finish_vp9(), Err(BoolError::InvalidTree));
    }
    let mut decoder = BoolDecoder::new(&[255; 64]);
    decoder.read_signed(u32::MAX);
    decoder.read_optional_signed(u32::MAX);
}

#[test]
fn vp9_marker_padding_and_exhaustion() {
    assert!(matches!(BoolDecoder::new_vp9(&[]), Err(BoolError::UnexpectedEof)));
    for byte in 0..=255u8 {
        let data = [byte];
        let result = BoolDecoder::new_vp9(&data);
        if byte < 128 {
            assert_eq!(result.unwrap().finish_vp9(), Ok(()));
        } else {
            assert!(matches!(result, Err(BoolError::InvalidMarker)));
        }
    }
    for length in 1..=32 {
        let data = vec![0; length];
        assert_eq!(BoolDecoder::new_vp9(&data).unwrap().finish_vp9(), Ok(()));
        for bit in 8..length * 8 {
            let mut mutated = data.clone();
            mutated[bit / 8] |= 1 << (7 - bit % 8);
            assert_eq!(BoolDecoder::new_vp9(&mutated).unwrap().finish_vp9(), Err(BoolError::NonzeroPadding));
        }
    }
    let mut decoder = BoolDecoder::new_vp9(&[0]).unwrap();
    assert!(!decoder.read_bool(128));
    assert_eq!(decoder.finish_vp9(), Err(BoolError::UnexpectedEof));
    for _ in 0..1_000_000 {
        assert!(!decoder.read_bool(0));
    }
    assert_eq!(decoder.finish_vp9(), Err(BoolError::UnexpectedEof));
}
