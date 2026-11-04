//! Bounded name-based reads from an in-memory ZIP archive.

use std::borrow::Cow;
use std::cell::OnceCell;
use std::io::Cursor;

use crate::{NameMode, ZipEntry, ZipError, ZipGuard, ZipLimits};

/// Validated in-memory ZIP entries in central-directory order.
///
/// Opening validates every entry but allocates nothing per entry; the name list and entry metadata
/// that [`names`](Self::names) and [`entry`](Self::entry) hand out by reference are built on first use.
pub struct ZipReader<'a> {
    guard: ZipGuard<Cursor<Cow<'a, [u8]>>>,
    names: OnceCell<Vec<String>>,
    entries: OnceCell<Vec<ZipEntry>>,
}

impl<'a> ZipReader<'a> {
    /// Rejects unsafe names, duplicates, symlinks and declared size violations.
    pub fn open(bytes: &'a [u8], limits: ZipLimits) -> Result<Self, ZipError> {
        Self::open_with_names(bytes, limits, NameMode::Strict)
    }

    /// Like [`open`](Self::open) with an explicit [`NameMode`].
    pub fn open_with_names(bytes: &'a [u8], limits: ZipLimits, mode: NameMode) -> Result<Self, ZipError> {
        let archive = zip::ZipArchive::new(Cursor::new(find_end_record(bytes)))?;
        let guard = ZipGuard::new_with_names(archive, limits, mode)?;
        Ok(Self { guard, names: OnceCell::new(), entries: OnceCell::new() })
    }

    fn entries(&self) -> &[ZipEntry] {
        self.entries.get_or_init(|| (0..self.guard.len()).filter_map(|index| self.guard.entry_at(index).ok()).collect())
    }

    /// Entry names in central-directory order.
    pub fn names(&self) -> &[String] {
        self.names.get_or_init(|| self.entries().iter().map(|entry| entry.name.clone()).collect())
    }

    pub fn contains(&self, name: &str) -> bool {
        self.guard.index_of(name).is_some()
    }

    /// Returns validated metadata for a named entry.
    pub fn entry(&self, name: &str) -> Option<&ZipEntry> {
        self.guard.index_of(name).and_then(|index| self.entries().get(index))
    }

    /// Reads one named entry, checking decoded size and CRC through [`ZipGuard`].
    pub fn read(&mut self, name: &str) -> Result<Option<Vec<u8>>, ZipError> {
        self.guard.index_of(name).map(|index| self.guard.read_entry(index)).transpose()
    }

    /// Reads one named entry with an additional per-entry decoded-byte cap.
    pub fn read_bounded(&mut self, name: &str, max_entry_bytes: u64) -> Result<Option<Vec<u8>>, ZipError> {
        self.guard.index_of(name).map(|index| self.guard.read_entry_bounded(index, max_entry_bytes)).transpose()
    }
}

const END_SIGNATURE: [u8; 4] = 0x0605_4b50u32.to_le_bytes();
const END_LEN: usize = 22;

/// Chooses the end-of-central-directory record the way the pre-swap readers did, so a comment that
/// happens to contain the signature cannot hide the real directory. The `zip` crate takes the last
/// signature that parses, which for a comment holding `PK\x05\x06` plus zeros is a bogus "empty
/// archive". Candidates are scanned backwards; the first whose directory is consistent wins, with an
/// exact-fit comment preferred over a trailing-garbage one, and a non-empty directory over an "empty archive" (all zeros is what a signature in a comment usually looks like). When that is the last signature (every
/// ordinary archive) the bytes pass through untouched; otherwise a copy ending at the chosen record
/// (comment length zeroed) is returned, offsets unchanged.
fn find_end_record(bytes: &[u8]) -> Cow<'_, [u8]> {
    let Some(last) = bytes.len().checked_sub(END_LEN) else { return Cow::Borrowed(bytes) };
    let u16_at = |at: usize| usize::from(u16::from_le_bytes([bytes[at], bytes[at + 1]]));
    let u32_at = |at: usize| u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]);
    let candidates = || (last.saturating_sub(0xFFFF)..=last).rev().filter(|&at| bytes[at..at + 4] == END_SIGNATURE);
    let consistent = |at: usize| {
        let (count, size, offset) = (u16_at(at + 10), u32_at(at + 12), u32_at(at + 16));
        if count == 0xFFFF || size == u32::MAX || offset == u32::MAX {
            return at >= 20 && bytes[at - 20..at - 16] == 0x0706_4b50u32.to_le_bytes();
        }
        let (size, offset) = (size as usize, offset as usize);
        if count == 0 {
            return size == 0 && offset <= at;
        }
        at.checked_sub(size).is_some_and(|start| offset <= start && bytes.get(start..start + 4) == Some(&0x0201_4b50u32.to_le_bytes()))
    };
    let exact = |at: usize| at + END_LEN + u16_at(at + 20) == bytes.len();
    let fits = |at: usize| at + END_LEN + u16_at(at + 20) <= bytes.len();
    let nonempty = |at: usize| u16_at(at + 10) != 0;
    let Some(first) = candidates().next() else { return Cow::Borrowed(bytes) };
    let chosen = candidates()
        .find(|&at| exact(at) && nonempty(at) && consistent(at))
        .or_else(|| candidates().find(|&at| fits(at) && nonempty(at) && consistent(at)))
        .or_else(|| candidates().find(|&at| exact(at) && consistent(at)));
    match chosen {
        Some(at) if at != first => {
            let mut copy = bytes[..at + END_LEN].to_vec();
            copy[at + 20..at + 22].fill(0);
            Cow::Owned(copy)
        }
        _ => Cow::Borrowed(bytes),
    }
}
