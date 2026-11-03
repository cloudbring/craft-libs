#![allow(clippy::unwrap_used, clippy::expect_used)]

use craft_cfb::{Cfb, EntryKind};
use proptest::prelude::*;

#[allow(dead_code)]
#[rustfmt::skip]
#[path = "oracles/gridcraft.rs"]
mod gridcraft;
#[allow(dead_code)]
#[rustfmt::skip]
#[path = "oracles/deckcraft.rs"]
mod deckcraft;

const FREE: u32 = 0xffff_ffff;
const END: u32 = 0xffff_fffe;
const FAT: u32 = 0xffff_fffd;
const DIFAT: u32 = 0xffff_fffc;

fn put16(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn put32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn directory(bytes: &mut [u8], offset: usize, name: &str, kind: u8, links: [u32; 3], stream: (u32, u64)) {
    for (index, unit) in name.encode_utf16().enumerate() {
        put16(bytes, offset + index * 2, unit);
    }
    put16(bytes, offset + 64, (name.len() as u16 + 1) * 2);
    bytes[offset + 66] = kind;
    bytes[offset + 67] = 1;
    put32(bytes, offset + 68, links[1]);
    put32(bytes, offset + 72, links[2]);
    put32(bytes, offset + 76, links[0]);
    put32(bytes, offset + 116, stream.0);
    put32(bytes, offset + 120, stream.1 as u32);
    put32(bytes, offset + 124, (stream.1 >> 32) as u32);
}

struct Fixture {
    bytes: Vec<u8>,
    sector_size: usize,
    directory_sector: usize,
    regular_sector: usize,
    mini_sector: usize,
    fat_sector: usize,
    root_sector: usize,
}

impl Fixture {
    fn set_metadata(&mut self, index: usize, clsid: [u8; 16], state_bits: u32, created: u64, modified: u64) {
        let offset = (self.directory_sector + 1) * self.sector_size + index * 128;
        self.bytes[offset + 80..offset + 96].copy_from_slice(&clsid);
        put32(&mut self.bytes, offset + 96, state_bits);
        self.bytes[offset + 100..offset + 108].copy_from_slice(&created.to_le_bytes());
        self.bytes[offset + 108..offset + 116].copy_from_slice(&modified.to_le_bytes());
    }
}

fn build(major: u16, fat_count: usize) -> Fixture {
    let sector_size = if major == 3 { 512 } else { 4096 };
    let per = sector_size / 4;
    let difat_count = fat_count.saturating_sub(109).div_ceil(per - 1);
    let directory_sector = fat_count + difat_count;
    let regular_sector = directory_sector + 2;
    let regular_count = 4096 / sector_size;
    let mini_sector = regular_sector + regular_count;
    let root_sector = mini_sector + 1;
    let count = if fat_count > 1 { fat_count * per } else { root_sector + 1 };
    let mut bytes = vec![0u8; (count + 1) * sector_size];
    bytes[..8].copy_from_slice(&[0xd0, 0xcf, 0x11, 0xe0, 0xa1, 0xb1, 0x1a, 0xe1]);
    put16(&mut bytes, 26, major);
    put16(&mut bytes, 28, 0xfffe);
    put16(&mut bytes, 30, if major == 3 { 9 } else { 12 });
    put16(&mut bytes, 32, 6);
    put32(&mut bytes, 44, fat_count as u32);
    put32(&mut bytes, 48, directory_sector as u32);
    put32(&mut bytes, 56, 4096);
    put32(&mut bytes, 60, mini_sector as u32);
    put32(&mut bytes, 64, 1);
    put32(&mut bytes, 68, if difat_count == 0 { END } else { fat_count as u32 });
    put32(&mut bytes, 72, difat_count as u32);
    for index in 0..109 {
        put32(&mut bytes, 76 + 4 * index, if index < fat_count { index as u32 } else { FREE });
    }
    for difat_index in 0..difat_count {
        let id = fat_count + difat_index;
        let offset = (id + 1) * sector_size;
        for index in 0..per - 1 {
            let fat_id = 109 + difat_index * (per - 1) + index;
            put32(&mut bytes, offset + index * 4, if fat_id < fat_count { fat_id as u32 } else { FREE });
        }
        put32(&mut bytes, offset + sector_size - 4, if difat_index + 1 == difat_count { END } else { (id + 1) as u32 });
    }
    for index in 0..fat_count {
        let offset = (index + 1) * sector_size;
        for entry in 0..per {
            put32(&mut bytes, offset + 4 * entry, FREE);
        }
    }
    let mut fat_entry = |id: usize, value: u32| {
        let offset = (id / per + 1) * sector_size + (id % per) * 4;
        put32(&mut bytes, offset, value);
    };
    for id in 0..fat_count {
        fat_entry(id, FAT);
    }
    for id in fat_count..fat_count + difat_count {
        fat_entry(id, DIFAT);
    }
    fat_entry(directory_sector, (directory_sector + 1) as u32);
    fat_entry(directory_sector + 1, END);
    for id in regular_sector..regular_sector + regular_count {
        fat_entry(id, if id + 1 == regular_sector + regular_count { END } else { (id + 1) as u32 });
    }
    fat_entry(mini_sector, END);
    fat_entry(root_sector, END);
    let dir = (directory_sector + 1) * sector_size;
    directory(&mut bytes, dir, "Root Entry", 5, [1, FREE, FREE], (root_sector as u32, 128));
    directory(&mut bytes, dir + 128, "EncryptionInfo", 2, [FREE, 3, 2], (0, 5));
    directory(&mut bytes, dir + 256, "EncryptedPackage", 2, [FREE, FREE, FREE], (regular_sector as u32, 4096));
    directory(&mut bytes, dir + 384, "Nested", 1, [4, FREE, FREE], (END, 0));
    bytes[dir + 256 + 67] = 0;
    bytes[dir + 384 + 67] = 0;
    directory(&mut bytes, dir + 512, "Inner", 2, [FREE, FREE, FREE], (1, 5));
    for index in 0..regular_count {
        bytes[(regular_sector + index + 1) * sector_size..(regular_sector + index + 2) * sector_size].fill(0x42);
    }
    let mini_offset = (mini_sector + 1) * sector_size;
    put32(&mut bytes, mini_offset, END);
    put32(&mut bytes, mini_offset + 4, END);
    let root_offset = (root_sector + 1) * sector_size;
    bytes[root_offset..root_offset + 5].copy_from_slice(b"hello");
    bytes[root_offset + 64..root_offset + 69].copy_from_slice(b"world");
    Fixture { bytes, sector_size, directory_sector, regular_sector, mini_sector, fat_sector: 0, root_sector }
}

#[test]
fn directory_metadata_round_trips() {
    for major in [3, 4] {
        let mut fixture = build(major, 1);
        let clsid = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15];
        for index in [0, 3, 4] {
            fixture.set_metadata(
                index,
                clsid,
                0x1234_5678 + index as u32,
                0x1234_5678_9abc_def0 + index as u64,
                0xfedc_ba98_7654_3210 + index as u64,
            );
        }
        let cfb = Cfb::open(&fixture.bytes).unwrap();
        for (path, name, kind, index, start) in [
            ("", "", EntryKind::Root, 0, fixture.root_sector as u32),
            ("Nested", "Nested", EntryKind::Storage, 3, END),
            ("Nested/Inner", "Inner", EntryKind::Stream, 4, 1),
        ] {
            let entry = cfb.entries().find(|entry| entry.path() == path).unwrap();
            assert_eq!(entry.kind(), kind);
            assert_eq!(entry.name(), name);
            assert_eq!(entry.clsid(), clsid);
            assert_eq!(entry.state_bits(), 0x1234_5678 + index);
            assert_eq!(entry.created(), 0x1234_5678_9abc_def0 + u64::from(index));
            assert_eq!(entry.modified(), 0xfedc_ba98_7654_3210 + u64::from(index));
            assert_eq!(entry.start_sector(), start);
        }
        let unset = cfb.entries().find(|entry| entry.path() == "EncryptionInfo").unwrap();
        assert_eq!((unset.clsid(), unset.state_bits(), unset.created(), unset.modified()), ([0; 16], 0, 0, 0));
    }
}

