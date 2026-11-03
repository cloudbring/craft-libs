//! CRC-32/ISO-HDLC and CRC-32/MPEG-2.
//!
//! ISO-HDLC is the reflected checksum PNG, ZIP, gzip and Matroska store (polynomial `0x04C11DB7`,
//! reflected `0xEDB88320`, init and xorout all ones). MPEG-2 is the non-reflected PSI checksum in
//! ISO/IEC 13818-1 Annex A (same polynomial, init all ones, xorout zero).
//!
//! The ISO-HDLC path folds eight bytes at a time through const tables (Kounavis and Berry,
//! slicing-by-8). The byte tables are the Sarwate table and seven further shifts of it. MPEG-2
//! sections are short, so that path is a 256-entry const table, the same shape as the FilmCraft
//! PSI helper. Both agree with the bitwise definition; the check value is pinned in a const.

/// Reflected polynomial of CRC-32/ISO-HDLC.
const IEEE_POLY: u32 = 0xEDB8_8320;
/// CRC-32/MPEG-2 polynomial (not reflected).
const MPEG_POLY: u32 = 0x04C1_1DB7;

/// Eight Sarwate shifts. Index 0 is the one-byte table.
const IEEE: [[u32; 256]; 8] = ieee_tables();
const MPEG: [u32; 256] = mpeg_table();

const fn reflected_byte(mut value: u32) -> u32 {
    let mut bit = 0;
    while bit < 8 {
        value = if value & 1 != 0 { (value >> 1) ^ IEEE_POLY } else { value >> 1 };
        bit += 1;
    }
    value
}

const fn ieee_tables() -> [[u32; 256]; 8] {
    let mut tables = [[0u32; 256]; 8];
    let mut i = 0;
    while i < 256 {
        tables[0][i] = reflected_byte(i as u32);
        i += 1;
    }
    let mut shift = 1;
    while shift < 8 {
        i = 0;
        while i < 256 {
            let prev = tables[shift - 1][i];
            tables[shift][i] = (prev >> 8) ^ tables[0][(prev & 0xff) as usize];
            i += 1;
        }
        shift += 1;
    }
    tables
}

const fn mpeg_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut crc = (i as u32) << 24;
        let mut bit = 0;
        while bit < 8 {
            crc = if crc & 0x8000_0000 != 0 { (crc << 1) ^ MPEG_POLY } else { crc << 1 };
            bit += 1;
        }
        table[i] = crc;
        i += 1;
    }
    table
}

/// The one-byte table matches the published Sarwate entries, so a bad generator fails the build.
const _: [(); (IEEE[0][1] == 0x7707_3096) as usize] = [()];
const _: [(); (IEEE[0][2] == 0xEE0E_612C) as usize] = [()];

/// CRC-32/ISO-HDLC (IEEE 802.3, ISO 3309, ITU-T V.42). PNG, ZIP, gzip and Matroska.
///
/// The value [`Crc32::finish`] returns is the register xored with `0xFFFFFFFF`. Keep the `Crc32`
/// between chunks; it is [`Copy`]. [`Crc32::finish`] does not reset it.
#[derive(Clone, Copy, PartialEq, Eq)]
#[must_use = "call update and finish, or the checksum is discarded"]
pub struct Crc32 {
    /// Reflected register, before the final xor. Starts at `0xFFFFFFFF`.
    state: u32,
}

impl Crc32 {
    /// Register of no bytes (`0xFFFFFFFF`, before the final xor).
    #[inline]
    pub const fn new() -> Self {
        Self { state: 0xFFFF_FFFF }
    }

