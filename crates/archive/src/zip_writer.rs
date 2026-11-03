//! Deterministic ZIP writing with bounded input and ZIP64 only when needed.

use std::collections::HashSet;
use std::io::{self, Cursor, Seek, Write};

use flate2::{Compression, write::DeflateEncoder};

const ZIP32_SIZE: u64 = u32::MAX as u64;
const ZIP32_COUNT: usize = u16::MAX as usize;
const DEFLATE_CHUNK: usize = 1 << 20;

fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}

fn limit(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}

/// Compression method for a new entry. Deflate levels range from 0 to 9.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Method {
    Store,
    Deflate(u32),
}

/// Raw MS-DOS time and date fields. The default is midnight, 1980-01-01.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DosTime {
    pub time: u16,
    pub date: u16,
}

impl Default for DosTime {
    fn default() -> Self {
        Self { time: 0, date: 0x21 }
    }
}

/// Caps on the number and uncompressed byte sizes of entries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ZipWriteLimits {
    pub max_entries: usize,
    pub max_entry_bytes: u64,
    pub max_total_bytes: u64,
}

impl Default for ZipWriteLimits {
    fn default() -> Self {
        Self { max_entries: 100_000, max_entry_bytes: 512 * 1024 * 1024, max_total_bytes: 2 * 1024 * 1024 * 1024 }
    }
}

/// Writes complete local headers followed by a central directory, without data descriptors.
///
/// The output must be at its intended start position when the first entry is added.
/// A failed write poisons the writer; an invalid input leaves it usable.
pub struct ZipWriter<W: Write + Seek> {
    output: W,
    central: Vec<u8>,
    names: HashSet<String>,
    limits: ZipWriteLimits,
    total_bytes: u64,
    time: DosTime,
    needs_zip64: bool,
    failed: bool,
}

impl<W: Write + Seek> ZipWriter<W> {
    pub fn new(output: W) -> Self {
        Self::with_limits(output, ZipWriteLimits::default())
    }

    pub fn with_limits(output: W, limits: ZipWriteLimits) -> Self {
        Self {
            output,
            central: Vec::new(),
            names: HashSet::new(),
            limits,
            total_bytes: 0,
            time: DosTime::default(),
            needs_zip64: false,
            failed: false,
        }
    }

    /// Sets the timestamp for subsequent entries; call before each entry to vary it.
    pub fn set_time(&mut self, time: DosTime) {
        self.time = time;
    }

    pub fn add_stored(&mut self, name: &str, bytes: &[u8]) -> io::Result<()> {
        self.add(name, bytes, Method::Store)
    }

    pub fn add_deflated(&mut self, name: &str, bytes: &[u8], level: u32) -> io::Result<()> {
        self.add(name, bytes, Method::Deflate(level))
    }

    /// Deflates an entry only when its compressed bytes are smaller than the original.
    pub fn add_deflated_if_smaller(&mut self, name: &str, bytes: &[u8], level: u32) -> io::Result<()> {
        self.add_impl(name, bytes, Method::Deflate(level), true, || false)
    }

    /// As above, but checks `stop` before each chunk of up to 1 MiB during compression.
    /// Cancellation leaves the archive unchanged and returns an interrupted I/O error.
    pub fn add_deflated_if_smaller_with_stop(&mut self, name: &str, bytes: &[u8], level: u32, stop: impl FnMut() -> bool) -> io::Result<()> {
        self.add_impl(name, bytes, Method::Deflate(level), true, stop)
    }

    /// Adds a complete entry. Deflate always uses the requested method, even if it expands.
    pub fn add(&mut self, name: &str, bytes: &[u8], method: Method) -> io::Result<()> {
        self.add_impl(name, bytes, method, false, || false)
    }

