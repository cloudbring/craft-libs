//! Shipping checksums, copied verbatim, so a later migration can show `craft-hash` matches them.
//!
//! Each body is the function as it stands in that file. `pub` is added only where the original
//! is private or `pub(crate)`, so this test can call it. The paths are the ones in
//! cloudbring/artcraft-algorithms#65 (CRC) and the FNV / SHA-256 copies named with #37.
//! `deckcraft/crates/pptx/src/png.rs` and `craft-libs/crates/media/src/bitstream.rs` are the same
//! bitwise IEEE loop as the EffectCraft copy; both are here because #65 lists them separately.

use proptest::prelude::*;

// lightcraft/crates/denoise-core/src/archive.rs — rebuilds the 256-entry table on every call.
mod lightcraft_denoise_core_archive {
    /// CRC-32 (IEEE) of `bytes`.
    pub fn crc32(bytes: &[u8]) -> u32 {
        let mut table = [0u32; 256];
        for (i, t) in table.iter_mut().enumerate() {
            let mut c = i as u32;
            for _ in 0..8 {
                c = if c & 1 != 0 { 0xedb8_8320 ^ (c >> 1) } else { c >> 1 };
            }
            *t = c;
        }
        !bytes.iter().fold(!0u32, |c, &b| table[((c ^ u32::from(b)) & 0xff) as usize] ^ (c >> 8))
    }
}

// effectcraft/crates/lottie/src/zip.rs — bitwise IEEE.
mod effectcraft_lottie_zip {
    pub fn crc32(data: &[u8]) -> u32 {
        let mut crc = 0xFFFF_FFFFu32;
        for &b in data {
            crc ^= b as u32;
            for _ in 0..8 {
                crc = if crc & 1 != 0 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
            }
        }
        !crc
    }
}

// craft-anything/crates/formats/src/zip.rs — bitwise IEEE, branch-free mask.
mod craft_anything_formats_zip {
    /// CRC-32 (IEEE, as used by ZIP and PNG).
    pub fn crc32(data: &[u8]) -> u32 {
        let mut crc = 0xffff_ffffu32;
        for &b in data {
            crc ^= u32::from(b);
            for _ in 0..8 {
                let mask = (crc & 1).wrapping_neg();
                crc = (crc >> 1) ^ (0xedb8_8320 & mask);
            }
        }
        !crc
    }
}

// deckcraft/crates/pptx/src/png.rs — bitwise IEEE and a per-byte Adler-32.
mod deckcraft_pptx_png {
    pub fn crc32(data: &[u8]) -> u32 {
        let mut c = 0xFFFF_FFFFu32;
        for b in data {
            c ^= *b as u32;
            for _ in 0..8 {
                c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
            }
        }
        !c
    }

    pub fn adler32(data: &[u8]) -> u32 {
        let (mut a, mut b) = (1u32, 0u32);
        for x in data {
            a = (a + *x as u32) % 65521;
            b = (b + a) % 65521;
        }
        (b << 16) | a
    }
}

// filmcraft/crates/mpegts/src/ts.rs — MPEG-2 PSI, const table, MSB first.
mod filmcraft_mpegts_ts {
    /// Byte-wise table for the MSB-first CRC-32 (polynomial 0x04C11DB7): entry `i` is the CRC register
    /// after shifting the byte `i` through eight bit steps.
    const CRC_TABLE: [u32; 256] = {
        let mut t = [0u32; 256];
        let mut i = 0;
        while i < 256 {
            let mut crc = (i as u32) << 24;
            let mut bit = 0;
            while bit < 8 {
                crc = if crc & 0x8000_0000 != 0 { (crc << 1) ^ 0x04C1_1DB7 } else { crc << 1 };
                bit += 1;
            }
            t[i] = crc;
            i += 1;
        }
        t
    };

