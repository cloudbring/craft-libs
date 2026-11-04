#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

//! Bounded DEFLATE readers, guarded ZIP entries and writing, and validated PNG chunks.

mod inflate;
mod png;
mod zip_guard;
mod zip_reader;
mod zip_writer;

pub use inflate::{Accounting, BoundedInflate, InflateError, Limits, Wrapper, inflate_bounded};
pub use png::{PngChunk, PngChunks, PngError};
pub use zip_guard::{NameMode, ZipEntry, ZipError, ZipGuard, ZipLimits};
pub use zip_reader::ZipReader;
pub use zip_writer::{DosTime, Method, ZipWriteLimits, ZipWriter};

#[cfg(test)]
mod tests;
#[cfg(test)]
mod zip_writer_tests;
