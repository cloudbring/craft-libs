use super::*;

fn fixture_tables() -> Vec<Huffman> {
    let mut found = Vec::new();
    for directory in ["tests/data", "fuzz/corpus/decode"] {
        let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(directory);
        let mut paths: Vec<_> = std::fs::read_dir(directory).expect("fixture directory").map(|path| path.expect("fixture").path()).collect();
        paths.sort();
        for path in paths {
            if path.extension().is_none_or(|extension| extension != "jpg") {
                continue;
            }
            let bytes = std::fs::read(path).expect("fixture bytes");
            let mut position = 2;
            while position + 4 <= bytes.len() {
                assert_eq!(bytes[position], 0xff);
                let marker = bytes[position + 1];
                if marker == 0xd9 {
                    break;
                }
                if marker == 0xff {
                    position += 1;
                    continue;
                }
                let length = usize::from(be16(&bytes, position + 2).expect("segment length"));
                let segment = &bytes[position + 4..position + 2 + length];
                if marker == 0xc4 {
                    let mut offset = 0;
                    while offset < segment.len() {
                        let counts = &segment[offset + 1..offset + 17];
                        let total = counts.iter().map(|&count| usize::from(count)).sum::<usize>();
                        let values = &segment[offset + 17..offset + 17 + total];
                        found.push(Huffman::new(counts, values).expect("fixture Huffman table"));
                        offset += 17 + total;
                    }
                }
                position += 2 + length;
                if marker == 0xda {
                    position = scan_end(&bytes, position);
                }
            }
        }
    }
    assert!(!found.is_empty());
    found
}

fn stuff(bytes: &[u8]) -> Vec<u8> {
    let mut stuffed = Vec::new();
    for &byte in bytes {
        stuffed.push(byte);
        if byte == 0xff {
            stuffed.push(0);
        }
    }
    stuffed
}

fn assert_equivalent(table: &Huffman, stream: &[u8], offset: u32, expected: Option<u8>) {
    let mut fast = Bits::new(stream);
    let mut serial = Bits::new(stream);
    fast.get(offset);
    serial.get(offset);
    let actual = table.decode(&mut fast);
    let reference = table.decode_serial(&mut serial);
    assert_eq!(actual.as_ref().map_err(ToString::to_string), reference.as_ref().map_err(ToString::to_string));
    if let Some(expected) = expected {
        assert_eq!(actual.expect("valid code"), expected);
        assert!(!fast.exhausted);
    }
    assert_eq!(fast.exhausted, serial.exhausted);
    for _ in 0..64 {
        assert_eq!(fast.bit(), serial.bit(), "remaining bits differ");
        assert_eq!(fast.exhausted, serial.exhausted, "padding consumed differently");
    }
    assert_eq!(fast.pos, serial.pos, "marker position differs");
}

#[test]
fn every_fixture_code_matches_serial_at_every_alignment() {
    let tables = fixture_tables();
    let mut checked = 0;
    let mut short = 0;
    let mut long = 0;
    for table in &tables {
        for length in 1..=16 {
            for code in table.mincode[length]..=table.maxcode[length] {
                let index = (table.valptr[length] + code - table.mincode[length]) as usize;
                let value = table.values[index];
                if length <= LOOKAHEAD as usize {
                    short += 1;
                } else {
                    long += 1;
                }
                for offset in 0..8 {
                    for suffix in [0, u32::MAX] {
                        let shift = 32 - offset - length;
                        let word = ((code as u32) << shift) | (suffix & ((1 << shift) - 1));
                        let stream = stuff(&word.to_be_bytes());
                        assert_equivalent(table, &stream, offset as u32, Some(value));
                        checked += 1;
                    }
                }
            }
        }
    }
    assert!(short > 0 && long > 0);
    eprintln!("{} fixture tables, {short} short and {long} long codes, {checked} aligned code checks", tables.len());
}

#[test]
fn every_fixture_prefix_matches_serial_including_invalid_codes() {
    for table in fixture_tables() {
        for prefix in 0..=u16::MAX {
            let word = (u32::from(prefix) << 16) | 0xa55a;
            assert_equivalent(&table, &stuff(&word.to_be_bytes()), 0, None);
        }
    }
}

#[test]
fn lookahead_preserves_truncation_stuffing_and_marker_boundaries() {
    for table in fixture_tables() {
        for stream in [
            &[][..],
            &[0],
            &[0xff],
            &[0xff, 0],
            &[0xff, 0xff, 0],
            &[0xff, 0xd9],
            &[0, 0xff, 0xff, 0xd9],
            &[0xff, 0, 0xff, 0xd0, 0x5a, 0xff, 0xd9],
            &[0xa5, 0xff, 0, 0x5a, 0xff, 0xff, 0xd9],
        ] {
            for end in 0..=stream.len() {
                for offset in 0..8 {
                    assert_equivalent(&table, &stream[..end], offset, None);
                }
            }
        }
    }
}

