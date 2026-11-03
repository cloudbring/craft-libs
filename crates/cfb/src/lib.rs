#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
//! Bounded read-only Compound File Binary (OLE2) reader.

use std::collections::HashSet;
use std::fmt;

const SIGNATURE: &[u8; 8] = b"\xd0\xcf\x11\xe0\xa1\xb1\x1a\xe1";
const FREE: u32 = 0xffff_ffff;
const END: u32 = 0xffff_fffe;
const MAX_SECTOR: u32 = 0xffff_fffa;
const MAX_STREAM: usize = 1 << 30;
const MAX_DIRECTORY_SECTORS: usize = 1024;

/// A malformed, unsupported, or absent compound-file object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Error(pub &'static str);

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0)
    }
}

impl std::error::Error for Error {}

fn word(bytes: &[u8], offset: usize) -> Result<u16, Error> {
    let data = bytes.get(offset..offset.checked_add(2).ok_or(Error("offset overflow"))?).ok_or(Error("truncated CFB"))?;
    Ok(u16::from_le_bytes([data[0], data[1]]))
}

fn dword(bytes: &[u8], offset: usize) -> Result<u32, Error> {
    let data = bytes.get(offset..offset.checked_add(4).ok_or(Error("offset overflow"))?).ok_or(Error("truncated CFB"))?;
    Ok(u32::from_le_bytes([data[0], data[1], data[2], data[3]]))
}

/// CFB directory object kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    Root,
    Storage,
    Stream,
}

/// A reachable directory object. The root path is the empty string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    path: String,
    kind: EntryKind,
    size: u64,
    start: u32,
    clsid: [u8; 16],
    state_bits: u32,
    created: u64,
    modified: u64,
}

impl Entry {
    /// Entry name, or an empty string for the root.
    pub fn name(&self) -> &str {
        self.path.rsplit('/').next().unwrap_or("")
    }
    /// Slash-delimited path relative to the root.
    pub fn path(&self) -> &str {
        &self.path
    }
    /// Object type.
    pub fn kind(&self) -> EntryKind {
        self.kind
    }
    /// Declared stream length in bytes.
    pub fn size(&self) -> u64 {
        self.size
    }
    /// Raw CLSID bytes in their on-disk order.
    pub fn clsid(&self) -> [u8; 16] {
        self.clsid
    }
    /// Application-defined state bits.
    pub fn state_bits(&self) -> u32 {
        self.state_bits
    }
    /// Raw creation FILETIME, or zero when unset.
    pub fn created(&self) -> u64 {
        self.created
    }
    /// Raw modification FILETIME, or zero when unset.
    pub fn modified(&self) -> u64 {
        self.modified
    }
    /// Starting sector ID as stored in the directory record.
    pub fn start_sector(&self) -> u32 {
        self.start
    }
}

struct Directory {
    name: String,
    kind: u8,
    left: u32,
    right: u32,
    child: u32,
    start: u32,
    size: u64,
    clsid: [u8; 16],
    state_bits: u32,
    created: u64,
    modified: u64,
}

impl Directory {
    fn entry(&self, path: String, kind: EntryKind) -> Entry {
        Entry {
            path,
            kind,
            size: self.size,
            start: self.start,
            clsid: self.clsid,
            state_bits: self.state_bits,
            created: self.created,
            modified: self.modified,
        }
    }
}

/// Parsed, borrowed CFB container. Parsing and stream reads never trust declared sizes
/// as allocation capacities without first checking the physical input.
pub struct Cfb<'a> {
    bytes: &'a [u8],
    sector_size: usize,
    sector_count: usize,
    fat: Vec<u32>,
    mini_fat: Vec<u32>,
    entries: Vec<Entry>,
}

