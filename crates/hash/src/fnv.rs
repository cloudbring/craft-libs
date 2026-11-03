//! FNV-1a (Fowler–Noll–Vo), public domain. <http://www.isthe.com/chongo/tech/comp/fnv/>
//!
//! Each byte is xored into the register, then the register is multiplied by the FNV prime.
//! FNV-1 multiplies first; this is the `1a` order the apps already use for image keys. There is
//! no final mix. The hash of no bytes is the offset basis.
//!
//! The 128-bit hash is the stable content hash ([`ContentHash`]). It is not a cryptographic hash
//! and it is not a MAC. Identical bytes produce the same `u128` on every platform and every
//! version of this crate; changing the parameters would be a breaking change. Hex is 32 lowercase
//! digits of the integer, big-endian, with leading zeros kept (`u128::to_be_bytes`).

const FNV32_OFFSET: u32 = 0x811C_9DC5;
const FNV32_PRIME: u32 = 0x0100_0193;
const FNV64_OFFSET: u64 = 0xCBF2_9CE4_8422_2325;
const FNV64_PRIME: u64 = 0x0000_0100_0000_01B3;
/// `2^88 + 2^8 + 0x3b`.
const FNV128_OFFSET: u128 = 0x6C62_272E_07BB_0142_62B8_2175_6295_C58D;
const FNV128_PRIME: u128 = 0x0000_0000_0100_0000_0000_0000_0000_013B;

/// FNV-1a 32-bit.
#[derive(Clone, Copy, PartialEq, Eq)]
#[must_use = "call update and finish, or the hash is discarded"]
pub struct Fnv32 {
    state: u32,
}

impl Fnv32 {
    /// Offset basis. The hash of no bytes.
    pub const OFFSET_BASIS: u32 = FNV32_OFFSET;
    /// FNV prime, `2^24 + 2^8 + 0x93`.
    pub const PRIME: u32 = FNV32_PRIME;

    /// Start from the offset basis.
    #[inline]
    pub const fn new() -> Self {
        Self { state: FNV32_OFFSET }
    }

    /// Continue from a register a previous [`finish`](Self::finish) returned.
    ///
    /// `new` is `from_state(OFFSET_BASIS)`. Passing any other value is a continuation, not the
    /// hash of the next buffer alone.
    #[inline]
    pub const fn from_state(state: u32) -> Self {
        Self { state }
    }

    /// Fold `data` in.
    #[inline]
    pub const fn update(&mut self, data: &[u8]) {
        let mut i = 0;
        while i < data.len() {
            self.state = (self.state ^ data[i] as u32).wrapping_mul(FNV32_PRIME);
            i += 1;
        }
    }

    /// The register. Does not change it.
    #[inline]
    #[must_use]
    pub const fn finish(&self) -> u32 {
        self.state
    }

    /// Hash of `data` in one call.
    #[inline]
    #[must_use]
    pub const fn digest(data: &[u8]) -> u32 {
        let mut hash = Self::new();
        hash.update(data);
        hash.finish()
    }

    /// Back to [`new`](Self::new).
    #[inline]
    pub const fn reset(&mut self) {
        *self = Self::new();
    }
}

impl Default for Fnv32 {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for Fnv32 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Fnv32({:#010x})", self.finish())
    }
}

/// FNV-1a 32-bit of `data`. `fnv1a32(b"") == 0x811c9dc5`, `fnv1a32(b"foobar") == 0xbf9cf968`.
#[inline]
#[must_use]
pub const fn fnv1a32(data: &[u8]) -> u32 {
    Fnv32::digest(data)
}

/// FNV-1a 64-bit. VectorCraft's `fnv1a` and the other 64-bit copies use these parameters.
#[derive(Clone, Copy, PartialEq, Eq)]
#[must_use = "call update and finish, or the hash is discarded"]
pub struct Fnv64 {
    state: u64,
}

impl Fnv64 {
    /// Offset basis. The hash of no bytes.
    pub const OFFSET_BASIS: u64 = FNV64_OFFSET;
    /// FNV prime, `2^40 + 2^8 + 0xb3`.
    pub const PRIME: u64 = FNV64_PRIME;

    /// Start from the offset basis.
    #[inline]
    pub const fn new() -> Self {
        Self { state: FNV64_OFFSET }
    }

    /// Continue from a register a previous [`finish`](Self::finish) returned.
    #[inline]
    pub const fn from_state(state: u64) -> Self {
        Self { state }
    }

    /// Fold `data` in.
    #[inline]
    pub const fn update(&mut self, data: &[u8]) {
        let mut i = 0;
        while i < data.len() {
            self.state = (self.state ^ data[i] as u64).wrapping_mul(FNV64_PRIME);
            i += 1;
        }
    }

    /// The register. Does not change it.
    #[inline]
    #[must_use]
    pub const fn finish(&self) -> u64 {
        self.state
    }

    /// Hash of `data` in one call.
    #[inline]
    #[must_use]
    pub const fn digest(data: &[u8]) -> u64 {
        let mut hash = Self::new();
        hash.update(data);
        hash.finish()
    }