    /// Fold `data` into the register.
    #[inline]
    pub const fn update(&mut self, data: &[u8]) {
        let mut i = 0;
        while data.len() - i >= 8 {
            self.state = fold8(self.state, [data[i], data[i + 1], data[i + 2], data[i + 3], data[i + 4], data[i + 5], data[i + 6], data[i + 7]]);
            i += 8;
        }
        if data.len() - i >= 4 {
            self.state = fold4(self.state, [data[i], data[i + 1], data[i + 2], data[i + 3]]);
            i += 4;
        }
        while i < data.len() {
            self.state = fold1(self.state, data[i]);
            i += 1;
        }
    }

    /// Current checksum. Does not change the register, so more [`update`](Self::update) calls
    /// include the bytes already folded.
    #[inline]
    #[must_use]
    pub const fn finish(&self) -> u32 {
        !self.state
    }

    /// Checksum of `data` in one call.
    #[inline]
    #[must_use]
    pub const fn digest(data: &[u8]) -> u32 {
        let mut crc = Self::new();
        crc.update(data);
        crc.finish()
    }

    /// Back to [`new`](Self::new).
    #[inline]
    pub const fn reset(&mut self) {
        *self = Self::new();
    }
}

impl Default for Crc32 {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for Crc32 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Crc32({:#010x})", self.finish())
    }
}

/// First byte in the low position: that is the byte the reflected polynomial consumes first.
/// `from_le_bytes` does that on every host, with no unaligned load.
#[inline]
const fn fold8(crc: u32, block: [u8; 8]) -> u32 {
    let mixed = crc ^ u32::from_le_bytes([block[0], block[1], block[2], block[3]]);
    IEEE[0][block[7] as usize]
        ^ IEEE[1][block[6] as usize]
        ^ IEEE[2][block[5] as usize]
        ^ IEEE[3][block[4] as usize]
        ^ IEEE[4][(mixed >> 24) as usize]
        ^ IEEE[5][((mixed >> 16) & 0xff) as usize]
        ^ IEEE[6][((mixed >> 8) & 0xff) as usize]
        ^ IEEE[7][(mixed & 0xff) as usize]
}

#[inline]
const fn fold4(crc: u32, block: [u8; 4]) -> u32 {
    let mixed = crc ^ u32::from_le_bytes(block);
    IEEE[0][(mixed >> 24) as usize]
        ^ IEEE[1][((mixed >> 16) & 0xff) as usize]
        ^ IEEE[2][((mixed >> 8) & 0xff) as usize]
        ^ IEEE[3][(mixed & 0xff) as usize]
}

#[inline]
const fn fold1(crc: u32, byte: u8) -> u32 {
    IEEE[0][((crc ^ byte as u32) & 0xff) as usize] ^ (crc >> 8)
}

/// CRC-32/ISO-HDLC of `data`. Check value: `crc32(b"123456789") == 0xCBF43926`.
#[doc(alias = "crc32_ieee")]
#[doc(alias = "crc32_iso_hdlc")]
#[inline]
#[must_use]
pub const fn crc32(data: &[u8]) -> u32 {
    Crc32::digest(data)
}

/// CRC-32/MPEG-2 (ISO/IEC 13818-1 Annex A). Non-reflected, init all ones, xorout zero.
///
/// [`Crc32Mpeg2::finish`] is the register itself. Empty input is `0xFFFFFFFF`.
#[derive(Clone, Copy, PartialEq, Eq)]
#[must_use = "call update and finish, or the checksum is discarded"]
pub struct Crc32Mpeg2 {
    state: u32,
}

impl Crc32Mpeg2 {
    /// Register of no bytes.
    #[inline]
    pub const fn new() -> Self {
        Self { state: 0xFFFF_FFFF }
    }

    /// Fold `data` into the register.
    #[inline]
    pub const fn update(&mut self, data: &[u8]) {
        let mut i = 0;
        while i < data.len() {
            self.state = (self.state << 8) ^ MPEG[((self.state >> 24) as u8 ^ data[i]) as usize];
            i += 1;
        }
    }

    /// Current checksum. Does not change the register.
    #[inline]
    #[must_use]
    pub const fn finish(&self) -> u32 {
        self.state
    }

