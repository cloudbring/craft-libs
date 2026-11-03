//! Cost-bounded caches and deterministic fingerprints.
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

extern crate self as craft_cache;

mod fingerprint;
mod lru;
mod sweep;

pub use craft_cache_derive::Fingerprint;
pub use fingerprint::{Fingerprint, FingerprintHasher};
pub use lru::{Budget, CacheStats, CostLru, InsertError};
pub use sweep::FrameSweep;