    fn add_impl(&mut self, name: &str, bytes: &[u8], method: Method, store_if_larger: bool, mut stop: impl FnMut() -> bool) -> io::Result<()> {
        if self.failed {
            return Err(io::Error::other("ZIP writer failed previously"));
        }
        if !safe_name(name) || name.len() > u16::MAX as usize {
            return Err(invalid("invalid ZIP entry name"));
        }
        if self.names.contains(name) {
            return Err(invalid("duplicate ZIP entry name"));
        }
        if self.names.len() >= self.limits.max_entries {
            return Err(limit("ZIP entry count exceeds limit"));
        }
        let size = u64::try_from(bytes.len()).map_err(|_| limit("ZIP entry exceeds limit"))?;
        if size > self.limits.max_entry_bytes {
            return Err(limit("ZIP entry exceeds limit"));
        }
        let total = self.total_bytes.checked_add(size).ok_or_else(|| limit("ZIP total exceeds limit"))?;
        if total > self.limits.max_total_bytes {
            return Err(limit("ZIP total exceeds limit"));
        }
        let (mut method_code, mut compressed): (u16, Option<Vec<u8>>) = match method {
            Method::Store => (0, None),
            Method::Deflate(level) if level <= 9 => {
                let mut encoder = DeflateEncoder::new(Vec::new(), Compression::new(level));
                for chunk in bytes.chunks(DEFLATE_CHUNK) {
                    if stop() {
                        return Err(io::Error::new(io::ErrorKind::Interrupted, "ZIP compression cancelled"));
                    }
                    encoder.write_all(chunk)?;
                }
                if bytes.is_empty() && stop() {
                    return Err(io::Error::new(io::ErrorKind::Interrupted, "ZIP compression cancelled"));
                }
                (8, Some(encoder.finish()?))
            }
            Method::Deflate(_) => return Err(invalid("invalid deflate level")),
        };
        if store_if_larger && compressed.as_ref().is_some_and(|body| body.len() >= bytes.len()) {
            method_code = 0;
            compressed = None;
        }
        let body = compressed.as_deref().unwrap_or(bytes);
        let packed = u64::try_from(body.len()).map_err(|_| limit("ZIP compressed entry exceeds limit"))?;
        let offset = self.output.stream_position()?;
        let large = size >= ZIP32_SIZE || packed >= ZIP32_SIZE;
        let version = if large { 45u16 } else { 20u16 };
        let flags = if name.is_ascii() { 0u16 } else { 1 << 11 };
        let crc = craft_hash::crc32(bytes);
        let mut local = Vec::new();
        local.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        local.extend_from_slice(&version.to_le_bytes());
        local.extend_from_slice(&flags.to_le_bytes());
        local.extend_from_slice(&method_code.to_le_bytes());
        local.extend_from_slice(&self.time.time.to_le_bytes());
        local.extend_from_slice(&self.time.date.to_le_bytes());
        local.extend_from_slice(&crc.to_le_bytes());
        local.extend_from_slice(&(if large { u32::MAX } else { packed as u32 }).to_le_bytes());
        local.extend_from_slice(&(if large { u32::MAX } else { size as u32 }).to_le_bytes());
        local.extend_from_slice(&(name.len() as u16).to_le_bytes());
        local.extend_from_slice(&(if large { 20u16 } else { 0u16 }).to_le_bytes());
        local.extend_from_slice(name.as_bytes());
        if large {
            local.extend_from_slice(&1u16.to_le_bytes());
            local.extend_from_slice(&16u16.to_le_bytes());
            local.extend_from_slice(&size.to_le_bytes());
            local.extend_from_slice(&packed.to_le_bytes());
        }

        let large_offset = offset >= ZIP32_SIZE;
        let central_extra_size = 8 * (usize::from(size >= ZIP32_SIZE) + usize::from(packed >= ZIP32_SIZE) + usize::from(large_offset));
        self.central
            .try_reserve(46 + name.len() + if central_extra_size == 0 { 0 } else { central_extra_size + 4 })
            .map_err(|_| limit("ZIP central directory exceeds memory limit"))?;
        let mut record = Vec::new();
        record.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
        record.extend_from_slice(&version.to_le_bytes());
        record.extend_from_slice(&(if large || large_offset { 45u16 } else { 20u16 }).to_le_bytes());
        record.extend_from_slice(&flags.to_le_bytes());
        record.extend_from_slice(&method_code.to_le_bytes());
        record.extend_from_slice(&self.time.time.to_le_bytes());
        record.extend_from_slice(&self.time.date.to_le_bytes());
        record.extend_from_slice(&crc.to_le_bytes());
        record.extend_from_slice(&(if packed >= ZIP32_SIZE { u32::MAX } else { packed as u32 }).to_le_bytes());
        record.extend_from_slice(&(if size >= ZIP32_SIZE { u32::MAX } else { size as u32 }).to_le_bytes());
        record.extend_from_slice(&(name.len() as u16).to_le_bytes());
        record.extend_from_slice(&(if central_extra_size == 0 { 0u16 } else { (central_extra_size + 4) as u16 }).to_le_bytes());
        record.extend_from_slice(&[0; 6]);
        record.extend_from_slice(&[0; 4]);
        record.extend_from_slice(&(if large_offset { u32::MAX } else { offset as u32 }).to_le_bytes());
        record.extend_from_slice(name.as_bytes());
        if central_extra_size != 0 {
            record.extend_from_slice(&1u16.to_le_bytes());
            record.extend_from_slice(&(central_extra_size as u16).to_le_bytes());
            if size >= ZIP32_SIZE {
                record.extend_from_slice(&size.to_le_bytes());
            }
            if packed >= ZIP32_SIZE {
                record.extend_from_slice(&packed.to_le_bytes());
            }
            if large_offset {
                record.extend_from_slice(&offset.to_le_bytes());
            }
        }
        if let Err(error) = self.output.write_all(&local).and_then(|()| self.output.write_all(body)) {
            self.failed = true;
            return Err(error);
        }
        self.central.extend_from_slice(&record);
        self.names.insert(name.to_owned());
        self.total_bytes = total;
        self.needs_zip64 |= large || large_offset;
        Ok(())
    }