    /// CRC-32 of PSI sections (MSB-first, polynomial 0x04C11DB7, initial value all ones; Annex A).
    pub fn crc32_mpeg(data: &[u8]) -> u32 {
        let mut crc = 0xFFFF_FFFFu32;
        for &b in data {
            crc = (crc << 8) ^ CRC_TABLE[((crc >> 24) as u8 ^ b) as usize];
        }
        crc
    }
}

// craft-libs/crates/media/src/bitstream.rs — bitwise IEEE.
mod craft_libs_media_bitstream {
    /// CRC-32 (IEEE, reflected) — used by containers (e.g. Matroska CRC-32 elements, PNG).
    pub fn crc32(data: &[u8]) -> u32 {
        let mut crc = 0xFFFF_FFFFu32;
        for &b in data {
            crc ^= b as u32;
            for _ in 0..8 {
                crc = if crc & 1 != 0 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
            }
        }
        !crc
    }
}

// craft-libs/crates/media/src/matroska/ebml.rs — OnceLock table, incremental register.
mod craft_libs_media_ebml {
    /// CRC-32 (ISO 3309 / IEEE 802.3, reflected polynomial 0xEDB88320), as used by EBML CRC-32 elements.
    pub fn crc32(data: &[u8]) -> u32 {
        crc32_update(0xFFFF_FFFF, data) ^ 0xFFFF_FFFF
    }

    /// Incremental form: start with `0xFFFF_FFFF`, finish with `^ 0xFFFF_FFFF`.
    pub fn crc32_update(mut crc: u32, data: &[u8]) -> u32 {
        static TABLE: std::sync::OnceLock<[u32; 256]> = std::sync::OnceLock::new();
        let t = TABLE.get_or_init(|| {
            let mut t = [0u32; 256];
            for (i, e) in t.iter_mut().enumerate() {
                let mut c = i as u32;
                for _ in 0..8 {
                    c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
                }
                *e = c;
            }
            t
        });
        for &b in data {
            crc = t[((crc ^ b as u32) & 0xFF) as usize] ^ (crc >> 8);
        }
        crc
    }
}

// vectorcraft/crates/svg/src/lib.rs — FNV-1a 64, offset basis then xor-multiply.
mod vectorcraft_svg {
    const FNV_SEED: u64 = 0xcbf29ce484222325;

    /// FNV-1a: a stable content hash (image keys, unique id prefixes).
    pub fn fnv1a(bytes: &[u8]) -> u64 {
        fnv1a_from(FNV_SEED, bytes)
    }

    /// FNV-1a of `bytes` continued from hash `h`.
    pub fn fnv1a_from(h: u64, bytes: &[u8]) -> u64 {
        bytes.iter().fold(h, |h, b| (h ^ *b as u64).wrapping_mul(0x100000001b3))
    }
}

// craft-libs/crates/fonts-build/src/sha256.rs — pure FIPS 180-4, independent of the sha2 crate.
mod craft_libs_fonts_build_sha256 {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be,
        0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa,
        0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85,
        0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3,
        0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f,
        0x682e6ff3, 0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
    ];

    /// The lowercase hex SHA-256 of `data`, as craft-fonts' manifest writes it.
    pub fn hex(data: &[u8]) -> String {
        let mut h: [u32; 8] = [0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19];
        let mut message = data.to_vec();
        message.push(0x80);
        while message.len() % 64 != 56 {
            message.push(0);
        }
        message.extend_from_slice(&(u64::try_from(data.len()).unwrap_or(u64::MAX).wrapping_mul(8)).to_be_bytes());
        for block in message.as_chunks::<64>().0 {
            let mut w = [0u32; 64];
            for (i, word) in block.as_chunks::<4>().0.iter().enumerate() {
                w[i] = u32::from_be_bytes(*word);
            }
            for i in 16..64 {
                let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
                let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
                w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
            }
            let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = h;
            for (k, w) in K.iter().zip(w) {
                let t1 = hh
                    .wrapping_add(e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25))
                    .wrapping_add((e & f) ^ (!e & g))
                    .wrapping_add(*k)
                    .wrapping_add(w);
                let t2 = (a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22)).wrapping_add((a & b) ^ (a & c) ^ (b & c));
                (hh, g, f, e, d, c, b, a) = (g, f, e, d.wrapping_add(t1), c, b, a, t1.wrapping_add(t2));
            }
            for (h, v) in h.iter_mut().zip([a, b, c, d, e, f, g, hh]) {
                *h = h.wrapping_add(v);
            }
        }
        h.iter().map(|v| format!("{v:08x}")).collect()
    }
}