impl<'a> Cfb<'a> {
    /// Validate the header, sector allocation tables and reachable directory tree.
    pub fn open(bytes: &'a [u8]) -> Result<Self, Error> {
        if bytes.len() < 512 {
            return Err(Error("truncated CFB header"));
        }
        if bytes.get(..8) != Some(SIGNATURE.as_slice()) {
            return Err(Error("missing CFB signature"));
        }
        if word(bytes, 28)? != 0xfffe {
            return Err(Error("unsupported byte order"));
        }
        let major = word(bytes, 26)?;
        let shift = word(bytes, 30)?;
        if !matches!((major, shift), (3, 9) | (4, 12)) || word(bytes, 32)? != 6 {
            return Err(Error("unsupported CFB version or sector size"));
        }
        let sector_size = 1usize << shift;
        if bytes.len() < sector_size {
            return Err(Error("truncated CFB header sector"));
        }
        let sector_count = (bytes.len() - sector_size) / sector_size;
        let per = sector_size / 4;
        let fat_count = usize::try_from(dword(bytes, 44)?).map_err(|_| Error("FAT count overflow"))?;
        let difat_count = usize::try_from(dword(bytes, 72)?).map_err(|_| Error("DIFAT count overflow"))?;
        if fat_count == 0 || fat_count > sector_count.min(sector_count.div_ceil(per).saturating_add(1)) || difat_count > sector_count {
            return Err(Error("FAT/DIFAT count exceeds file"));
        }
        if dword(bytes, 56)? != 4096 {
            return Err(Error("unsupported mini-stream cutoff"));
        }
        let mut fat_sectors = Vec::new();
        for index in 0..109 {
            let id = dword(bytes, 76 + index * 4)?;
            if id != FREE {
                if id >= MAX_SECTOR {
                    return Err(Error("invalid FAT sector marker"));
                }
                fat_sectors.push(id);
            }
        }
        let mut next = dword(bytes, 68)?;
        let mut visited_difat = HashSet::new();
        for _ in 0..difat_count {
            if next >= MAX_SECTOR || !visited_difat.insert(next) {
                return Err(Error("DIFAT chain loops or ends early"));
            }
            let sector = raw_sector(bytes, sector_size, sector_count, next)?;
            for index in 0..per - 1 {
                let id = dword(sector, index * 4)?;
                if id != FREE {
                    if id >= MAX_SECTOR {
                        return Err(Error("invalid DIFAT entry"));
                    }
                    fat_sectors.push(id);
                    if fat_sectors.len() > fat_count {
                        return Err(Error("FAT list exceeds declared count"));
                    }
                }
            }
            next = dword(sector, (per - 1) * 4)?;
        }
        if next != END || fat_sectors.len() != fat_count {
            return Err(Error("inconsistent DIFAT chain or FAT count"));
        }
        let mut seen_fat = HashSet::new();
        let mut fat = Vec::new();
        for id in fat_sectors {
            if !seen_fat.insert(id) || visited_difat.contains(&id) {
                return Err(Error("duplicate FAT/DIFAT sector"));
            }
            let sector = raw_sector(bytes, sector_size, sector_count, id)?;
            for chunk in sector.as_chunks::<4>().0 {
                fat.push(u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
            }
        }
        let mut cfb = Self { bytes, sector_size, sector_count, fat, mini_fat: Vec::new(), entries: Vec::new() };
        let directory_start = dword(bytes, 48)?;
        let directory_sectors = cfb.chain(directory_start, &cfb.fat, None, MAX_DIRECTORY_SECTORS, false)?;
        let mut directory = Vec::new();
        for id in directory_sectors {
            let sector = cfb.sector(id)?;
            for raw in sector.as_chunks::<128>().0 {
                directory.push(parse_directory(raw, major)?);
            }
        }
        let root = directory.first().ok_or(Error("missing root directory"))?;
        if root.kind != 5 {
            return Err(Error("missing root directory"));
        }
        cfb.entries.push(root.entry(String::new(), EntryKind::Root));
        let mut seen = vec![false; directory.len()];
        seen[0] = true;
        let mut stack = vec![(root.child, String::new(), false)];
        while let Some((id, parent, expanded)) = stack.pop() {
            if id == FREE {
                continue;
            }
            let index = usize::try_from(id).map_err(|_| Error("directory index overflow"))?;
            let item = directory.get(index).ok_or(Error("directory index out of range"))?;
            if !expanded {
                if seen[index] || item.kind != 1 && item.kind != 2 {
                    return Err(Error("directory loop or invalid entry"));
                }
                seen[index] = true;
                stack.push((item.right, parent.clone(), false));
                stack.push((id, parent.clone(), true));
                stack.push((item.left, parent, false));
            } else {
                let path = if parent.is_empty() { item.name.clone() } else { format!("{parent}/{}", item.name) };
                if item.name.is_empty() || item.name.contains('/') || item.name.contains('\\') {
                    return Err(Error("invalid directory name"));
                }
                let kind = if item.kind == 1 { EntryKind::Storage } else { EntryKind::Stream };
                cfb.entries.push(item.entry(path.clone(), kind));
                if kind == EntryKind::Storage {
                    stack.push((item.child, path, false));
                } else if item.child != FREE {
                    return Err(Error("stream has children"));
                }
            }
        }
        let mini_count = usize::try_from(dword(bytes, 64)?).map_err(|_| Error("mini FAT count overflow"))?;
        if mini_count > sector_count || mini_count.saturating_mul(sector_size) > MAX_STREAM {
            return Err(Error("mini FAT exceeds file"));
        }
        let mini_start = dword(bytes, 60)?;
        let mini_ids = cfb.chain(mini_start, &cfb.fat, Some(mini_count), sector_count, false)?;
        for id in mini_ids {
            let sector = cfb.sector(id)?;
            for chunk in sector.as_chunks::<4>().0 {
                cfb.mini_fat.push(u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
            }
        }
        Ok(cfb)
    }

    fn sector(&self, id: u32) -> Result<&'a [u8], Error> {
        raw_sector(self.bytes, self.sector_size, self.sector_count, id)
    }

    fn chain(&self, start: u32, table: &[u32], needed: Option<usize>, cap: usize, allow_short_final: bool) -> Result<Vec<u32>, Error> {
        let mut ids = Vec::new();
        let mut seen = HashSet::new();
        let mut id = start;
        let file_cap = self.sector_count.saturating_add(usize::from(allow_short_final));
        let maximum = cap.min(table.len()).min(file_cap.max(1));
        loop {
            if needed == Some(ids.len()) {
                return Ok(ids);
            }
            if id == END {
                return if needed.is_none() { Ok(ids) } else { Err(Error("sector chain ends early")) };
            }
            if id >= MAX_SECTOR || !seen.insert(id) {
                return Err(Error("sector chain loops or uses marker"));
            }
            if ids.len() >= maximum {
                return Err(Error("sector chain exceeds file or limit"));
            }
            let next = *table.get(id as usize).ok_or(Error("sector missing from FAT"))?;
            if !(allow_short_final && needed == Some(ids.len() + 1) && next == END) {
                self.sector(id)?;
            }
            ids.push(id);
            id = next;
        }
    }

    fn read_regular(&self, start: u32, length: usize) -> Result<Vec<u8>, Error> {
        if length > MAX_STREAM || length > self.bytes.len().saturating_sub(self.sector_size) {
            return Err(Error("stream exceeds file or size limit"));
        }
        let needed = length.div_ceil(self.sector_size);
        let cap = self.sector_count.saturating_add(1);
        let ids = self.chain(start, &self.fat, Some(needed), cap, true)?;
        let mut data = Vec::new();
        for (index, id) in ids.into_iter().enumerate() {
            let sector = if index + 1 == needed { self.short_final_sector(id)? } else { self.sector(id)? };
            let count = (length - data.len()).min(self.sector_size);
            if sector.len() < count {
                return Err(Error("truncated sector"));
            }
            data.extend_from_slice(&sector[..count]);
        }
        Ok(data)
    }

    fn short_final_sector(&self, id: u32) -> Result<&'a [u8], Error> {
        if id >= MAX_SECTOR {
            return Err(Error("sector outside file"));
        }
        let offset = (id as usize + 1).checked_mul(self.sector_size).ok_or(Error("sector offset overflow"))?;
        let end = offset.checked_add(self.sector_size).ok_or(Error("sector offset overflow"))?;
        self.bytes.get(offset..end.min(self.bytes.len())).ok_or(Error("truncated sector"))
    }