    /// Completes the archive and returns the underlying writer.
    pub fn finish(mut self) -> io::Result<W> {
        if self.failed {
            return Err(io::Error::other("ZIP writer failed previously"));
        }
        let start = self.output.stream_position()?;
        let central_size = u64::try_from(self.central.len()).map_err(|_| limit("ZIP central directory exceeds limit"))?;
        let count = self.names.len() as u64;
        let zip64 = self.needs_zip64 || self.names.len() >= ZIP32_COUNT || start >= ZIP32_SIZE || central_size >= ZIP32_SIZE;
        self.output.write_all(&self.central)?;
        if zip64 {
            let end = start.checked_add(central_size).ok_or_else(|| limit("ZIP offset overflow"))?;
            self.output.write_all(&0x0606_4b50u32.to_le_bytes())?;
            self.output.write_all(&44u64.to_le_bytes())?;
            self.output.write_all(&45u16.to_le_bytes())?;
            self.output.write_all(&45u16.to_le_bytes())?;
            self.output.write_all(&[0; 8])?;
            self.output.write_all(&count.to_le_bytes())?;
            self.output.write_all(&count.to_le_bytes())?;
            self.output.write_all(&central_size.to_le_bytes())?;
            self.output.write_all(&start.to_le_bytes())?;
            self.output.write_all(&0x0706_4b50u32.to_le_bytes())?;
            self.output.write_all(&0u32.to_le_bytes())?;
            self.output.write_all(&end.to_le_bytes())?;
            self.output.write_all(&1u32.to_le_bytes())?;
        }
        self.output.write_all(&0x0605_4b50u32.to_le_bytes())?;
        self.output.write_all(&[0; 4])?;
        let classic_count = if self.names.len() >= ZIP32_COUNT { u16::MAX } else { self.names.len() as u16 };
        self.output.write_all(&classic_count.to_le_bytes())?;
        self.output.write_all(&classic_count.to_le_bytes())?;
        self.output.write_all(&(if central_size >= ZIP32_SIZE { u32::MAX } else { central_size as u32 }).to_le_bytes())?;
        self.output.write_all(&(if start >= ZIP32_SIZE { u32::MAX } else { start as u32 }).to_le_bytes())?;
        self.output.write_all(&0u16.to_le_bytes())?;
        Ok(self.output)
    }
}

impl ZipWriter<Cursor<Vec<u8>>> {
    /// Creates an in-memory archive writer; `finish()?.into_inner()` returns its bytes.
    pub fn in_memory() -> Self {
        Self::new(Cursor::new(Vec::new()))
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
