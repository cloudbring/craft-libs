# craft-hash

Checksums and content hashes for more than one Craft app. The crate takes bytes and returns a
number or lowercase hex. It does not open a file except [`sha256_file`](src/sha256.rs), and it
does not decide what a mismatch means. No `unsafe`.

```toml
craft-hash = { git = "https://github.com/storytold/craft-libs", rev = "<commit>" }
```

## Algorithms

| Function | Algorithm | Check |
|---|---|---|
| `crc32` | CRC-32/ISO-HDLC. Reflected polynomial `0xEDB88320`, init and xorout `0xFFFFFFFF`. PNG, ZIP, gzip, Matroska. Slicing-by-8, const tables. | `crc32(b"123456789") == 0xCBF43926` |
| `crc32_mpeg2` | CRC-32/MPEG-2. Polynomial `0x04C11DB7`, init `0xFFFFFFFF`, not reflected, xorout `0`. ISO/IEC 13818-1 Annex A (PSI). Const 256-entry table. | `crc32_mpeg2(b"123456789") == 0x0376E6E7` |
| `adler32` | Adler-32, RFC 1950. Modulo 65521, `s1` starts at 1. The modulus is deferred 5552 bytes (zlib's `NMAX`) and applied at the end of each `update`. | `adler32(b"Wikipedia") == 0x11E60398` |
| `fnv1a32`, `fnv1a64` | FNV-1a. Xor the byte, then multiply by the FNV prime. Offset basis is the hash of no bytes. | `fnv1a32(b"foobar") == 0xbf9cf968`, `fnv1a64(b"foobar") == 0x85944171f73967e8` |
| `content_hash` | The stable 128-bit content hash: FNV-1a 128, offset `0x6c62272e07bb014262b821756295c58d`, prime `2^88 + 2^8 + 0x3b`. Frozen. Hex is 32 lowercase digits, big-endian, leading zeros kept. Not a cryptographic hash. | `content_hash(b"a") == 0xd228cb696f1a8caf78912b704e4a8964` |
| `sha256_hex` | SHA-256, FIPS 180-4, via the workspace `sha2` crate (RustCrypto), the same one `craft-update`, `craft-font-store` and `craft-keymap` use. | `sha256_hex(b"abc")` is `ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad` |

Parameters for FNV are the ones published by Fowler, Noll and Vo
(<http://www.isthe.com/chongo/tech/comp/fnv/>). The 32- and 64-bit vectors are the ones in the
IETF FNV draft (`""`, `"a"`, `"foobar"`). The 128-bit vectors for `""` and `"a"` are the ones the
Zig standard library pins.

## Streaming

`Crc32`, `Crc32Mpeg2`, `Adler32`, `Fnv32`, `Fnv64`, `ContentHash` and `Sha256` each have `new`,
`update`, `finish`, `reset` and `digest`. `finish` returns the current value and does not reset
the hasher, so a later `update` still covers the bytes already fed. `Fnv32::from_state`,
`Fnv64::from_state` and `ContentHash::from_state` continue from a register `finish` returned,
which is how a caller that stored the integer (rather than the hasher) keeps hashing.

Each hasher implements `std::io::Write`. `std::io::copy` into a `Sha256` is the file digest the
launcher and `craft-update` do today.

`sha256_reader` and `sha256_file` read to the end in 1 MiB chunks and return lowercase hex. They
do not report progress and they do not cancel. A caller that must (the model verifier that hashes
a download while reporting `Verifying`, or an install that hashes while it writes) keeps its own
loop, checks its flag between chunks, and calls `Sha256::update`. The hash of a prefix is not the
hash of the whole file; compare only when the read finished.

CRC-32, Adler-32, FNV and the content hash are checksums and cache keys. SHA-256 is the hash to
store for a download.

```rust
use craft_hash::{Crc32, crc32, sha256_hex};

let mut crc = Crc32::new();
crc.update(b"12345");
crc.update(b"6789");
assert_eq!(crc.finish(), crc32(b"123456789"));
assert_eq!(
    sha256_hex(b"abc"),
    "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
);
```
