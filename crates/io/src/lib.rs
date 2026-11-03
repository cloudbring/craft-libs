#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
//! Bounded reads of files and other readers.
//!
//! Path reads open once, nonblocking on Unix, and inspect the opened handle before reading.
//! The cap is enforced while reading, even if a regular file grows after it is opened.

use std::fs::{File, OpenOptions};
use std::io::{self, Read};
use std::path::Path;

/// Failure to open, validate, or read a capped file or reader.
#[derive(Debug, thiserror::Error)]
pub enum CappedReadError {
    /// The input contains more than `limit` bytes.
    #[error("input is larger than the {limit}-byte limit")]
    TooLarge { limit: usize },
    /// The opened handle does not refer to a regular file.
    #[error("not a regular file")]
    NotRegular,
    /// Opening, reading, or decoding the input failed.
    #[error(transparent)]
    Io(#[from] io::Error),
}

#[derive(Debug)]
struct LimitExceeded;

impl std::fmt::Display for LimitExceeded {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("reader exceeded its byte limit")
    }
}

impl std::error::Error for LimitExceeded {}

/// A [`Read`] adapter that permits at most `limit` bytes and probes one extra byte.
///
/// Reading past the limit returns an I/O error, rather than silently truncating.
/// [`CappedReader::into_bytes`] converts this error to [`CappedReadError::TooLarge`].
/// This adapter does not inspect the reader's file type; use [`read_capped_from`] for files.
pub struct CappedReader<R> {
    inner: R,
    limit: usize,
    remaining: usize,
    finished: bool,
    overflowed: bool,
}

impl<R: Read> CappedReader<R> {
    /// Wrap a reader with an inclusive byte limit.
    pub fn new(inner: R, limit: usize) -> Self {
        Self { inner, limit, remaining: limit, finished: false, overflowed: false }
    }

    /// Read the entire input, returning a structured error if it exceeds the limit.
    pub fn into_bytes(mut self) -> Result<Vec<u8>, CappedReadError> {
        let mut bytes = Vec::new();
        match Read::read_to_end(&mut self, &mut bytes) {
            Ok(_) => Ok(bytes),
            Err(_) if self.overflowed => Err(CappedReadError::TooLarge { limit: self.limit }),
            Err(error) => Err(CappedReadError::Io(error)),
        }
    }
}

impl<R: Read> Read for CappedReader<R> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if buffer.is_empty() || self.finished {
            return Ok(0);
        }
        if self.overflowed {
            return Err(io::Error::other(LimitExceeded));
        }
        if self.remaining == 0 {
            let mut extra = [0];
            return match self.inner.read(&mut extra)? {
                0 => {
                    self.finished = true;
                    Ok(0)
                }
                _ => {
                    self.overflowed = true;
                    Err(io::Error::other(LimitExceeded))
                }
            };
        }
        let allowed = buffer.len().min(self.remaining);
        let count = self.inner.read(&mut buffer[..allowed])?;
        self.remaining -= count;
        if count == 0 {
            self.finished = true;
        }
        Ok(count)
    }
}

/// Read a path as a regular file, returning at most `max` bytes.
///
/// On Unix the open is nonblocking, so a path naming a FIFO without a writer does not
/// hang before its type can be checked. Symlinks to regular files are accepted.
/// On Windows the opened handle is checked but opening a named pipe may block.
pub fn read_capped(path: &Path, max: usize) -> Result<Vec<u8>, CappedReadError> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NONBLOCK);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(windows_sys::Win32::Storage::FileSystem::FILE_FLAG_BACKUP_SEMANTICS);
    }
    read_capped_from(options.open(path)?, max)
}

/// Read from an already-open file handle, checking that it is a regular file.
///
/// Callers are responsible for opening the handle without blocking on special files.
pub fn read_capped_from(file: File, max: usize) -> Result<Vec<u8>, CappedReadError> {
    if !file.metadata()?.is_file() {
        return Err(CappedReadError::NotRegular);
    }
    CappedReader::new(file, max).into_bytes()
}

/// Read a regular file as UTF-8, rejecting oversized or invalid input.
pub fn read_capped_to_string(path: &Path, max: usize) -> Result<String, CappedReadError> {
    String::from_utf8(read_capped(path, max)?).map_err(|error| CappedReadError::Io(io::Error::new(io::ErrorKind::InvalidData, error)))
}