#[test]
fn short_code_at_end_does_not_consume_lookahead_padding() {
    let mut counts = [0; 16];
    counts[0] = 1;
    let table = Huffman::new(&counts, &[42]).expect("table");
    for ending in [&[][..], &[0xff, 0xd9], &[0xff, 0xd0]] {
        let mut stream = vec![0xfe];
        stream.extend(ending);
        let mut bits = Bits::new(&stream);
        assert_eq!(bits.get(7), 127);
        assert_eq!(table.decode(&mut bits).expect("last real bit"), 42);
        assert!(!bits.exhausted);
        assert_eq!(bits.pos, 1);
        assert_eq!(table.decode(&mut bits).expect("old zero-padding behavior"), 42);
        assert!(bits.exhausted);
    }
}

#[test]
fn lookahead_does_not_read_across_restart_markers() {
    let mut counts = [0; 16];
    counts[0] = 2;
    let table = Huffman::new(&counts, &[10, 20]).expect("table");
    for restart in 0xd0..=0xd7 {
        let stream = [0xa5, 0xff, 0xff, restart, 0xff, 0, 0xff, 0xd9];
        let mut fast = Bits::new(&stream);
        let mut serial = Bits::new(&stream);
        for _ in 0..8 {
            assert_eq!(table.decode(&mut fast).expect("code"), table.decode_serial(&mut serial).expect("serial"));
            assert!(!fast.exhausted);
        }
        fast.restart();
        serial.restart();
        assert_eq!(fast.pos, 4);
        assert_eq!(fast.pos, serial.pos);
        for _ in 0..8 {
            assert_eq!(table.decode(&mut fast).expect("code"), 20);
            assert_eq!(table.decode_serial(&mut serial).expect("serial"), 20);
            assert!(!fast.exhausted);
        }
        assert_eq!(fast.pos, 6);
    }
}

#[test]
fn malformed_tables_keep_canonical_errors() {
    assert!(Huffman::new(&[3], &[1, 2, 3]).is_err());
    assert!(Huffman::new(&[2, 1], &[1, 2, 3]).is_err());
    for table in [Huffman::new(&[0; 16], &[]).expect("empty"), Huffman::new(&[2], &[42]).expect("missing value")] {
        for byte in 0..=u8::MAX {
            assert_equivalent(&table, &stuff(&[byte, byte, byte, byte]), 0, None);
        }
    }
}

#[test]
#[ignore = "manual release-mode Huffman kernel timing"]
fn huffman_lookahead_timing() {
    let table = fixture_tables().into_iter().find(|table| table.values.len() > 100).expect("AC table");
    let mut entropy = Vec::new();
    let mut accumulator = 0u8;
    let mut buffered = 0;
    let mut symbols = 0;
    for length in 1..=16 {
        for code in table.mincode[length]..=table.maxcode[length] {
            for _ in 0..(1 << (16 - length)) {
                for shift in (0..length).rev() {
                    accumulator = (accumulator << 1) | ((code >> shift) & 1) as u8;
                    buffered += 1;
                    if buffered == 8 {
                        entropy.push(accumulator);
                        buffered = 0;
                    }
                }
                symbols += 1;
            }
        }
    }
    if buffered != 0 {
        entropy.push(accumulator << (8 - buffered));
    }
    let entropy = stuff(&entropy);
    let iterations = 200;
    let mut times = Vec::new();
    let mut checksums = Vec::new();
    for fast in [false, true] {
        let start = std::time::Instant::now();
        let mut checksum = 0u64;
        for _ in 0..iterations {
            let mut bits = Bits::new(std::hint::black_box(&entropy));
            for _ in 0..symbols {
                let value = if fast { table.decode(&mut bits) } else { table.decode_serial(&mut bits) };
                checksum += u64::from(value.expect("timing stream"));
            }
            assert!(!bits.exhausted);
        }
        times.push(start.elapsed());
        checksums.push(std::hint::black_box(checksum));
    }
    assert_eq!(checksums[0], checksums[1]);
    eprintln!(
        "{} symbols, prefix-span weighted: serial {:?}, lookahead {:?}, {:.2}x",
        iterations * symbols,
        times[0],
        times[1],
        times[0].as_secs_f64() / times[1].as_secs_f64()
    );
}
