use super::*;
use std::io::{Cursor, Read, Write};

fn limits(wrapper: Wrapper, max_output: u64) -> Limits {
    Limits { max_output, max_input: 1 << 20, max_ratio: 100_000, wrapper }
}

fn encoded(data: &[u8], wrapper: Wrapper) -> Vec<u8> {
    use flate2::Compression;
    match wrapper {
        Wrapper::Raw => {
            let mut encoder = flate2::write::DeflateEncoder::new(Vec::new(), Compression::default());
            assert!(encoder.write_all(data).is_ok());
            encoder.finish().unwrap_or_default()
        }
        Wrapper::Zlib | Wrapper::Auto => {
            let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), Compression::default());
            assert!(encoder.write_all(data).is_ok());
            encoder.finish().unwrap_or_default()
        }
        Wrapper::Gzip => {
            let mut encoder = flate2::write::GzEncoder::new(Vec::new(), Compression::default());
            assert!(encoder.write_all(data).is_ok());
            encoder.finish().unwrap_or_default()
        }
    }
}

#[test]
fn wrappers_and_exact_output_boundary() {
    for wrapper in [Wrapper::Raw, Wrapper::Zlib, Wrapper::Gzip] {
        let compressed = encoded(b"hello world", wrapper);
        for mode in [wrapper, Wrapper::Auto] {
            assert_eq!(inflate_bounded(&compressed, limits(mode, 11)).ok().as_deref(), Some(&b"hello world"[..]));
            assert!(matches!(inflate_bounded(&compressed, limits(mode, 10)), Err(InflateError::OutputLimit { limit: 10 })));
        }
    }
}

#[test]
fn streaming_is_independent_of_buffer_size() -> Result<(), Box<dyn std::error::Error>> {
    let data = vec![42u8; 32_768];
    let compressed = encoded(&data, Wrapper::Zlib);
    for size in [1, 17, 8192] {
        let mut reader = BoundedInflate::new(compressed.as_slice(), limits(Wrapper::Auto, data.len() as u64))?;
        let mut chunk = vec![0; size];
        let mut output = Vec::new();
        loop {
            let count = reader.read_chunk(&mut chunk)?;
            if count == 0 {
                break;
            }
            output.extend_from_slice(&chunk[..count]);
        }
        assert_eq!(output, data);
        assert_eq!(reader.accounting(), Accounting { bytes_in: compressed.len() as u64, bytes_out: data.len() as u64 });
    }
    Ok(())
}

#[test]
fn input_and_ratio_boundaries() -> Result<(), Box<dyn std::error::Error>> {
    let data = encoded(b"hello world", Wrapper::Zlib);
    let mut allowed = limits(Wrapper::Zlib, 11);
    allowed.max_input = data.len() as u64;
    assert!(inflate_bounded(&data, allowed).is_ok());
    allowed.max_input -= 1;
    assert!(matches!(inflate_bounded(&data, allowed), Err(InflateError::InputLimit { .. })));
    allowed.max_input = 0;
    allowed.wrapper = Wrapper::Auto;
    assert!(matches!(inflate_bounded(&data, allowed), Err(InflateError::InputLimit { limit: 0 })));

    let mut ratio_boundary = limits(Wrapper::Zlib, 11);
    ratio_boundary.max_ratio = 1;
    assert_eq!(inflate_bounded(&data, ratio_boundary).ok().as_deref(), Some(&b"hello world"[..]));
    ratio_boundary.max_ratio = 0;
    assert!(matches!(inflate_bounded(&data, ratio_boundary), Err(InflateError::RatioLimit)));

    let bomb = encoded(&vec![0; 1 << 20], Wrapper::Zlib);
    let mut ratio = limits(Wrapper::Zlib, 1 << 30);
    ratio.max_ratio = 1;
    assert!(matches!(inflate_bounded(&bomb, ratio), Err(InflateError::RatioLimit)));
    assert!(bomb.len() < 1 << 15);
    let mut reader = BoundedInflate::new(bomb.as_slice(), ratio)?;
    let mut output = [0; 1024];
    assert!(matches!(reader.read_chunk(&mut output), Err(InflateError::RatioLimit)));
    assert!(reader.accounting().bytes_in > 0);
    assert_eq!(reader.accounting().bytes_out, 0);
    Ok(())
}