#[test]
fn truncated_directory_record_is_rejected() {
    let fixture = build(3, 1);
    let end = (fixture.directory_sector + 1) * fixture.sector_size + 127;
    assert!(Cfb::open(&fixture.bytes[..end]).is_err());
}

#[test]
fn v3_and_v4_regular_mini_and_directory_tree() {
    for major in [3, 4] {
        let fixture = build(major, 1);
        let cfb = Cfb::open(&fixture.bytes).unwrap();
        assert_eq!(cfb.root().kind(), EntryKind::Root);
        assert_eq!(cfb.entries().map(|item| item.path()).collect::<Vec<_>>(), ["", "Nested", "Nested/Inner", "EncryptionInfo", "EncryptedPackage"]);
        assert_eq!(cfb.read_stream("encryptioninfo").unwrap(), b"hello");
        assert_eq!(cfb.read_stream("nested/inner").unwrap(), b"world");
        assert_eq!(cfb.read_stream("EncryptedPackage").unwrap(), vec![0x42; 4096]);
    }
}

#[test]
fn multi_sector_fat_and_difat() {
    for fat_count in [110, 250] {
        let fixture = build(3, fat_count);
        let cfb = Cfb::open(&fixture.bytes).unwrap();
        assert_eq!(cfb.read_stream("EncryptedPackage").unwrap(), vec![0x42; 4096]);
    }
}