/// Split `data` at the cut points and call `update` once per piece, including empty pieces.
///
/// Cuts are offsets, not a walk. A later cut smaller than an earlier one is still a
/// partition point: the pieces stay in order and every byte is hashed once. Duplicate
/// cuts produce an empty chunk, which is a legal streaming update.
fn feed(data: &[u8], cuts: &[usize], mut update: impl FnMut(&[u8])) {
    if data.is_empty() {
        for _ in cuts {
            update(&[]);
        }
        update(&[]);
        return;
    }
    let mut points: Vec<usize> = cuts.iter().map(|cut| cut % (data.len() + 1)).collect();
    points.sort_unstable();
    let mut at = 0usize;
    for end in points {
        update(&data[at..end]);
        at = end;
    }
    update(&data[at..]);
}

#[test]
fn known_inputs_match_the_copied_functions() {
    let zeros = [0u8; 32];
    let long: Vec<u8> = (0..80).collect();
    for data in [b"" as &[u8], b"123456789", b"IEND", b"Wikipedia", b"foobar", b"abc", &zeros, &long] {
        let ieee = craft_hash::crc32(data);
        assert_eq!(ieee, lightcraft_denoise_core_archive::crc32(data), "lightcraft");
        assert_eq!(ieee, effectcraft_lottie_zip::crc32(data), "effectcraft");
        assert_eq!(ieee, craft_anything_formats_zip::crc32(data), "craft-anything");
        assert_eq!(ieee, deckcraft_pptx_png::crc32(data), "deckcraft");
        assert_eq!(ieee, craft_libs_media_bitstream::crc32(data), "bitstream");
        assert_eq!(ieee, craft_libs_media_ebml::crc32(data), "ebml");
        assert_eq!(craft_hash::crc32_mpeg2(data), filmcraft_mpegts_ts::crc32_mpeg(data), "mpeg");
        assert_eq!(craft_hash::adler32(data), deckcraft_pptx_png::adler32(data), "adler");
        assert_eq!(craft_hash::fnv1a64(data), vectorcraft_svg::fnv1a(data), "fnv");
        assert_eq!(craft_hash::sha256_hex(data), craft_libs_fonts_build_sha256::hex(data), "sha256");
    }
}

#[test]
fn exhaustive_short_inputs_match_two_ieee_copies_and_mpeg() {
    assert_eq!(craft_hash::crc32(b""), effectcraft_lottie_zip::crc32(b""));
    assert_eq!(craft_hash::crc32(b""), lightcraft_denoise_core_archive::crc32(b""));
    for a in 0..=255u8 {
        let one = [a];
        assert_eq!(craft_hash::crc32(&one), effectcraft_lottie_zip::crc32(&one));
        assert_eq!(craft_hash::crc32_mpeg2(&one), filmcraft_mpegts_ts::crc32_mpeg(&one));
        assert_eq!(craft_hash::adler32(&one), deckcraft_pptx_png::adler32(&one));
    }
    for a in 0..=255u8 {
        for b in 0..=255u8 {
            let two = [a, b];
            assert_eq!(craft_hash::crc32(&two), craft_anything_formats_zip::crc32(&two));
            assert_eq!(craft_hash::crc32(&two), effectcraft_lottie_zip::crc32(&two));
        }
    }
}

