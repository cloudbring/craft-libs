// Vendored verbatim from https://github.com/storytold/wordcraft at b47079fc791595fd84bb499d64dcd3f0624ef35a
// path: crates/doc/src/cfb_guard.rs
// Only change: the trailing '#[cfg(test)]' items are dropped (they are the app's own unit tests and need its dev-dependencies); nothing else is edited.

//! A bytes-only check of an OLE compound file ([MS-CFB]) before the `cfb` crate opens it
//! (#3642). Every reader of `.doc`, `.xls` and encrypted Office packages calls [`check`] first.

/// Check a compound file's FAT layout before `cfb` reads it ([MS-CFB] §2.2 header, §2.5
/// DIFAT). `cfb` appends a whole sector of FAT entries for every FAT sector the DIFAT lists and
/// only validates the FAT once it is built, so a small header naming one sector over and over
/// makes it allocate gigabytes. Here the DIFAT is walked the way `cfb` walks it (header entries,
/// then the DIFAT chain, at most one step per sector; trailing zero and free entries trimmed
/// the same way) and the file is refused when it lists one FAT sector twice, or more FAT sectors
/// than its sectors need (one FAT sector covers `sector_len / 4` of them, plus one spare), so the
/// FAT `cfb` builds is about the size the file needs. Anything else that is wrong is left for
/// `cfb` to report.
pub fn check(bytes: &[u8]) -> Result<(), String> {
    const MAGIC: [u8; 8] = [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];
    const FREE: u32 = 0xFFFF_FFFF;
    const END_OF_CHAIN: u32 = 0xFFFF_FFFE;
    const HEADER_ENTRIES: usize = 109;
    let u16_at = |at: usize| bytes.get(at..at.checked_add(2)?).map(|b| u16::from_le_bytes([b[0], b[1]]));
    let u32_at = |at: usize| bytes.get(at..at.checked_add(4)?).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    if bytes.len() < 512 || bytes.get(..8) != Some(MAGIC.as_slice()) {
        return Ok(());
    }
    let sector_len: usize = match u16_at(0x1A) {
        Some(3) => 512,
        Some(4) => 4096,
        _ => return Ok(()),
    };
    if bytes.len() < sector_len {
        return Ok(());
    }
    // `cfb` counts the sectors after the header sector, the last one possibly partial.
    let num_sectors = bytes.len().div_ceil(sector_len) - 1;
    let num_fat_sectors = u32_at(0x2C).unwrap_or(0) as usize;
    let mut difat: Vec<u32> = (0..HEADER_ENTRIES).filter_map(|i| u32_at(0x4C + i * 4)).collect();
    let mut next = u32_at(0x44).unwrap_or(END_OF_CHAIN);
    let mut steps = 0usize;
    while next != END_OF_CHAIN && next != FREE {
        steps += 1;
        // `cfb` refuses a chain sector past the file or seen before; a chain longer than the
        // file has sectors must have done one of the two.
        if steps > num_sectors || next as usize >= num_sectors {
            return Err(format!("the DIFAT chain is longer than the file or leaves it (sector {next})"));
        }
        let at = (next as usize + 1) * sector_len;
        let entries = sector_len / 4 - 1;
        difat.extend((0..entries).map(|i| u32_at(at + i * 4).unwrap_or(FREE)));
        next = u32_at(at + entries * 4).unwrap_or(END_OF_CHAIN);
    }
    while difat.len() > HEADER_ENTRIES && difat.len() > num_fat_sectors && difat.last() == Some(&0) {
        difat.pop();
    }
    while difat.last() == Some(&FREE) {
        difat.pop();
    }
    // Each listed FAT sector adds a whole sector of entries. A FAT sector holds the entries of
    // `sector_len / 4` sectors, so a file needs no more FAT sectors than that covers (one spare),
    // and none of them twice. A free entry in the middle is left for `cfb` to refuse.
    let listed: Vec<u32> = difat.into_iter().filter(|&s| s != FREE).collect();
    let needed = num_sectors.div_ceil(sector_len / 4).saturating_add(1);
    if listed.len() > needed {
        return Err(format!("the DIFAT lists {} FAT sectors in a file of {num_sectors} sectors", listed.len()));
    }
    let mut seen = std::collections::HashSet::with_capacity(listed.len());
    if let Some(dup) = listed.iter().find(|&&s| !seen.insert(s)) {
        return Err(format!("the DIFAT lists FAT sector {dup} twice"));
    }
    Ok(())
}