#[test]
fn differential_against_original_gridcraft_and_deckcraft_copies() {
    // Verbatim pre-test prefixes of gridcraft/crates/xlsx/src/cfb.rs and
    // deckcraft/crates/pptx/src/cfb.rs live in tests/oracles/ (the latter only detects names).
    for (major, fat_count) in [(3, 1), (4, 1), (3, 110), (3, 250)] {
        let fixture = build(major, fat_count);
        let cfb = Cfb::open(&fixture.bytes).unwrap();
        let original = gridcraft::Compound::open(&fixture.bytes).unwrap();
        for name in ["EncryptionInfo", "EncryptedPackage"] {
            assert_eq!(cfb.read_stream(name).unwrap(), original.root_stream(name).unwrap().unwrap());
        }
        assert_eq!(gridcraft::is_encrypted(&fixture.bytes), deckcraft::is_encrypted(&fixture.bytes));
        assert!(deckcraft::is_encrypted(&fixture.bytes));
    }
}

#[test]
fn hostile_sector_and_directory_inputs_are_rejected() {
    let fixture = build(3, 1);
    let truncated = fixture.bytes[..200].to_vec();
    assert!(Cfb::open(&truncated).is_err());
    let fat = (fixture.fat_sector + 1) * fixture.sector_size;
    let dir = (fixture.directory_sector + 1) * fixture.sector_size;
    let mut cycle = fixture.bytes.clone();
    put32(&mut cycle, fat + (fixture.directory_sector + 1) * 4, fixture.directory_sector as u32);
    assert!(Cfb::open(&cycle).is_err());
    let mut invalid = fixture.bytes.clone();
    put32(&mut invalid, fat + fixture.directory_sector * 4, 999_999);
    assert!(Cfb::open(&invalid).is_err());
    let mut fat_cycle = fixture.bytes.clone();
    put32(&mut fat_cycle, fat + (fixture.regular_sector + 1) * 4, fixture.regular_sector as u32);
    assert!(Cfb::open(&fat_cycle).unwrap().read_stream("EncryptedPackage").is_err());
    let mut directory_loop = fixture.bytes.clone();
    put32(&mut directory_loop, dir + 128 + 68, 1);
    assert!(Cfb::open(&directory_loop).is_err());
    let mut mini_overflow = fixture.bytes.clone();
    put32(&mut mini_overflow, dir + 128 + 116, 99);
    assert!(Cfb::open(&mini_overflow).unwrap().read_stream("EncryptionInfo").is_err());
    let mut huge = fixture.bytes.clone();
    put32(&mut huge, dir + 256 + 120, u32::MAX);
    assert!(Cfb::open(&huge).unwrap().read_stream("EncryptedPackage").is_err());
    let mut repeating_fat = fixture.bytes.clone();
    put32(&mut repeating_fat, 44, u32::MAX);
    assert!(Cfb::open(&repeating_fat).is_err());
    let mut duplicate_fat = fixture.bytes.clone();
    put32(&mut duplicate_fat, 44, 2);
    put32(&mut duplicate_fat, 80, 0);
    assert!(Cfb::open(&duplicate_fat).is_err());
    let mut mini_cycle = fixture.bytes.clone();
    let mini = (fixture.mini_sector + 1) * fixture.sector_size;
    put32(&mut mini_cycle, mini, 0);
    put32(&mut mini_cycle, dir + 128 + 120, 65);
    assert!(Cfb::open(&mini_cycle).unwrap().read_stream("EncryptionInfo").is_err());
    let mut root_overflow = fixture.bytes.clone();
    put32(&mut root_overflow, dir + 120, u32::MAX);
    assert!(Cfb::open(&root_overflow).unwrap().read_stream("EncryptionInfo").is_err());
    assert_eq!(Cfb::open(&fixture.bytes).unwrap().read_stream("absent").unwrap_err().0, "stream not found");
    assert!(fixture.root_sector > fixture.mini_sector);
    assert!(fixture.regular_sector > fixture.directory_sector);
}

proptest! {
    #[test]
    fn mutations_never_panic(edits in proptest::collection::vec((0usize..8192, any::<u8>()), 0..24)) {
        let mut fixture = build(3, 1);
        for (offset, value) in edits { if let Some(byte) = fixture.bytes.get_mut(offset) { *byte = value; } }
        if let Ok(cfb) = Cfb::open(&fixture.bytes) {
            for entry in cfb.entries() {
                if entry.kind() == EntryKind::Stream { let _ = cfb.read_stream(entry.path()); }
            }
        }
    }
}
