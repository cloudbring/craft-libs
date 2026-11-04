//! Bounded name-based reads from an in-memory ZIP archive.

use std::io::Cursor;

use crate::{NameMode, ZipEntry, ZipError, ZipGuard, ZipLimits};

/// Validated in-memory ZIP entries in central-directory order.
pub struct ZipReader<'a> {
    guard: ZipGuard<Cursor<&'a [u8]>>,
    names: Vec<String>,
    entries: Vec<ZipEntry>,
}

impl<'a> ZipReader<'a> {
    /// Rejects unsafe names, duplicates, symlinks and declared size violations.
    pub fn open(bytes: &'a [u8], limits: ZipLimits) -> Result<Self, ZipError> {
        Self::open_with_names(bytes, limits, NameMode::Strict)
    }

    /// Like [`open`](Self::open) with an explicit [`NameMode`].
    pub fn open_with_names(bytes: &'a [u8], limits: ZipLimits, mode: NameMode) -> Result<Self, ZipError> {
        let archive = zip::ZipArchive::new(Cursor::new(bytes))?;
        let mut guard = ZipGuard::new_with_names(archive, limits, mode)?;
        let mut names = Vec::new();
        let mut entries = Vec::new();
        names.try_reserve(guard.len()).map_err(|_| ZipError::EntryCount)?;
        entries.try_reserve(guard.len()).map_err(|_| ZipError::EntryCount)?;
        for index in 0..guard.len() {
            let entry = guard.entry(index)?;
            names.push(entry.name.clone());
            entries.push(entry);
        }
        Ok(Self { guard, names, entries })
    }

    pub fn names(&self) -> &[String] {
        &self.names
    }

    pub fn contains(&self, name: &str) -> bool {
        self.names.iter().any(|entry| entry == name)
    }

    /// Returns validated metadata for a named entry.
    pub fn entry(&self, name: &str) -> Option<&ZipEntry> {
        self.entries.iter().find(|entry| entry.name == name)
    }

    /// Reads one named entry, checking decoded size and CRC through [`ZipGuard`].
    pub fn read(&mut self, name: &str) -> Result<Option<Vec<u8>>, ZipError> {
        self.names.iter().position(|entry| entry == name).map(|index| self.guard.read_entry(index)).transpose()
    }

    /// Reads one named entry with an additional per-entry decoded-byte cap.
    pub fn read_bounded(&mut self, name: &str, max_entry_bytes: u64) -> Result<Option<Vec<u8>>, ZipError> {
        self.names.iter().position(|entry| entry == name).map(|index| self.guard.read_entry_bounded(index, max_entry_bytes)).transpose()
    }
}
