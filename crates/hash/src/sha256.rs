//! SHA-256 (FIPS 180-4) through the workspace [`sha2`] crate, the same implementation
//! `craft-update`, `craft-font-store` and `craft-keymap` already hash with.
//!
//! [`sha256_hex`] is the one-shot lowercase hex form those crates return. [`Sha256`] is the
//! streaming form: feed it with [`Sha256::update`] or [`std::io::Write`] (so `std::io::copy` into
//! it works, which is how the launcher and `craft-update` digest a file). [`sha256_file`] reads a
//! path to the end in 1 MiB chunks and returns that hex. It does not report progress and it does
//! not cancel. A caller that must, the model verifier in craft-ai-integrations for example, keeps
//! its own read loop, checks its flag between chunks, and calls [`Sha256::update`]. Stopping early
//! leaves the hash of the bytes fed so far, which is not the hash of the whole file.

use sha2::Digest;
use std::io::Read;
use std::path::Path;

/// Bytes per read in [`sha256_reader`] and [`sha256_file`].
const READ_CHUNK: usize = 1024 * 1024;

/// Streaming SHA-256.
///
/// [`Sha256::finish`] clones the state first. The `sha2` finalizer consumes a hasher; cloning
/// keeps this one usable, so a later [`update`](Self::update) still covers the bytes already fed.
#[derive(Clone)]
#[must_use = "call update and finish, or the digest is discarded"]
pub struct Sha256 {
    inner: sha2::Sha256,
}

impl Sha256 {
    /// Empty hasher. The digest of no bytes is the FIPS empty hash.
    #[inline]
    pub fn new() -> Self {
        Self { inner: sha2::Sha256::new() }
    }

    /// Fold `data` in.
    #[inline]
    pub fn update(&mut self, data: &[u8]) {
        self.inner.update(data);
    }

    /// 32-byte digest. Does not change this hasher.
    #[inline]
    #[must_use]
    pub fn finish(&self) -> [u8; 32] {
        to_bytes(self.inner.clone().finalize())
    }

    /// Lowercase hex of [`finish`](Self::finish).
    #[inline]
    #[must_use]
    pub fn hex(&self) -> String {
        crate::hex::encode(&self.finish())
    }

    /// Digest of `data` in one call.
    #[inline]
    #[must_use]
    pub fn digest(data: &[u8]) -> [u8; 32] {
        to_bytes(sha2::Sha256::digest(data))
    }

    /// Back to [`new`](Self::new).
    #[inline]
    pub fn reset(&mut self) {
        *self = Self::new();
    }
}

impl Default for Sha256 {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for Sha256 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Sha256({})", self.hex())
    }
}

fn to_bytes(digest: sha2::digest::Output<sha2::Sha256>) -> [u8; 32] {
    let slice = digest.as_slice();
    let mut out = [0u8; 32];
    if slice.len() == out.len() {
        out.copy_from_slice(slice);
    }
    out
}

/// SHA-256 of `data`, 32 bytes.
#[inline]
#[must_use]
pub fn sha256(data: &[u8]) -> [u8; 32] {
    Sha256::digest(data)
}

/// Lowercase hex SHA-256 of `data`, 64 digits.
///
/// `sha256_hex(b"abc")` is `ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad`.
#[inline]
#[must_use]
pub fn sha256_hex(data: &[u8]) -> String {
    crate::hex::encode(&sha256(data))
}

/// Lowercase hex SHA-256 of everything `reader` returns.
///
/// A short read is hashed as itself. Cancellation is not visible here: close the reader, or stop
/// calling [`Sha256::update`] from your own loop, to stop early. The hash is then the hash of the
/// prefix that was read.
pub fn sha256_reader(mut reader: impl Read) -> std::io::Result<String> {
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; READ_CHUNK];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        let n = n.min(buf.len());
        hasher.update(buf.get(..n).unwrap_or(&[]));
    }
    Ok(hasher.hex())
}

/// Lowercase hex SHA-256 of the file at `path`, read to the end in 1 MiB chunks.
///
/// The error is the I/O error from opening or reading. A missing file, a short file and a file
/// that does not match a published digest are the caller's to tell apart: compare the hex.
pub fn sha256_file(path: &Path) -> std::io::Result<String> {
    sha256_reader(std::fs::File::open(path)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ABC: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
    const EMPTY: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

    #[test]
    fn fips_vectors() {
        assert_eq!(sha256_hex(b"abc"), ABC);
        assert_eq!(sha256_hex(b""), EMPTY);
        assert_eq!(hex_of(sha256(b"abc")), ABC);
    }

    fn hex_of(bytes: [u8; 32]) -> String {
        crate::hex::encode(&bytes)
    }

    #[test]
    fn streaming_write_and_prefix() {
        let mut hasher = Sha256::new();
        hasher.update(b"a");
        hasher.update(b"b");
        hasher.update(b"c");
        assert_eq!(hasher.hex(), ABC);
        assert_eq!(hasher.finish(), sha256(b"abc"));

        let mut via_copy = Sha256::new();
        std::io::copy(&mut &b"abc"[..], &mut via_copy).unwrap();
        assert_eq!(via_copy.hex(), ABC);

        let mut prefix = Sha256::new();
        prefix.update(b"ab");
        assert_eq!(prefix.hex(), sha256_hex(b"ab"));
        assert_ne!(prefix.hex(), ABC);
        prefix.reset();
        assert_eq!(prefix.hex(), EMPTY);
    }

    #[test]
    fn reader_chunks_and_file() {
        assert_eq!(sha256_reader(OneByte { data: b"abc", at: 0 }).unwrap(), ABC);
        assert_eq!(sha256_reader(std::io::Cursor::new(b"")).unwrap(), EMPTY);

        let path = std::env::temp_dir().join(format!("craft-hash-sha256-{}", std::process::id()));
        let _remove = Remove(&path);
        std::fs::write(&path, b"abc").unwrap();
        assert_eq!(sha256_file(&path).unwrap(), ABC);
        std::fs::write(&path, b"").unwrap();
        assert_eq!(sha256_file(&path).unwrap(), EMPTY);
    }

    struct OneByte<'a> {
        data: &'a [u8],
        at: usize,
    }

    impl Read for OneByte<'_> {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            if self.at >= self.data.len() || buf.is_empty() {
                return Ok(0);
            }
            buf[0] = self.data[self.at];
            self.at += 1;
            Ok(1)
        }
    }

    struct Remove<'a>(&'a std::path::Path);
    impl Drop for Remove<'_> {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(self.0);
        }
    }
}
