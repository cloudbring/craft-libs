//! A `.pcraft` ZIP adapter over `craft-archive`: STORE and DEFLATE entries, found through the
//! central directory and CRC-checked. The format policy remains local: no ZIP64, encryption or
//! split archives.

use std::cell::RefCell;

use craft_archive::{ZipError, ZipLimits, ZipReader};

use crate::{Budget, Error, malformed};

const CENTRAL_SIG: u32 = 0x0201_4b50;
const EOCD_SIG: u32 = 0x0605_4b50;
const ZIP64_LOCATOR_SIG: u32 = 0x0706_4b50;
const EOCD_LEN: usize = 22;
const CENTRAL_LEN: usize = 46;

fn u16_at(bytes: &[u8], at: usize) -> Option<u16> {
    let slice = bytes.get(at..at.checked_add(2)?)?;
    Some(u16::from_le_bytes([*slice.first()?, *slice.get(1)?]))
}

fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
    let slice = bytes.get(at..at.checked_add(4)?)?;
    Some(u32::from_le_bytes([*slice.first()?, *slice.get(1)?, *slice.get(2)?, *slice.get(3)?]))
}

/// One entry of the central directory.
pub(crate) struct Entry {
    name: String,
    encrypted: bool,
    method: u16,
    size: u64,
}

pub(crate) struct Zip<'a> {
    reader: RefCell<ZipReader<'a>>,
    entries: Vec<Entry>,
}

impl<'a> Zip<'a> {
    pub(crate) fn open(bytes: &'a [u8]) -> Result<Self, Error> {
        reject_unsupported_envelope(bytes)?;
        let reader =
            ZipReader::open(bytes, ZipLimits { max_entries: usize::MAX, max_entry_bytes: u64::MAX, max_total_bytes: u64::MAX }).map_err(malformed)?;
        let entries = reader
            .names()
            .iter()
            .filter_map(|name| reader.entry(name))
            .map(|entry| Entry { name: entry.name.clone(), encrypted: entry.encrypted, method: entry.compression_method, size: entry.size })
            .collect();
        Ok(Self { reader: RefCell::new(reader), entries })
    }

    /// The first entry named `name`.
    pub(crate) fn find(&self, name: &str) -> Option<&Entry> {
        self.entries.iter().find(|entry| entry.name == name)
    }

    /// The entry's bytes, decompressed and CRC-checked by `craft-archive`; their declared size is
    /// checked against `max` and counted against `budget` first.
    pub(crate) fn read(&self, entry: &Entry, max: u64, budget: &mut Budget, what: &str) -> Result<Vec<u8>, Error> {
        if entry.encrypted {
            return Err(Error::Unsupported(format!("{what} is encrypted")));
        }
        if !matches!(entry.method, 0 | 8) {
            return Err(Error::Unsupported(format!("{what} uses ZIP compression method {}", entry.method)));
        }
        if entry.size > max {
            return Err(Error::LimitExceeded(format!("{what} is {} bytes, more than the limit ({max})", entry.size)));
        }
        budget.take(entry.size, what)?;
        self.reader
            .borrow_mut()
            .read_bounded(&entry.name, entry.size)
            .map_err(|error| read_error(error, what))?
            .ok_or_else(|| malformed(format!("the bundle has no {what}")))
    }
}

fn read_error(error: ZipError, what: &str) -> Error {
    match error {
        ZipError::EntryLimit => malformed(format!("{what} has the wrong size")),
        ZipError::Archive(error) if error.to_string().contains("checksum") => malformed(format!("{what} failed its CRC check")),
        error => malformed(format!("{what}: {error}")),
    }
}

