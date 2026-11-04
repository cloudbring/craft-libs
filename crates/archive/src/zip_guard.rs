use std::io::{Read, Seek};

/// Limits for a ZIP archive. Declared sizes are checked before reading, and
/// actual decoded bytes are checked while reading each entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ZipLimits {
    pub max_entries: usize,
    pub max_entry_bytes: u64,
    pub max_total_bytes: u64,
}

/// How entry names are treated before validation. The default is strict.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NameMode {
    /// Names containing `\` are rejected as unsafe.
    #[default]
    Strict,
    /// `\` is mapped to `/` before validation (some writers emit Windows separators).
    /// Duplicate detection and the unsafe-name checks then apply to the normalised name.
    NormalizeBackslash,
}

impl NameMode {
    pub(crate) fn apply_cow(self, name: std::borrow::Cow<'_, str>) -> std::borrow::Cow<'_, str> {
        if self == Self::NormalizeBackslash && name.contains('\\') { std::borrow::Cow::Owned(name.replace('\\', "/")) } else { name }
    }

    /// Returns the name as exposed by the guard under this mode.
    pub fn apply<'a>(self, name: &'a str) -> std::borrow::Cow<'a, str> {
        if self == Self::NormalizeBackslash && name.contains('\\') {
            std::borrow::Cow::Owned(name.replace('\\', "/"))
        } else {
            std::borrow::Cow::Borrowed(name)
        }
    }
}

/// Metadata for a validated central-directory entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ZipEntry {
    pub name: String,
    pub compressed_size: u64,
    pub size: u64,
    pub compression_method: u16,
    pub encrypted: bool,
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
    names: NameMode,
    total_read: u64,
}

impl<R: Read + Seek> ZipGuard<R> {
    /// Validates all central-directory names and declared sizes before exposing entries.
    pub fn new(archive: zip::ZipArchive<R>, limits: ZipLimits) -> Result<Self, ZipError> {
        Self::new_with_names(archive, limits, NameMode::Strict)
    }

    /// Like [`new`](Self::new) with an explicit [`NameMode`].
    pub fn new_with_names(archive: zip::ZipArchive<R>, limits: ZipLimits, names: NameMode) -> Result<Self, ZipError> {
        validate(&archive, &limits, names)?;
        Ok(Self { archive, limits, names, total_read: 0 })
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

    /// Returns the validated name of an entry in central-directory order.
    pub fn entry_name(&mut self, index: usize) -> Result<String, ZipError> {
        Ok(self.names.apply(&self.archive.by_index_data(index)?.name()?).into_owned())
    }

    /// Returns validated metadata for one entry without reading its contents.
    pub fn entry(&mut self, index: usize) -> Result<ZipEntry, ZipError> {
        self.entry_at(index)
    }

    pub(crate) fn entry_at(&self, index: usize) -> Result<ZipEntry, ZipError> {
        let entry = self.archive.by_index_data(index)?;
        #[allow(deprecated)]
        let compression_method = entry.compression().to_u16();
        Ok(ZipEntry {
            name: self.names.apply(&entry.name()?).into_owned(),
            compressed_size: entry.compressed_size(),
            size: entry.size(),
            compression_method,
            encrypted: entry.encrypted(),
        })
    }

    /// Index of the first entry whose exposed name equals `name`.
    pub(crate) fn index_of(&self, name: &str) -> Option<usize> {
        (0..self.archive.len()).find(|&index| {
            self.archive.by_index_data(index).ok().and_then(|entry| entry.name().ok().map(|raw| self.names.apply(&raw) == name)).unwrap_or(false)
        })
    }

    /// Reads an entry to completion, including its CRC check, without exceeding caps.
    pub fn read_entry(&mut self, index: usize) -> Result<Vec<u8>, ZipError> {
        self.read_entry_bounded(index, self.limits.max_entry_bytes)
    }

    /// Reads an entry using a cap no larger than this guard's archive-wide cap.
    pub fn read_entry_bounded(&mut self, index: usize, max_entry_bytes: u64) -> Result<Vec<u8>, ZipError> {
        let mut entry = self.archive.by_index(index)?;
        let declared = entry.size();
        let max_entry_bytes = max_entry_bytes.min(self.limits.max_entry_bytes);
        if entry.size() > max_entry_bytes {
            return Err(ZipError::EntryLimit);
        }
        let mut output = Vec::new();
        let mut buffer = [0; 8192];
        loop {
            let remaining_entry = max_entry_bytes - output.len() as u64;
            let remaining_total = self.limits.max_total_bytes - self.total_read;
            let chunk = buffer.len().min(usize::try_from(remaining_entry.min(remaining_total).saturating_add(1)).unwrap_or(usize::MAX));
            let count = entry.read(&mut buffer[..chunk])?;
            if count == 0 {
                // The CRC passed; the stream must also be as long as the directory says (the pre-swap
                // readers refused an entry that decoded to another length).
                if output.len() as u64 != declared {
                    return Err(ZipError::Archive(zip::result::ZipError::InvalidArchive(
                        "entry decodes to a different size than its declared size".into(),
                    )));
                }
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

/// One pass over the central directory: duplicates, count, names, symlinks and declared sizes.
/// Allocates nothing per entry in strict mode.
fn validate<R: Read + Seek>(archive: &zip::ZipArchive<R>, limits: &ZipLimits, mode: NameMode) -> Result<(), ZipError> {
    if archive.has_duplicate_names() {
        return Err(ZipError::Duplicate);
    }
    if archive.len() > limits.max_entries {
        return Err(ZipError::EntryCount);
    }
    // Strict names are unique already (`has_duplicate_names` above); normalising can create collisions.
    let mut seen = std::collections::HashSet::<String>::new();
    if mode != NameMode::Strict {
        seen.try_reserve(archive.len()).map_err(|_| ZipError::EntryCount)?;
    }
    let mut total = 0u64;
    for index in 0..archive.len() {
        let entry = archive.by_index_data(index)?;
        let raw = entry.name()?;
        let name = mode.apply_cow(raw);
        if !safe_name(&name) {
            return Err(ZipError::UnsafePath(name.into_owned()));
        }
        if mode != NameMode::Strict && !seen.insert(name.to_string()) {
            return Err(ZipError::Duplicate);
        }
        if entry.unix_mode().is_some_and(|mode| mode & 0o170000 == 0o120000) {
            return Err(ZipError::Symlink(name.into_owned()));
        }
        if entry.size() > limits.max_entry_bytes {
            return Err(ZipError::EntryLimit);
        }
        total = total.checked_add(entry.size()).ok_or(ZipError::TotalLimit)?;
        if total > limits.max_total_bytes {
            return Err(ZipError::TotalLimit);
        }
    }
    Ok(())
}

fn safe_name(name: &str) -> bool {
    let path = name.strip_suffix('/').unwrap_or(name);
    if path.is_empty() || name.starts_with('/') || name.as_bytes().get(1) == Some(&b':') {
        return false;
    }
    // One byte pass. `char::is_control` is U+0000..=U+001F and U+007F..=U+009F; the latter range is
    // the UTF-8 pair C2 80..=C2 9F, and no other multi-byte sequence contains a control character.
    let bytes = name.as_bytes();
    if bytes.iter().any(|&b| b < 0x20 || b == 0x7f || b == b'\\') || bytes.windows(2).any(|w| w[0] == 0xC2 && (0x80..=0x9F).contains(&w[1])) {
        return false;
    }
    path.split('/').all(|part| !part.is_empty() && part != "." && part != "..")
}
