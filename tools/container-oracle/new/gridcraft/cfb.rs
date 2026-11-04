// Vendored verbatim from https://github.com/cloudbring/gridcraft branch algo/123-use-craft-cfb at f0048bf34430f01ca049eeb359f2a0e8a1b3c14d
// path: crates/xlsx/src/cfb.rs
// Only change: the trailing '#[cfg(test)]' items are dropped (they are the app's own unit tests and need its dev-dependencies); nothing else is edited.

//! Just enough of the Compound File Binary (CFB/OLE2) container to tell what a non-zip
//! spreadsheet file is: Excel wraps password-protected workbooks in a CFB holding an
//! `EncryptedPackage` stream, while Excel 97-2003 `.xls` (and other legacy Office files) are
//! CFB files without one. Parsing the container itself lives in the shared `craft-cfb` crate
//! (one reader across the ArtCraft apps); the streams are read through `craft_cfb::Cfb` at the
//! call sites, and every read there is bounds-checked with bounded loops.

use craft_cfb::Cfb;

/// The CFB signature at the start of the file.
pub(crate) const SIGNATURE: [u8; 8] = [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];

/// Stream names that mark an encrypted (password-protected) OOXML package.
const ENCRYPTED_STREAMS: [&str; 2] = ["EncryptedPackage", "EncryptionInfo"];

/// Whether the compound file holds an encryption stream. Reads the directory through the shared
/// `craft-cfb` reader; when the structure can't be followed (truncated or malformed), falls
/// back to scanning the bytes for the UTF-16LE stream names, so sniffing of damaged-but-
/// encrypted files stays as tolerant as before.
pub(crate) fn is_encrypted(bytes: &[u8]) -> bool {
    match Cfb::open(bytes) {
        Ok(cf) => cf.entries().any(|e| ENCRYPTED_STREAMS.iter().any(|s| e.path().rsplit('/').next().is_some_and(|n| n.eq_ignore_ascii_case(s)))),
        Err(_) => ENCRYPTED_STREAMS.iter().any(|e| contains_utf16(bytes, e)),
    }
}

fn contains_utf16(hay: &[u8], needle: &str) -> bool {
    let n: Vec<u8> = needle.encode_utf16().flat_map(u16::to_le_bytes).collect();
    !n.is_empty() && hay.windows(n.len()).any(|w| w == n.as_slice())
}