fn reject_unsupported_envelope(bytes: &[u8]) -> Result<(), Error> {
    let Some(last) = bytes.len().checked_sub(EOCD_LEN) else {
        return Ok(());
    };
    let eocd = (last.saturating_sub(65535)..=last).rev().find(|&at| {
        let comment_end = u16_at(bytes, at + 20).and_then(|length| at.checked_add(EOCD_LEN)?.checked_add(usize::from(length)));
        let directory_end = u32_at(bytes, at + 16)
            .zip(u32_at(bytes, at + 12))
            .and_then(|(offset, size)| usize::try_from(offset).ok()?.checked_add(usize::try_from(size).ok()?));
        u32_at(bytes, at) == Some(EOCD_SIG) && comment_end == Some(bytes.len()) && directory_end == Some(at)
    });
    let Some(eocd) = eocd else {
        let locator_end = last.saturating_sub(20);
        if (last.saturating_sub(65535 + 20)..=locator_end).any(|at| u32_at(bytes, at) == Some(ZIP64_LOCATOR_SIG)) {
            return Err(Error::Unsupported("ZIP64 archives are not supported".into()));
        }
        return Ok(());
    };
    let disk = u16_at(bytes, eocd + 4);
    let directory_disk = u16_at(bytes, eocd + 6);
    if !matches!((disk, directory_disk), (Some(0), Some(0))) {
        return Err(Error::Unsupported("ZIP archives split across disks are not supported".into()));
    }
    let Some(directory_size) = u32_at(bytes, eocd + 12).and_then(|size| usize::try_from(size).ok()) else {
        return Ok(());
    };
    let Some(mut at) = eocd.checked_sub(directory_size) else {
        return Ok(());
    };
    let Some(count) = u16_at(bytes, eocd + 10).map(usize::from) else {
        return Ok(());
    };
    for _ in 0..count {
        if u32_at(bytes, at) != Some(CENTRAL_SIG) || at.checked_add(CENTRAL_LEN).is_none_or(|end| end > eocd) {
            return Ok(());
        }
        if matches!(u32_at(bytes, at + 20), Some(u32::MAX))
            || matches!(u32_at(bytes, at + 24), Some(u32::MAX))
            || matches!(u32_at(bytes, at + 42), Some(u32::MAX))
        {
            return Err(Error::Unsupported("ZIP64 entries are not supported".into()));
        }
        let Some(name_length) = u16_at(bytes, at + 28).map(usize::from) else {
            return Ok(());
        };
        let Some(extra_length) = u16_at(bytes, at + 30).map(usize::from) else {
            return Ok(());
        };
        let Some(comment_length) = u16_at(bytes, at + 32).map(usize::from) else {
            return Ok(());
        };
        let Some(next) = at
            .checked_add(CENTRAL_LEN)
            .and_then(|value| value.checked_add(name_length))
            .and_then(|value| value.checked_add(extra_length))
            .and_then(|value| value.checked_add(comment_length))
        else {
            return Ok(());
        };
        at = next;
    }
    Ok(())
}

/// Does a local header in `head` name `name`? Walks the local headers from the start, for a
/// head too short to reach the central directory; stops at an entry whose size is only in a data
/// descriptor.
pub(crate) fn local_names_include(head: &[u8], name: &str) -> bool {
    const LOCAL_SIG: u32 = 0x0403_4b50;
    const LOCAL_LEN: usize = 30;

    let mut at = 0usize;
    while u32_at(head, at) == Some(LOCAL_SIG) {
        let (Some(flags), Some(compressed), Some(name_length), Some(extra_length)) =
            (u16_at(head, at + 6), u32_at(head, at + 18), u16_at(head, at + 26), u16_at(head, at + 28))
        else {
            return false;
        };
        let start = at + LOCAL_LEN;
        let end = start + usize::from(name_length);
        if head.get(start..end) == Some(name.as_bytes()) {
            return true;
        }
        if flags & 8 != 0 {
            return false;
        }
        at = match end.checked_add(usize::from(extra_length)).and_then(|value| value.checked_add(compressed as usize)) {
            Some(value) => value,
            None => return false,
        };
    }
    false
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use craft_archive::ZipWriter;

    use super::*;
    use crate::Limits;

    fn archive() -> Result<Vec<u8>, std::io::Error> {
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        writer.add_stored("first", b"ab")?;
        writer.add_stored("second", b"cde")?;
        writer.add_stored("third", b"fghi")?;
        Ok(writer.finish()?.into_inner())
    }

    #[test]
    fn zip64_locator_is_rejected_by_open() -> Result<(), Box<dyn std::error::Error>> {
        let archive = archive()?;
        let end = archive.len() - EOCD_LEN;
        let mut zip64 = archive[..end].to_vec();
        zip64.extend(ZIP64_LOCATOR_SIG.to_le_bytes());
        zip64.extend([0; 16]);
        zip64.extend(&archive[end..]);

        let error = Zip::open(&zip64).err().ok_or_else(|| std::io::Error::other("ZIP64 archive was accepted"))?;
        assert_eq!(error, Error::Unsupported("ZIP64 archives are not supported".into()));
        Ok(())
    }

    #[test]
    fn reads_charge_each_entry_against_the_budget() -> Result<(), Box<dyn std::error::Error>> {
        let archive = archive()?;
        let zip = Zip::open(&archive)?;
        let limits = Limits { max_total: 8, ..Limits::default() };
        let mut budget = Budget::new(&limits);

        for (name, expected, remaining) in [("first", b"ab".as_slice(), 6), ("second", b"cde".as_slice(), 3)] {
            let entry = zip.find(name).ok_or_else(|| std::io::Error::other(format!("missing {name}")))?;
            assert_eq!(zip.read(entry, limits.max_entry, &mut budget, name)?, expected);
            assert_eq!(budget.left(), remaining);
        }

        let entry = zip.find("third").ok_or_else(|| std::io::Error::other("missing third"))?;
        let error = zip.read(entry, limits.max_entry, &mut budget, "third").err().ok_or_else(|| std::io::Error::other("third entry was read"))?;
        assert_eq!(error, Error::LimitExceeded("third: more than max_total (8 bytes) in all".into()));
        assert_eq!(budget.left(), 3);
        Ok(())
    }
}