    /// Back to [`new`](Self::new).
    #[inline]
    pub const fn reset(&mut self) {
        *self = Self::new();
    }
}

impl Default for Fnv64 {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for Fnv64 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Fnv64({:#018x})", self.finish())
    }
}

/// FNV-1a 64-bit of `data`. `fnv1a64(b"foobar") == 0x85944171f73967e8`.
#[inline]
#[must_use]
pub const fn fnv1a64(data: &[u8]) -> u64 {
    Fnv64::digest(data)
}

/// Stable 128-bit content hash. This is FNV-1a 128 with the parameters above, and that choice is
/// frozen: cache keys and file identities stored as this hash keep their meaning across versions.
///
/// Not a cryptographic hash. Do not use it for signatures, download authentication or passwords;
/// use [`crate::sha256_hex`] for those.
#[derive(Clone, Copy, PartialEq, Eq)]
#[must_use = "call update and finish, or the hash is discarded"]
pub struct ContentHash {
    state: u128,
}

impl ContentHash {
    /// FNV-1a 128-bit offset basis. The hash of no bytes.
    pub const OFFSET_BASIS: u128 = FNV128_OFFSET;
    /// FNV-1a 128-bit prime, `2^88 + 2^8 + 0x3b`.
    pub const PRIME: u128 = FNV128_PRIME;

    /// Start from the offset basis.
    #[inline]
    pub const fn new() -> Self {
        Self { state: FNV128_OFFSET }
    }

    /// Continue from a register a previous [`finish`](Self::finish) returned.
    #[inline]
    pub const fn from_state(state: u128) -> Self {
        Self { state }
    }

    /// Fold `data` in.
    #[inline]
    pub const fn update(&mut self, data: &[u8]) {
        let mut i = 0;
        while i < data.len() {
            self.state = (self.state ^ data[i] as u128).wrapping_mul(FNV128_PRIME);
            i += 1;
        }
    }

    /// The register. Does not change it.
    #[inline]
    #[must_use]
    pub const fn finish(&self) -> u128 {
        self.state
    }

    /// 32 lowercase hex digits, big-endian, leading zeros kept.
    #[inline]
    #[must_use]
    pub fn hex(&self) -> String {
        crate::hex::encode(&self.finish().to_be_bytes())
    }

    /// Hash of `data` in one call.
    #[inline]
    #[must_use]
    pub const fn digest(data: &[u8]) -> u128 {
        let mut hash = Self::new();
        hash.update(data);
        hash.finish()
    }

    /// Back to [`new`](Self::new).
    #[inline]
    pub const fn reset(&mut self) {
        *self = Self::new();
    }
}

impl Default for ContentHash {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for ContentHash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ContentHash({})", self.hex())
    }
}

/// 128-bit content hash of `data` (FNV-1a 128).
#[inline]
#[must_use]
pub const fn content_hash(data: &[u8]) -> u128 {
    ContentHash::digest(data)
}

/// [`content_hash`] as 32 lowercase hex digits of the big-endian register, leading zeros kept.
#[inline]
#[must_use]
pub fn content_hash_hex(data: &[u8]) -> String {
    crate::hex::encode(&ContentHash::digest(data).to_be_bytes())
}

const _: [(); (fnv1a32(b"") == 0x811C_9DC5) as usize] = [()];
const _: [(); (fnv1a32(b"a") == 0xE40C_292C) as usize] = [()];
const _: [(); (fnv1a32(b"foobar") == 0xBF9C_F968) as usize] = [()];
const _: [(); (fnv1a64(b"") == 0xCBF2_9CE4_8422_2325) as usize] = [()];
const _: [(); (fnv1a64(b"a") == 0xAF63_DC4C_8601_EC8C) as usize] = [()];
const _: [(); (fnv1a64(b"foobar") == 0x8594_4171_F739_67E8) as usize] = [()];
const _: [(); (content_hash(b"") == FNV128_OFFSET) as usize] = [()];
const _: [(); (content_hash(b"a") == 0xD228_CB69_6F1A_8CAF_7891_2B70_4E4A_8964) as usize] = [()];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn published_vectors_and_hex_width() {
        assert_eq!(fnv1a32(b""), 0x811C_9DC5);
        assert_eq!(fnv1a32(b"a"), 0xE40C_292C);
        assert_eq!(fnv1a32(b"foobar"), 0xBF9C_F968);
        assert_eq!(fnv1a64(b""), Fnv64::OFFSET_BASIS);
        assert_eq!(fnv1a64(b"a"), 0xAF63_DC4C_8601_EC8C);
        assert_eq!(fnv1a64(b"foobar"), 0x8594_4171_F739_67E8);
        assert_eq!(content_hash(b""), ContentHash::OFFSET_BASIS);
        assert_eq!(content_hash(b"a"), 0xD228_CB69_6F1A_8CAF_7891_2B70_4E4A_8964);
        let hex = content_hash_hex(b"");
        assert_eq!(hex, "6c62272e07bb014262b821756295c58d");
        assert_eq!(hex.len(), 32);
        assert!(hex.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
    }

    #[test]
    fn continuation_and_every_split() {
        let data = b"foobar";
        for i in 0..=data.len() {
            let mut h32 = Fnv32::new();
            h32.update(&data[..i]);
            let mut carried = Fnv32::from_state(h32.finish());
            carried.update(&data[i..]);
            assert_eq!(carried.finish(), fnv1a32(data));

            let mut h64 = Fnv64::new();
            h64.update(&data[..i]);
            h64.update(&data[i..]);
            assert_eq!(h64.finish(), fnv1a64(data));

            let mut h128 = ContentHash::new();
            h128.update(&data[..i]);
            h128.update(&data[i..]);
            assert_eq!(h128.finish(), content_hash(data));
            assert_eq!(h128.hex(), content_hash_hex(data));
        }
    }
}