#[test]
fn ebml_incremental_register_matches_one_shot() {
    let data = b"matroska-crc";
    for i in 0..=data.len() {
        let mid = craft_libs_media_ebml::crc32_update(0xFFFF_FFFF, &data[..i]);
        let full = craft_libs_media_ebml::crc32_update(mid, &data[i..]) ^ 0xFFFF_FFFF;
        assert_eq!(full, craft_hash::crc32(data));
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn ieee_matches_the_copied_implementations(data in proptest::collection::vec(any::<u8>(), 0..2048usize)) {
        let got = craft_hash::crc32(&data);
        prop_assert_eq!(got, lightcraft_denoise_core_archive::crc32(&data));
        prop_assert_eq!(got, effectcraft_lottie_zip::crc32(&data));
        prop_assert_eq!(got, craft_anything_formats_zip::crc32(&data));
        prop_assert_eq!(got, deckcraft_pptx_png::crc32(&data));
        prop_assert_eq!(got, craft_libs_media_bitstream::crc32(&data));
        prop_assert_eq!(got, craft_libs_media_ebml::crc32(&data));
    }

    #[test]
    fn mpeg_adler_fnv_and_sha_match_the_copied_implementations(data in proptest::collection::vec(any::<u8>(), 0..1024usize)) {
        prop_assert_eq!(craft_hash::crc32_mpeg2(&data), filmcraft_mpegts_ts::crc32_mpeg(&data));
        prop_assert_eq!(craft_hash::adler32(&data), deckcraft_pptx_png::adler32(&data));
        prop_assert_eq!(craft_hash::fnv1a64(&data), vectorcraft_svg::fnv1a(&data));
        let mid = data.len() / 2;
        // `fnv1a_from` continues a register. `fnv1a(prefix)` is that register, so carrying it
        // across the rest is the hash of the whole buffer, same as `Fnv64::from_state`.
        prop_assert_eq!(vectorcraft_svg::fnv1a_from(vectorcraft_svg::fnv1a(&data[..mid]), &data[mid..]), craft_hash::fnv1a64(&data));
        let mut carried = craft_hash::Fnv64::from_state(craft_hash::fnv1a64(&data[..mid]));
        carried.update(&data[mid..]);
        prop_assert_eq!(carried.finish(), vectorcraft_svg::fnv1a(&data));
        prop_assert_eq!(craft_hash::sha256_hex(&data), craft_libs_fonts_build_sha256::hex(&data));
    }

    #[test]
    fn streaming_matches_one_shot_for_arbitrary_cuts(
        data in proptest::collection::vec(any::<u8>(), 0..3000usize),
        cuts in proptest::collection::vec(any::<usize>(), 0..12usize),
    ) {
        let mut ieee = craft_hash::Crc32::new();
        feed(&data, &cuts, |chunk| ieee.update(chunk));
        prop_assert_eq!(ieee.finish(), craft_hash::crc32(&data));

        let mut mpeg = craft_hash::Crc32Mpeg2::new();
        feed(&data, &cuts, |chunk| mpeg.update(chunk));
        prop_assert_eq!(mpeg.finish(), craft_hash::crc32_mpeg2(&data));

        let mut adler = craft_hash::Adler32::new();
        feed(&data, &cuts, |chunk| adler.update(chunk));
        prop_assert_eq!(adler.finish(), craft_hash::adler32(&data));

        let mut fnv32 = craft_hash::Fnv32::new();
        feed(&data, &cuts, |chunk| fnv32.update(chunk));
        prop_assert_eq!(fnv32.finish(), craft_hash::fnv1a32(&data));

        let mut fnv64 = craft_hash::Fnv64::new();
        feed(&data, &cuts, |chunk| fnv64.update(chunk));
        prop_assert_eq!(fnv64.finish(), craft_hash::fnv1a64(&data));

        let mut content = craft_hash::ContentHash::new();
        feed(&data, &cuts, |chunk| content.update(chunk));
        prop_assert_eq!(content.finish(), craft_hash::content_hash(&data));

        let mut sha = craft_hash::Sha256::new();
        feed(&data, &cuts, |chunk| sha.update(chunk));
        prop_assert_eq!(sha.hex(), craft_hash::sha256_hex(&data));
    }
}