#[test]
fn truncation_and_corruption() {
    for wrapper in [Wrapper::Raw, Wrapper::Zlib, Wrapper::Gzip] {
        let mut data = encoded(b"hello world", wrapper);
        data.truncate(data.len() / 2);
        assert!(matches!(inflate_bounded(&data, limits(wrapper, 100)), Err(InflateError::Truncated)));
    }
    let mut gzip = encoded(b"hello world", Wrapper::Gzip);
    if let Some(byte) = gzip.last_mut() {
        *byte ^= 1;
    }
    assert!(matches!(inflate_bounded(&gzip, limits(Wrapper::Gzip, 100)), Err(InflateError::Corrupt)));
    let both = [encoded(b"first", Wrapper::Gzip), encoded(b"second", Wrapper::Gzip)].concat();
    assert_eq!(inflate_bounded(&both, limits(Wrapper::Gzip, 11)).ok().as_deref(), Some(&b"firstsecond"[..]));
}

#[test]
fn gigabyte_bomb_is_rejected_without_gigabyte_allocation() -> Result<(), Box<dyn std::error::Error>> {
    let zeros = [0u8; 1 << 20];
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    for _ in 0..1024 {
        encoder.write_all(&zeros)?;
    }
    let compressed = encoder.finish()?;
    assert!((1 << 19..2 << 20).contains(&compressed.len()));
    let limits = Limits { max_output: 1 << 30, max_input: compressed.len() as u64, max_ratio: 1, wrapper: Wrapper::Zlib };
    assert!(matches!(inflate_bounded(&compressed, limits), Err(InflateError::RatioLimit)));
    Ok(())
}

// Verbatim bounded wrapper from vectorcraft/crates/eps/src/ps.rs:286.
fn reference_inflate(data: &[u8], max: u64) -> Option<Vec<u8>> {
    let mut out = vec![];
    flate2::read::ZlibDecoder::new(data).take(max + 1).read_to_end(&mut out).ok()?;
    (out.len() as u64 <= max).then_some(out)
}

proptest::proptest! {
    #[test]
    fn valid_round_trip_and_reference(bytes in proptest::collection::vec(proptest::prelude::any::<u8>(), 0..4096)) {
        let compressed = encoded(&bytes, Wrapper::Zlib);
        let decoded = inflate_bounded(&compressed, limits(Wrapper::Auto, bytes.len() as u64));
        proptest::prop_assert_eq!(decoded.as_ref().ok(), Some(&bytes));
        proptest::prop_assert_eq!(decoded.ok(), reference_inflate(&compressed, bytes.len() as u64));
    }
}

fn zip_bytes(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in entries {
        assert!(writer.start_file(*name, zip::write::SimpleFileOptions::default()).is_ok());
        assert!(writer.write_all(bytes).is_ok());
    }
    writer.finish().map(Cursor::into_inner).unwrap_or_default()
}

fn rename_zip(bytes: &mut [u8], from: &str, to: &str) {
    assert_eq!(from.len(), to.len());
    for offset in 0..bytes.len() {
        let name_offset = if bytes[offset..].starts_with(b"PK\x03\x04") {
            Some(30)
        } else if bytes[offset..].starts_with(b"PK\x01\x02") {
            Some(46)
        } else {
            None
        };
        if let Some(name_offset) = name_offset {
            let start = offset + name_offset;
            if bytes.get(start..start + from.len()) == Some(from.as_bytes()) {
                bytes[start..start + from.len()].copy_from_slice(to.as_bytes());
            }
        }
    }
}

fn zip_limits() -> ZipLimits {
    ZipLimits { max_entries: 2, max_entry_bytes: 8, max_total_bytes: 12 }
}

fn guard(bytes: &[u8], limits: ZipLimits) -> Result<ZipGuard<Cursor<&[u8]>>, ZipError> {
    ZipGuard::new(zip::ZipArchive::new(Cursor::new(bytes))?, limits)
}

#[test]
fn zip_paths_and_limits() {
    for path in ["../x", "/etc/passwd", "C:\\x", "a\\..\\b", "a/../b", "a//b"] {
        let placeholder = "x".repeat(path.len());
        let mut bytes = zip_bytes(&[(&placeholder, b"x")]);
        rename_zip(&mut bytes, &placeholder, path);
        assert!(matches!(guard(&bytes, zip_limits()), Err(ZipError::UnsafePath(_))), "{path}");
    }
    let bytes = zip_bytes(&[("a", b"abc"), ("b", b"def")]);
    let mut reader = guard(&bytes, zip_limits()).ok();
    assert!(reader.is_some());
    let Some(ref mut reader) = reader else { return };
    assert_eq!(reader.read_entry(0).ok().as_deref(), Some(&b"abc"[..]));
    assert_eq!(reader.read_entry(1).ok().as_deref(), Some(&b"def"[..]));
    let mut limited = zip_limits();
    limited.max_entry_bytes = 2;
    assert!(matches!(guard(&bytes, limited), Err(ZipError::EntryLimit)));
    limited = zip_limits();
    limited.max_total_bytes = 5;
    assert!(matches!(guard(&bytes, limited), Err(ZipError::TotalLimit)));
    limited = zip_limits();
    limited.max_entries = 1;
    assert!(matches!(guard(&bytes, limited), Err(ZipError::EntryCount)));
    let mut duplicate = zip_bytes(&[("a", b"x"), ("b", b"y")]);
    rename_zip(&mut duplicate, "b", "a");
    assert!(matches!(guard(&duplicate, zip_limits()), Err(ZipError::Duplicate)));
}

