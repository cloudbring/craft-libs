#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![forbid(unsafe_code)]

//! Checksums and content hashes shared by the Craft apps.
//!
//! CRC-32, Adler-32 and FNV-1a are for file formats and cache keys. The 128-bit content hash is
//! FNV-1a 128 and is stable: the same bytes always produce the same [`content_hash`]. It is not a
//! cryptographic hash. SHA-256 is, and [`sha256_hex`] is the lowercase hex form the suite already
//! stores for downloads and font files.
//!
//! Every algorithm has a streaming hasher (`update`, `finish`, `reset`) and a one-shot function.
//! `finish` reports the current value and leaves the hasher ready for more bytes. Each hasher
//! implements [`std::io::Write`], so `std::io::copy` into it works.
//!
//! ```
//! use craft_hash::{adler32, content_hash, crc32, crc32_mpeg2, fnv1a32, sha256_hex, Crc32};
//!
//! assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
//! assert_eq!(crc32_mpeg2(b"123456789"), 0x0376_E6E7);
//! assert_eq!(adler32(b"Wikipedia"), 0x11E6_0398);
//! assert_eq!(fnv1a32(b"foobar"), 0xBF9C_F968);
//! assert_eq!(content_hash(b""), 0x6C62_272E_07BB_0142_62B8_2175_6295_C58D);
//! assert_eq!(
//!     sha256_hex(b"abc"),
//!     "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
//! );
//!
//! let mut crc = Crc32::new();
//! crc.update(b"12345");
//! crc.update(b"6789");
//! assert_eq!(crc.finish(), crc32(b"123456789"));
//! ```

mod adler32;
mod crc32;
mod fnv;
mod hex;
mod sha256;

pub use adler32::{Adler32, adler32};
pub use crc32::{Crc32, Crc32Mpeg2, crc32, crc32_mpeg2};
pub use fnv::{ContentHash, Fnv32, Fnv64, content_hash, content_hash_hex, fnv1a32, fnv1a64};
pub use sha256::{Sha256, sha256, sha256_file, sha256_hex, sha256_reader};

macro_rules! impl_io_write {
    ($($ty:ty),+ $(,)?) => {$(
        impl std::io::Write for $ty {
            fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
                self.update(buf);
                Ok(buf.len())
            }

            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
    )+};
}

impl_io_write!(Crc32, Crc32Mpeg2, Adler32, Fnv32, Fnv64, ContentHash, Sha256);
