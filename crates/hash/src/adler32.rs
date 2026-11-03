//! Adler-32 (RFC 1950), the checksum zlib stores in a gzip-less zlib wrapper and in raw deflate
//! trailers used by PNG.
//!
//! Two sums modulo 65521: `s1` starts at 1 and adds each byte, `s2` starts at 0 and adds `s1`.
//! The modulus is deferred for 5552 bytes, the largest count at which both sums still fit in a
//! `u32` when they start below the modulus (zlib's `NMAX`). A shorter chunk is reduced at the end
//! of [`Adler32::update`], so splitting the input does not change the result.

const BASE: u32 = 65521;
/// See the module docs. `255·n·(n+1)/2 + (n+1)·(BASE-1)` for `n = 5552` is below `u32::MAX`.
const NMAX: usize = 5552;

/// Adler-32. [`Adler32::finish`] is `(s2 << 16) | s1` and does not reset the sums.
#[derive(Clone, Copy, PartialEq, Eq)]
#[must_use = "call update and finish, or the checksum is discarded"]
pub struct Adler32 {
    s1: u32,
    s2: u32,
}

impl Adler32 {
    /// Sums of no bytes: `s1 = 1`, `s2 = 0`, so the checksum is 1.
    #[inline]
    pub const fn new() -> Self {
        Self { s1: 1, s2: 0 }
    }

    /// Fold `data` into the sums.
    #[inline]
    pub const fn update(&mut self, data: &[u8]) {
        let mut i = 0;
        while i < data.len() {
            let end = if data.len() - i > NMAX { i + NMAX } else { data.len() };
            while i < end {
                self.s1 += data[i] as u32;
                self.s2 += self.s1;
                i += 1;
            }
            self.s1 %= BASE;
            self.s2 %= BASE;
        }
    }

    /// Current checksum. Does not change the sums.
    #[inline]
    #[must_use]
    pub const fn finish(&self) -> u32 {
        (self.s2 << 16) | self.s1
    }

    /// Checksum of `data` in one call.
    #[inline]
    #[must_use]
    pub const fn digest(data: &[u8]) -> u32 {
        let mut sum = Self::new();
        sum.update(data);
        sum.finish()
    }

    /// Back to [`new`](Self::new).
    #[inline]
    pub const fn reset(&mut self) {
        *self = Self::new();
    }
}

impl Default for Adler32 {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for Adler32 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Adler32({:#010x})", self.finish())
    }
}

/// Adler-32 of `data`. Check value: `adler32(b"Wikipedia") == 0x11E60398`. Empty input is 1.
#[inline]
#[must_use]
pub const fn adler32(data: &[u8]) -> u32 {
    Adler32::digest(data)
}

const _: [(); (adler32(b"Wikipedia") == 0x11E6_0398) as usize] = [()];
const _: [(); (adler32(b"") == 1) as usize] = [()];

#[cfg(test)]
mod tests {
    use super::*;

    /// The definition with a modulus after every byte, which is what the small in-app copies do.
    fn per_byte(data: &[u8]) -> u32 {
        let (mut s1, mut s2) = (1u32, 0u32);
        for &b in data {
            s1 = (s1 + u32::from(b)) % BASE;
            s2 = (s2 + s1) % BASE;
        }
        (s2 << 16) | s1
    }

    #[test]
    fn check_value_and_deferred_modulus_agree() {
        assert_eq!(adler32(b"Wikipedia"), 0x11E6_0398);
        assert_eq!(adler32(b""), 1);
        let mut data = Vec::new();
        for n in 0..NMAX + 64 {
            assert_eq!(adler32(&data), per_byte(&data), "len {n}");
            data.push(n as u8);
        }
    }

    #[test]
    fn every_split_matches_one_shot() {
        let data: Vec<u8> = (0..NMAX + 10).map(|n| n as u8).collect();
        for i in [0, 1, 5551, 5552, 5553, data.len()] {
            let mut sum = Adler32::new();
            sum.update(&data[..i]);
            sum.update(&data[i..]);
            assert_eq!(sum.finish(), adler32(&data));
        }
    }
}