#[test]
fn zip_symlinks_are_rejected() {
    let mut bytes = zip_bytes(&[("link", b"target")]);
    if let Some(offset) = bytes.windows(4).position(|window| window == b"PK\x01\x02") {
        bytes[offset + 5] = 3;
        bytes[offset + 38..offset + 42].copy_from_slice(&(0o120777u32 << 16).to_le_bytes());
    }
    assert!(matches!(guard(&bytes, zip_limits()), Err(ZipError::Symlink(_))));
}

#[test]
fn png_crc_length_and_truncation() -> Result<(), Box<dyn std::error::Error>> {
    let bytes: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\0IEND\xae\x42\x60\x82";
    let mut chunks = PngChunks::new(bytes, 0)?;
    assert_eq!(chunks.next().and_then(Result::ok).map(|chunk| chunk.kind), Some(*b"IEND"));
    assert!(chunks.next().is_none());
    let mut bad = bytes.to_vec();
    if let Some(byte) = bad.last_mut() {
        *byte ^= 1;
    }
    assert!(matches!(PngChunks::new(&bad, 0).ok().and_then(|mut chunks| chunks.next()), Some(Err(PngError::Crc))));
    let mut large = bytes.to_vec();
    large[11] = 1;
    assert!(matches!(PngChunks::new(&large, 0).ok().and_then(|mut chunks| chunks.next()), Some(Err(PngError::LengthLimit { .. }))));
    assert!(matches!(PngChunks::new(&bytes[..12], 0).ok().and_then(|mut chunks| chunks.next()), Some(Err(PngError::Truncated))));
    Ok(())
}

#[test]
fn backslash_names_normalise_only_when_opted_in() {
    let placeholder = "xl_workbook.xml";
    let mut bytes = zip_bytes(&[(placeholder, b"wb"), ("ok.xml", b"ok")]);
    rename_zip(&mut bytes, placeholder, "xl\\workbook.xml");
    assert!(matches!(guard(&bytes, zip_limits()), Err(ZipError::UnsafePath(_))));
    let archive = zip::ZipArchive::new(Cursor::new(&bytes[..])).expect("archive");
    let mut tolerant = ZipGuard::new_with_names(archive, zip_limits(), NameMode::NormalizeBackslash).expect("guard");
    assert_eq!(tolerant.entry_name(0).ok().as_deref(), Some("xl/workbook.xml"));
    assert_eq!(tolerant.entry(0).map(|entry| entry.name).ok().as_deref(), Some("xl/workbook.xml"));
    assert_eq!(tolerant.read_entry(0).ok().as_deref(), Some(&b"wb"[..]));
    let mut reader = ZipReader::open_with_names(&bytes, zip_limits(), NameMode::NormalizeBackslash).expect("reader");
    assert!(reader.contains("xl/workbook.xml"));
    assert_eq!(reader.read("xl/workbook.xml").ok().flatten().as_deref(), Some(&b"wb"[..]));
    assert!(ZipReader::open(&bytes, zip_limits()).is_err());
}

#[test]
fn normalised_backslash_names_still_checked_and_deduplicated() {
    for path in ["..\\x", "a\\..\\b", "\\abs", "a\\\\b"] {
        let placeholder = "x".repeat(path.len());
        let mut bytes = zip_bytes(&[(&placeholder, b"x")]);
        rename_zip(&mut bytes, &placeholder, path);
        assert!(matches!(ZipReader::open_with_names(&bytes, zip_limits(), NameMode::NormalizeBackslash), Err(ZipError::UnsafePath(_))), "{path}");
    }
    let mut bytes = zip_bytes(&[("a/b", b"x"), ("a_b", b"y")]);
    rename_zip(&mut bytes, "a_b", "a\\b");
    assert!(matches!(ZipReader::open_with_names(&bytes, zip_limits(), NameMode::NormalizeBackslash), Err(ZipError::Duplicate)));
}
