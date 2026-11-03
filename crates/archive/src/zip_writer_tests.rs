use super::*;
use std::io::{Cursor, Read, Seek, SeekFrom, Write};

fn verify(bytes: Vec<u8>, expected: &[(&str, &[u8])]) -> Result<(), Box<dyn std::error::Error>> {
    let mut archive = zip::ZipArchive::new(Cursor::new(&bytes))?;
    assert_eq!(archive.len(), expected.len());
    for (index, &(name, data)) in expected.iter().enumerate() {
        let mut file = archive.by_index(index)?;
        assert_eq!(file.name()?.as_ref(), name);
        let mut content = Vec::new();
        file.read_to_end(&mut content)?;
        assert_eq!(content, data);
    }
    let limits = ZipLimits { max_entries: expected.len(), max_entry_bytes: 1 << 20, max_total_bytes: 1 << 24 };
    let mut guard = ZipGuard::new(zip::ZipArchive::new(Cursor::new(bytes.as_slice()))?, limits)?;
    for (index, &(_, data)) in expected.iter().enumerate() {
        assert_eq!(guard.read_entry(index)?, data);
    }
    let mut reader = ZipReader::open(&bytes, limits)?;
    assert_eq!(reader.names().iter().map(String::as_str).collect::<Vec<_>>(), expected.iter().map(|entry| entry.0).collect::<Vec<_>>());
    for &(name, data) in expected {
        assert!(reader.contains(name));
        assert_eq!(reader.read(name)?.as_deref(), Some(data));
    }
    assert_eq!(reader.read("missing")?, None);
    Ok(())
}

#[test]
fn stored_deflated_and_unicode() -> Result<(), Box<dyn std::error::Error>> {
    let mut writer = ZipWriter::in_memory();
    writer.add_stored("folder/a.txt", b"hello")?;
    writer.set_time(DosTime { time: 0, date: 0x21 });
    writer.add_deflated("caf\u{e9}.txt", b"repeated repeated repeated", 6)?;
    let bytes = writer.finish()?.into_inner();
    assert_eq!(u16::from_le_bytes([bytes[6], bytes[7]]), 0);
    let second = 30 + "folder/a.txt".len() + 5;
    assert_eq!(u16::from_le_bytes([bytes[second + 6], bytes[second + 7]]), 1 << 11);
    assert_eq!(u16::from_le_bytes([bytes[second + 8], bytes[second + 9]]), 8);
    assert_eq!(&bytes[bytes.len() - 22..bytes.len() - 18], b"PK\x05\x06");
    verify(bytes, &[("folder/a.txt", b"hello"), ("caf\u{e9}.txt", b"repeated repeated repeated")])
}

#[test]
fn empty_archive() -> Result<(), Box<dyn std::error::Error>> {
    let bytes = ZipWriter::in_memory().finish()?.into_inner();
    assert_eq!(bytes.len(), 22);
    verify(bytes, &[])
}

#[test]
fn optional_store_on_expansion() -> Result<(), Box<dyn std::error::Error>> {
    let mut writer = ZipWriter::in_memory();
    writer.add_deflated_if_smaller("tiny", b"x", 6)?;
    let bytes = writer.finish()?.into_inner();
    assert_eq!(&bytes[8..10], &[0, 0]);
    verify(bytes, &[("tiny", b"x")])
}

#[test]
fn cancelled_compression_does_not_add_entry() -> Result<(), Box<dyn std::error::Error>> {
    let mut writer = ZipWriter::in_memory();
    let mut checks = 0;
    let result = writer.add_deflated_if_smaller_with_stop("cancelled", &vec![0; 2 << 20], 6, || {
        checks += 1;
        checks == 2
    });
    assert_eq!(result.err().map(|error| error.kind()), Some(std::io::ErrorKind::Interrupted));
    assert_eq!(checks, 2);
    writer.add_stored("good", b"yes")?;
    verify(writer.finish()?.into_inner(), &[("good", b"yes")])
}