    /// Checksum of `data` in one call.
    #[inline]
    #[must_use]
    pub const fn digest(data: &[u8]) -> u32 {
        let mut crc = Self::new();
        crc.update(data);
        crc.finish()
    }

    /// Back to [`new`](Self::new).
    #[inline]
    pub const fn reset(&mut self) {
        *self = Self::new();
    }
}

impl Default for Crc32Mpeg2 {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for Crc32Mpeg2 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Crc32Mpeg2({:#010x})", self.finish())
    }
}

/// CRC-32/MPEG-2 of `data`. Check value: `crc32_mpeg2(b"123456789") == 0x0376E6E7`.
#[inline]
#[must_use]
pub const fn crc32_mpeg2(data: &[u8]) -> u32 {
    Crc32Mpeg2::digest(data)
}

/// Catalogue check values. A wrong fold fails the build, before any test runs.
const _: [(); (crc32(b"123456789") == 0xCBF4_3926) as usize] = [()];
const _: [(); (crc32(b"") == 0) as usize] = [()];
const _: [(); (crc32_mpeg2(b"123456789") == 0x0376_E6E7) as usize] = [()];
const _: [(); (crc32_mpeg2(b"") == 0xFFFF_FFFF) as usize] = [()];

#[cfg(test)]
mod tests {
    use super::*;

    fn ieee_bits(data: &[u8]) -> u32 {
        let mut crc = 0xFFFF_FFFFu32;
        for &b in data {
            crc ^= u32::from(b);
            let mut bit = 0;
            while bit < 8 {
                crc = if crc & 1 != 0 { (crc >> 1) ^ IEEE_POLY } else { crc >> 1 };
                bit += 1;
            }
        }
        !crc
    }

    fn mpeg_bits(data: &[u8]) -> u32 {
        let mut crc = 0xFFFF_FFFFu32;
        for &b in data {
            crc ^= u32::from(b) << 24;
            let mut bit = 0;
            while bit < 8 {
                crc = if crc & 0x8000_0000 != 0 { (crc << 1) ^ MPEG_POLY } else { crc << 1 };
                bit += 1;
            }
        }
        crc
    }

    #[test]
    fn check_values_and_the_png_iend_chunk() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(crc32(b"IEND"), 0xAE42_6082);
        assert_eq!(crc32_mpeg2(b"123456789"), 0x0376_E6E7);
        let mut crc = Crc32::new();
        crc.update(b"123456789");
        assert_eq!(format!("{crc:?}"), "Crc32(0xcbf43926)");
    }

    #[test]
    fn slicing_matches_the_bitwise_definition_at_every_length() {
        let mut data = Vec::new();
        for n in 0u8..96 {
            assert_eq!(crc32(&data), ieee_bits(&data), "ieee len {n}");
            assert_eq!(crc32_mpeg2(&data), mpeg_bits(&data), "mpeg len {n}");
            data.push(n.wrapping_mul(17));
        }
        for b in 0..=255u8 {
            assert_eq!(crc32(&[b]), ieee_bits(&[b]));
            assert_eq!(crc32_mpeg2(&[b]), mpeg_bits(&[b]));
        }
    }

    #[test]
    fn every_split_matches_one_shot() {
        let data: Vec<u8> = (0..40).collect();
        for i in 0..=data.len() {
            let mut ieee = Crc32::new();
            ieee.update(&data[..i]);
            ieee.update(&data[i..]);
            assert_eq!(ieee.finish(), crc32(&data));
            let mut mpeg = Crc32Mpeg2::new();
            mpeg.update(&data[..i]);
            mpeg.update(&data[i..]);
            assert_eq!(mpeg.finish(), crc32_mpeg2(&data));
        }
        let mut crc = Crc32::new();
        crc.update(b"");
        assert_eq!(crc.finish(), 0);
        crc.reset();
        assert_eq!(crc.finish(), 0);
    }
}