    /// Root directory entry.
    pub fn root(&self) -> &Entry {
        &self.entries[0]
    }

    /// All reachable directory entries, root first, siblings in tree order.
    pub fn entries(&self) -> impl Iterator<Item = &Entry> {
        self.entries.iter()
    }

    /// Read a stream by its case-insensitive, slash-delimited path from the root.
    pub fn read_stream(&self, path: &str) -> Result<Vec<u8>, Error> {
        let path = path.replace('\\', "/");
        let entry = self.entries.iter().find(|entry| entry.path.eq_ignore_ascii_case(&path)).ok_or(Error("stream not found"))?;
        if entry.kind != EntryKind::Stream {
            return Err(Error("not a stream"));
        }
        let length = usize::try_from(entry.size).map_err(|_| Error("stream too large"))?;
        if length > MAX_STREAM {
            return Err(Error("stream too large"));
        }
        if length >= 4096 {
            return self.read_regular(entry.start, length);
        }
        let root = self.root();
        let root_len = usize::try_from(root.size).map_err(|_| Error("mini stream too large"))?;
        if root_len > MAX_STREAM {
            return Err(Error("mini stream too large"));
        }
        if length > root_len {
            return Err(Error("mini stream exceeds root stream"));
        }
        let mut sectors = Vec::new();
        let mut seen = HashSet::new();
        let mut id = entry.start;
        let mut remaining = length;
        let mut needed_end = 0;
        while remaining > 0 {
            if id >= MAX_SECTOR || !seen.insert(id) || seen.len() > self.mini_fat.len() {
                return Err(Error("mini FAT cycle or marker"));
            }
            let offset =
                usize::try_from(id).map_err(|_| Error("mini sector index overflow"))?.checked_mul(64).ok_or(Error("mini sector offset overflow"))?;
            let count = remaining.min(64);
            let end = offset.checked_add(count).ok_or(Error("mini sector offset overflow"))?;
            needed_end = needed_end.max(end);
            sectors.push((offset, count));
            id = *self.mini_fat.get(id as usize).ok_or(Error("mini sector missing from mini FAT"))?;
            remaining -= count;
        }
        let container = self.read_regular(root.start, root_len.min(needed_end))?;
        let mut output = Vec::new();
        for (offset, count) in sectors {
            let sector = container.get(offset..offset + count).ok_or(Error("mini stream overflow"))?;
            output.extend_from_slice(sector);
        }
        Ok(output)
    }
}

