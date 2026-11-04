// Vendored verbatim from https://github.com/cloudbring/deckcraft branch algo/123-use-craft-cfb at 0bff70074fcd91e154235cd7792e16f80b987e9e
// path: crates/pptx/src/cfb.rs
// Only change: the trailing '#[cfg(test)]' items are dropped (they are the app's own unit tests and need its dev-dependencies); nothing else is edited.

//! Detect password-protected OOXML presentations using the shared CFB reader.

use craft_cfb::{Cfb, EntryKind};

const SIGNATURE: [u8; 8] = [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];
const ENCRYPTED_STREAMS: [&str; 2] = ["EncryptedPackage", "EncryptionInfo"];

/// Require both encryption streams in a valid CFB. Preserve the UTF-16LE marker scan
/// for malformed containers only, so damaged password-protected files remain recognizable.
pub(crate) fn is_encrypted(bytes: &[u8]) -> bool {
    match Cfb::open(bytes) {
        Ok(cfb) => ENCRYPTED_STREAMS
            .iter()
            .all(|name| cfb.entries().any(|entry| entry.kind() == EntryKind::Stream && entry.path().eq_ignore_ascii_case(name))),
        Err(_) => bytes.starts_with(&SIGNATURE) && ENCRYPTED_STREAMS.iter().any(|name| contains_utf16(bytes, name)),
    }
}

fn contains_utf16(hay: &[u8], needle: &str) -> bool {
    let encoded: Vec<u8> = needle.encode_utf16().flat_map(u16::to_le_bytes).collect();
    !encoded.is_empty() && hay.windows(encoded.len()).any(|window| window == encoded.as_slice())
}

