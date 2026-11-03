use std::io::{Read, Seek};

/// Limits for a ZIP archive. Declared sizes are checked before reading, and
/// actual decoded bytes are checked while reading each entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ZipLimits {
    pub max_entries: usize,
    pub max_entry_bytes: u64,
    pub max_total_bytes: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum ZipError {
    #[error("invalid ZIP: {0}")]
    Archive(#[from] zip::result::ZipError),
    #[error("ZIP I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("ZIP has too many entries")]
    EntryCount,
    #[error("ZIP entry exceeds byte limit")]
    EntryLimit,
    #[error("ZIP total exceeds byte limit")]
    TotalLimit,
    #[error("unsafe ZIP entry path: {0}")]
    UnsafePath(String),
    #[error("ZIP symlink is not permitted: {0}")]
    Symlink(String),
    #[error("duplicate ZIP entry names")]
    Duplicate,
}

/// A validated ZIP reader. Exposes entry bytes, never extraction paths.
pub struct ZipGuard<R: Read + Seek> {
    archive: zip::ZipArchive<R>,
    limits: ZipLimits,
    total_read: u64,
}

impl<R: Read + Seek> ZipGuard<R> {
    /// Validates all central-directory names and declared sizes before exposing entries.
    pub fn new(mut archive: zip::ZipArchive<R>, limits: ZipLimits) -> Result<Self, ZipError> {
        if archive.has_duplicate_names() {
            return Err(ZipError::Duplicate);
        }
        if archive.len() > limits.max_entries {
            return Err(ZipError::EntryCount);
        }
        let mut total = 0u64;
        for index in 0..archive.len() {
            let entry = archive.by_index(index)?;
            let name = entry.name()?;
            if !safe_name(&name) {
                return Err(ZipError::UnsafePath(name.to_string()));
            }
            if entry.unix_mode().is_some_and(|mode| mode & 0o170000 == 0o120000) {
                return Err(ZipError::Symlink(name.to_string()));
            }
            if entry.size() > limits.max_entry_bytes {
                return Err(ZipError::EntryLimit);
            }
            total = total.checked_add(entry.size()).ok_or(ZipError::TotalLimit)?;
            if total > limits.max_total_bytes {
                return Err(ZipError::TotalLimit);
            }
        }
        Ok(Self { archive, limits, total_read: 0 })
    }

    pub fn len(&self) -> usize {
        self.archive.len()
    }
    pub fn is_empty(&self) -> bool {
        self.archive.is_empty()
    }
    pub fn total_read(&self) -> u64 {
        self.total_read
    }

    /// Reads an entry to completion, including its CRC check, without exceeding caps.
    pub fn read_entry(&mut self, index: usize) -> Result<Vec<u8>, ZipError> {
        let mut entry = self.archive.by_index(index)?;
        let mut output = Vec::new();
        let mut buffer = [0; 8192];
        loop {
            let remaining_entry = self.limits.max_entry_bytes - output.len() as u64;
            let remaining_total = self.limits.max_total_bytes - self.total_read;
            let chunk = buffer.len().min(usize::try_from(remaining_entry.min(remaining_total).saturating_add(1)).unwrap_or(usize::MAX));
            let count = entry.read(&mut buffer[..chunk])?;
            if count == 0 {
                return Ok(output);
            }
            if count as u64 > remaining_entry {
                return Err(ZipError::EntryLimit);
            }
            if count as u64 > remaining_total {
                return Err(ZipError::TotalLimit);
            }
            output.try_reserve_exact(count).map_err(|_| ZipError::EntryLimit)?;
            output.extend_from_slice(&buffer[..count]);
            self.total_read += count as u64;
        }
    }
}

fn safe_name(name: &str) -> bool {
    let path = name.strip_suffix('/').unwrap_or(name);
    !path.is_empty()
        && !name.starts_with('/')
        && !name.contains('\\')
        && !name.chars().any(char::is_control)
        && !name.as_bytes().get(1).is_some_and(|colon| *colon == b':')
        && path.split('/').all(|part| !part.is_empty() && part != "." && part != "..")
}
