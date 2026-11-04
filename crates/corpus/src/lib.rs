#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![forbid(unsafe_code)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

//! Pinned test corpora shared by the Craft apps.
//!
//! A corpus is a TOML [`Manifest`]: HTTPS URL files and GitHub repo@commit sources, each file with
//! its sha256, licence, provenance and optional feature tags.
//!
//! - [`fetch`] downloads over HTTPS only (an `http://` URL or a redirect off HTTPS is refused),
//!   checks every sha256, writes atomically and finishes with a `.craft-corpus.json` stamp.
//!   [`fetch_with`] takes any [`Fetcher`], which is how unit tests avoid the network.
//! - [`verify`] / [`verify_manifest`] refuse a tree with no stamp, an edited, missing or extra file,
//!   or a stamp from another manifest version.
//! - [`corpus_or_skip!`] in a test prints `SKIP ...` when the corpus is absent, unless
//!   `CRAFT_CORPUS_REQUIRE=1` or `<NAME>_CORPUS_REQUIRED=1` makes absence a failure (CI).
//! - [`Floors`] is the `corpus-floors.toml` ratchet: counts may rise, never fall.
//! - [`Manifest::cache_key`] keys CI caches; [`Corpus::files_by_tag`] iterates by feature tag.

pub mod cli;
mod error;
mod fetch;
mod floors;
mod glob;
mod manifest;
mod require;

pub use error::{Error, Result};
pub use fetch::{
    Corpus, CorpusFile, FetchReport, Fetcher, Stamp, StampFile, UreqFetcher, fetch, fetch_with, resolve_redirect, verify, verify_manifest,
};
pub use floors::{Counts, Floors, Raised, Violation};
pub use glob::glob_match;
pub use manifest::{Entry, GitFile, GitSource, Manifest, STAMP_FILE, UrlFile, safe_rel};
pub use require::{GLOBAL_VAR, is_required, load_or_skip, load_or_skip_with, per_corpus_var, required_by, required_by_with};

/// Convenience: SHA-256 hex of a file, for pinning a new entry.
///
/// # Errors
/// I/O errors.
pub fn sha256_file(path: &std::path::Path) -> Result<String> {
    craft_hash::sha256_file(path).map_err(|e| Error::io(format!("read {}", path.display()), e))
}
