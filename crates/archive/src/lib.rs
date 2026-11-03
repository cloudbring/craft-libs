#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

//! Bounded DEFLATE readers, guarded ZIP entries and validated PNG chunks.

mod inflate;
mod png;
mod zip_guard;

pub use inflate::{Accounting, BoundedInflate, InflateError, Limits, Wrapper, inflate_bounded};
pub use png::{PngChunk, PngChunks, PngError};
pub use zip_guard::{ZipError, ZipGuard, ZipLimits};

#[cfg(test)]
mod tests;
