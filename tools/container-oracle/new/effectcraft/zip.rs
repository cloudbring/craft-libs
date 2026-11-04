// Vendored verbatim from https://github.com/cloudbring/effectcraft branch algo/120-use-craft-archive-zip at e38f5598e624db5d4090f71a807c30f3251038f0
// path: crates/lottie/src/zip.rs
// Only change: the trailing '#[cfg(test)]' items are dropped (they are the app's own unit tests and need its dev-dependencies); nothing else is edited.

//! ZIP helpers for `.lottie` (dotLottie) archives.

use craft_archive::{ZipLimits, ZipReader, ZipWriter};

const MAX_ENTRIES: usize = 1_024;
const MAX_ENTRY_BYTES: u64 = 64 * 1024 * 1024;
const MAX_TOTAL_BYTES: u64 = 256 * 1024 * 1024;

fn limits() -> ZipLimits {
    ZipLimits { max_entries: MAX_ENTRIES, max_entry_bytes: MAX_ENTRY_BYTES, max_total_bytes: MAX_TOTAL_BYTES }
}

/// Build a deterministic stored ZIP archive with the given (name, bytes) entries.
pub fn store(entries: &[(String, Vec<u8>)]) -> Vec<u8> {
    let mut writer = ZipWriter::in_memory();
    for (name, bytes) in entries {
        if writer.add_stored(name, bytes).is_err() {
            return Vec::new();
        }
    }
    match writer.finish() {
        Ok(output) => output.into_inner(),
        Err(_) => Vec::new(),
    }
}

/// Read bounded, relative-path entries from a dotLottie ZIP archive.
///
/// dotLottie permits DEFLATE compression, so [`ZipReader`] accepts both stored and deflated
/// entries after validating names, declared sizes, decoded sizes, CRCs, and total output.
pub fn read_stored(data: &[u8]) -> Vec<(String, Vec<u8>)> {
    let Ok(mut reader) = ZipReader::open(data, limits()) else {
        return Vec::new();
    };
    let names = reader.names().to_vec();
    let mut entries = Vec::new();
    for name in names {
        match reader.read(&name) {
            Ok(Some(bytes)) => entries.push((name, bytes)),
            Ok(None) | Err(_) => return Vec::new(),
        }
    }
    entries
}