fn raw_sector(bytes: &[u8], sector_size: usize, count: usize, id: u32) -> Result<&[u8], Error> {
    if id >= MAX_SECTOR || id as usize >= count {
        return Err(Error("sector outside file"));
    }
    let offset = (id as usize + 1).checked_mul(sector_size).ok_or(Error("sector offset overflow"))?;
    bytes.get(offset..offset + sector_size).ok_or(Error("truncated sector"))
}

fn parse_directory(raw: &[u8], major: u16) -> Result<Directory, Error> {
    let kind = raw[66];
    if kind == 0 {
        return Ok(Directory {
            name: String::new(),
            kind,
            left: FREE,
            right: FREE,
            child: FREE,
            start: END,
            size: 0,
            clsid: [0; 16],
            state_bits: 0,
            created: 0,
            modified: 0,
        });
    }
    let length = usize::from(word(raw, 64)?);
    if !(2..=64).contains(&length) || length % 2 != 0 || word(raw, length - 2)? != 0 {
        return Err(Error("invalid directory name length"));
    }
    let units: Result<Vec<u16>, Error> = (0..length - 2).step_by(2).map(|index| word(raw, index)).collect();
    let name = String::from_utf16(&units?).map_err(|_| Error("invalid directory UTF-16"))?;
    let size = u64::from(dword(raw, 120)?) | if major == 4 { u64::from(dword(raw, 124)?) << 32 } else { 0 };
    let clsid = raw.get(80..96).ok_or(Error("truncated CFB"))?.try_into().map_err(|_| Error("truncated CFB"))?;
    let created = u64::from(dword(raw, 100)?) | (u64::from(dword(raw, 104)?) << 32);
    let modified = u64::from(dword(raw, 108)?) | (u64::from(dword(raw, 112)?) << 32);
    Ok(Directory {
        name,
        kind,
        left: dword(raw, 68)?,
        right: dword(raw, 72)?,
        child: dword(raw, 76)?,
        start: dword(raw, 116)?,
        size,
        clsid,
        state_bits: dword(raw, 96)?,
        created,
        modified,
    })
}