#[test]
fn zip64_entry_count() -> Result<(), Box<dyn std::error::Error>> {
    let mut writer = ZipWriter::in_memory();
    for index in 0..65_536 {
        writer.add_stored(&format!("{index:05}"), b"")?;
    }
    let bytes = writer.finish()?.into_inner();
    let eocd = bytes.len() - 22;
    assert_eq!(&bytes[eocd - 20..eocd - 16], b"PK\x06\x07");
    assert_eq!(&bytes[eocd - 76..eocd - 72], b"PK\x06\x06");
    assert_eq!(u16::from_le_bytes([bytes[eocd + 10], bytes[eocd + 11]]), u16::MAX);
    let mut archive = zip::ZipArchive::new(Cursor::new(&bytes))?;
    assert_eq!(archive.len(), 65_536);
    assert_eq!(archive.by_index(65_535)?.name()?.as_ref(), "65535");
    let mut guard =
        ZipGuard::new(zip::ZipArchive::new(Cursor::new(bytes))?, ZipLimits { max_entries: 65_536, max_entry_bytes: 0, max_total_bytes: 0 })?;
    assert_eq!(guard.len(), 65_536);
    assert_eq!(guard.read_entry(65_535)?, b"");
    Ok(())
}

#[test]
fn seeded_round_trips() -> Result<(), Box<dyn std::error::Error>> {
    let mut state = 0x1234_5678_9abc_def0u64;
    for index in 0..128 {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let name = format!("dir/{index}-{:x}.bin", state & 0xffff);
        let size = (state % 1024) as usize;
        let mut data = Vec::with_capacity(size);
        for _ in 0..size {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            data.push(state as u8);
        }
        let mut writer = ZipWriter::in_memory();
        writer.add(&name, &data, if index % 2 == 0 { Method::Store } else { Method::Deflate(6) })?;
        verify(writer.finish()?.into_inner(), &[(name.as_str(), &data)])?;
    }
    Ok(())
}

#[test]
fn limits_and_names_do_not_corrupt_writer() -> Result<(), Box<dyn std::error::Error>> {
    let limits = ZipWriteLimits { max_entries: 1, max_entry_bytes: 3, max_total_bytes: 2 };
    let mut writer = ZipWriter::with_limits(Cursor::new(Vec::new()), limits);
    for name in ["", "/absolute", "a/../b", "a\\b", "C:x", "a//b", "a/./b", "a\0b"] {
        assert!(writer.add_stored(name, b"x").is_err());
    }
    assert!(writer.add_stored("large", b"1234").is_err());
    assert!(writer.add_stored("total", b"123").is_err());
    assert!(writer.add_deflated("bad-level", b"x", 10).is_err());
    writer.add_stored("ok", b"xy")?;
    assert!(writer.add_stored("ok", b"").is_err());
    assert!(writer.add_stored("other", b"").is_err());
    verify(writer.finish()?.into_inner(), &[("ok", b"xy")])
}

#[test]
fn classic_layout_and_timestamp() -> Result<(), Box<dyn std::error::Error>> {
    let mut writer = ZipWriter::in_memory();
    writer.set_time(DosTime { time: 0x1234, date: 0x5678 });
    writer.add_stored("x", b"1")?;
    let bytes = writer.finish()?.into_inner();
    assert_eq!(bytes.len(), 30 + 1 + 1 + 46 + 1 + 22);
    assert_eq!(&bytes[10..14], &[0x34, 0x12, 0x78, 0x56]);
    assert_eq!(&bytes[32..36], b"PK\x01\x02");
    verify(bytes, &[("x", b"1")])
}

#[test]
fn zip64_offset_uses_extra_field() -> Result<(), Box<dyn std::error::Error>> {
    struct Sparse {
        base: u64,
        position: u64,
        bytes: Vec<u8>,
    }
    impl Write for Sparse {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            let start = (self.position - self.base) as usize;
            self.bytes.resize(start + bytes.len(), 0);
            self.bytes[start..].copy_from_slice(bytes);
            self.position += bytes.len() as u64;
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    impl Seek for Sparse {
        fn seek(&mut self, target: SeekFrom) -> std::io::Result<u64> {
            self.position = match target {
                SeekFrom::Start(offset) => offset,
                SeekFrom::Current(delta) => self.position.checked_add_signed(delta).ok_or(std::io::Error::other("seek overflow"))?,
                SeekFrom::End(delta) => {
                    (self.base + self.bytes.len() as u64).checked_add_signed(delta).ok_or(std::io::Error::other("seek overflow"))?
                }
            };
            Ok(self.position)
        }
    }
    let base = u32::MAX as u64 + 1;
    let mut writer = ZipWriter::new(Sparse { base, position: base, bytes: Vec::new() });
    writer.add_stored("x", b"y")?;
    let output = writer.finish()?;
    let central = &output.bytes[32..];
    assert_eq!(&central[..4], b"PK\x01\x02");
    assert_eq!(&central[42..46], &u32::MAX.to_le_bytes());
    assert_eq!(&central[47..51], &[1, 0, 8, 0]);
    assert_eq!(&central[51..59], &base.to_le_bytes());
    Ok(())
}
