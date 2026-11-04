//! Hostile synthetic inputs, generated in code (nothing is checked in): damaged and malicious ZIP
//! archives and compound files. Every case is fed to the same scenarios as the corpus files.

use std::io::{Cursor, Write};

pub type Case = (String, Vec<u8>);

// ------------------------------------------------------------------------ raw ZIP builder

pub struct Raw {
    pub name: Vec<u8>,
    /// Uncompressed content.
    pub data: Vec<u8>,
    pub method: u16,
    pub flags: u16,
    pub crc: Option<u32>,
    pub csize: Option<u32>,
    pub size: Option<u32>,
    pub ext_attr: u32,
    pub made_by: u16,
    pub local_name: Option<Vec<u8>>,
    pub local_extra: Vec<u8>,
    /// Pre-compressed body, when the case needs a deflate stream that disagrees with `data`.
    pub body: Option<Vec<u8>>,
    pub descriptor: bool,
}

impl Raw {
    pub fn stored(name: &str, data: &[u8]) -> Self {
        Self { name: name.as_bytes().to_vec(), data: data.to_vec(), method: 0, flags: 0, crc: None, csize: None, size: None, ext_attr: 0, made_by: 20, local_name: None, local_extra: vec![], body: None, descriptor: false }
    }
    pub fn deflated(name: &str, data: &[u8]) -> Self {
        Self { method: 8, ..Self::stored(name, data) }
    }
}

pub fn crc32(d: &[u8]) -> u32 {
    let mut c = flate2::Crc::new();
    c.update(d);
    c.sum()
}

pub fn deflate(d: &[u8]) -> Vec<u8> {
    let mut e = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
    e.write_all(d).unwrap();
    e.finish().unwrap()
}

#[derive(Default, Clone)]
pub struct Tail {
    pub comment: Vec<u8>,
    pub count: Option<u16>,
    pub dir_offset: Option<u32>,
    pub dir_size: Option<u32>,
    pub disk: u16,
    pub dir_disk: u16,
}

pub fn build(entries: &[Raw], tail: &Tail) -> Vec<u8> {
    let (mut out, mut central) = (Vec::new(), Vec::new());
    for e in entries {
        let body = e.body.clone().unwrap_or_else(|| if e.method == 8 { deflate(&e.data) } else { e.data.clone() });
        let crc = e.crc.unwrap_or_else(|| crc32(&e.data));
        let csize = e.csize.unwrap_or(body.len() as u32);
        let size = e.size.unwrap_or(e.data.len() as u32);
        let offset = out.len() as u32;
        let ln = e.local_name.clone().unwrap_or_else(|| e.name.clone());
        out.extend(0x0403_4b50u32.to_le_bytes());
        out.extend(20u16.to_le_bytes());
        out.extend((e.flags | if e.descriptor { 8 } else { 0 }).to_le_bytes());
        out.extend(e.method.to_le_bytes());
        out.extend([0, 0, 0x21, 0]);
        let (lc, ls, lz) = if e.descriptor { (0, 0, 0) } else { (crc, csize, size) };
        out.extend(lc.to_le_bytes());
        out.extend(ls.to_le_bytes());
        out.extend(lz.to_le_bytes());
        out.extend((ln.len() as u16).to_le_bytes());
        out.extend((e.local_extra.len() as u16).to_le_bytes());
        out.extend(&ln);
        out.extend(&e.local_extra);
        out.extend(&body);
        if e.descriptor {
            out.extend(0x0807_4b50u32.to_le_bytes());
            out.extend(crc.to_le_bytes());
            out.extend(csize.to_le_bytes());
            out.extend(size.to_le_bytes());
        }
        central.extend(0x0201_4b50u32.to_le_bytes());
        central.extend(e.made_by.to_le_bytes());
        central.extend(20u16.to_le_bytes());
        central.extend((e.flags | if e.descriptor { 8 } else { 0 }).to_le_bytes());
        central.extend(e.method.to_le_bytes());
        central.extend([0, 0, 0x21, 0]);
        central.extend(crc.to_le_bytes());
        central.extend(csize.to_le_bytes());
        central.extend(size.to_le_bytes());
        central.extend((e.name.len() as u16).to_le_bytes());
        central.extend([0u8; 2]);
        central.extend([0u8; 2]);
        central.extend([0u8; 2]);
        central.extend([0u8; 2]);
        central.extend(e.ext_attr.to_le_bytes());
        central.extend(offset.to_le_bytes());
        central.extend(&e.name);
    }
    let dir_at = out.len() as u32;
    out.extend(&central);
    out.extend(0x0605_4b50u32.to_le_bytes());
    out.extend(tail.disk.to_le_bytes());
    out.extend(tail.dir_disk.to_le_bytes());
    let n = tail.count.unwrap_or(entries.len() as u16);
    out.extend(n.to_le_bytes());
    out.extend(n.to_le_bytes());
    out.extend(tail.dir_size.unwrap_or(central.len() as u32).to_le_bytes());
    out.extend(tail.dir_offset.unwrap_or(dir_at).to_le_bytes());
    out.extend((tail.comment.len() as u16).to_le_bytes());
    out.extend(&tail.comment);
    out
}

fn one(e: Raw) -> Vec<u8> {
    build(&[e], &Tail::default())
}

/// A small valid archive with a mix of stored and deflated entries, the base of most cases.
pub fn base_zip() -> Vec<u8> {
    build(
        &[
            Raw::stored("a.txt", b"hello hello hello"),
            Raw::deflated("[Content_Types].xml", &b"<Types>".repeat(40)),
            Raw::deflated("dir/b.txt", &b"0123456789".repeat(30)),
        ],
        &Tail::default(),
    )
}

/// A `.pcraft`-shaped base (craft-doc needs a manifest); the corpus-independent part of the doc cases.
pub fn pcraft_base() -> Vec<u8> {
    crate::docside::sample_pcraft()
}

fn zero_deflate(total: usize) -> Vec<u8> {
    let mut e = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::fast());
    let chunk = vec![0u8; 1 << 20];
    let mut left = total;
    while left > 0 {
        let n = left.min(chunk.len());
        e.write_all(&chunk[..n]).unwrap();
        left -= n;
    }
    e.finish().unwrap()
}

/// Hostile and damaged ZIP archives built around `base` (a valid archive) plus standalone ones.
/// `prefix` names the family in the case name.
pub fn zip_cases(base: &[u8], with_bombs: bool) -> Vec<Case> {
    let mut v: Vec<Case> = Vec::new();
    let mut add = |n: &str, b: Vec<u8>| v.push((n.to_string(), b));
    add("zip/valid-base", base.to_vec());
    add("zip/empty-archive", build(&[], &Tail::default()));
    add("zip/zero-bytes", vec![]);
    add("zip/not-a-zip", b"this is not a zip archive at all, just text".to_vec());
    add("zip/eocd-only", build(&[], &Tail::default()));
    add("zip/local-signature-only", b"PK\x03\x04".to_vec());
    // Truncations.
    let len = base.len();
    for cut in [1usize, 4, 10, 21, 22, 30, len / 4, len / 2, len - 60, len - 23, len - 22, len - 21, len - 2, len - 1] {
        if cut < len {
            add(&format!("zip/truncated-{cut}"), base[..cut].to_vec());
        }
    }
    // CRC and size lies.
    let mut e = Raw::stored("a.txt", b"hello world");
    e.crc = Some(0xDEAD_BEEF);
    add("zip/bad-crc-stored", one(e));
    let mut e = Raw::deflated("a.txt", &b"hello world ".repeat(20));
    e.crc = Some(0xDEAD_BEEF);
    add("zip/bad-crc-deflated", one(e));
    let mut e = Raw::deflated("a.txt", &b"x".repeat(1000));
    e.size = Some(5);
    add("zip/size-understated-deflated", one(e));
    let mut e = Raw::deflated("a.txt", &b"x".repeat(1000));
    e.size = Some(5000);
    add("zip/size-overstated-deflated", one(e));
    let mut e = Raw::stored("a.txt", b"stored body");
    e.size = Some(3);
    add("zip/size-understated-stored", one(e));
    let mut e = Raw::stored("a.txt", b"stored body");
    e.csize = Some(3);
    add("zip/csize-understated-stored", one(e));
    let mut e = Raw::stored("a.txt", b"stored body");
    e.csize = Some(900);
    e.size = Some(900);
    add("zip/sizes-past-eof", one(e));
    // Names.
    add("zip/duplicate-names", build(&[Raw::stored("a.txt", b"first"), Raw::stored("a.txt", b"second")], &Tail::default()));
    add("zip/duplicate-names-case", build(&[Raw::stored("a.txt", b"first"), Raw::stored("A.TXT", b"second")], &Tail::default()));
    for (tag, name) in [
        ("dotdot", "../evil.txt"),
        ("dotdot-nested", "a/../../evil.txt"),
        ("absolute", "/etc/passwd"),
        ("backslash", "dir\\file.txt"),
        ("backslash-dotdot", "..\\evil.txt"),
        ("drive", "C:\\x.txt"),
        ("drive-relative", "c:x.txt"),
        ("control-char", "a\u{1}b.txt"),
        ("dot-segment", "./a.txt"),
        ("empty-segment", "a//b.txt"),
        ("directory-entry", "dir/"),
        ("trailing-dot-dot", "a/.."),
        ("unicode", "caf\u{e9}/\u{4e2d}\u{6587}.txt"),
    ] {
        add(&format!("zip/name-{tag}"), build(&[Raw::stored(name, b"payload"), Raw::stored("ok.txt", b"ok")], &Tail::default()));
    }
    add("zip/name-empty", build(&[Raw::stored("", b"payload"), Raw::stored("ok.txt", b"ok")], &Tail::default()));
    let mut e = Raw::stored("lat\u{e9}.txt", b"payload");
    e.name = vec![b'l', b'a', b't', 0xE9, b'.', b't', b'x', b't'];
    add("zip/name-non-utf8", one(e));
    add("zip/name-very-long", one(Raw::stored(&"n".repeat(300), b"payload")));
    // Entry kinds and features.
    let mut e = Raw::stored("link", b"/etc/passwd");
    e.made_by = (3 << 8) | 20;
    e.ext_attr = 0o120777 << 16;
    add("zip/symlink", one(e));
    let mut e = Raw::stored("a.txt", b"payload");
    e.flags = 1;
    add("zip/encrypted-flag", one(e));
    for m in [1u16, 6, 9, 12, 14, 93, 95, 99] {
        let mut e = Raw::stored("a.txt", b"payload");
        e.method = m;
        add(&format!("zip/method-{m}"), one(e));
    }
    let mut e = Raw::stored("a.txt", b"payload");
    e.csize = Some(0xFFFF_FFFF);
    e.size = Some(0xFFFF_FFFF);
    add("zip/zip64-sentinel-sizes", one(e));
    add("zip/count-sentinel-ffff", build(&[Raw::stored("a.txt", b"x")], &Tail { count: Some(0xFFFF), ..Tail::default() }));
    let mut e = Raw::deflated("a.txt", &b"descriptor body ".repeat(20));
    e.descriptor = true;
    add("zip/data-descriptor", one(e));
    let mut e = Raw::stored("a.txt", b"payload");
    e.local_extra = vec![0xCA, 0xFE, 4, 0, 1, 2, 3, 4];
    add("zip/local-extra-field", one(e));
    let mut e = Raw::stored("a.txt", b"payload");
    e.local_name = Some(b"other.txt".to_vec());
    add("zip/local-name-mismatch", one(e));
    // Envelope.
    let mut x = b"MZ-self-extractor-stub".repeat(3);
    x.extend_from_slice(base);
    add("zip/prefix-junk", x);
    let mut x = base.to_vec();
    x.extend(b"trailing garbage after the end record");
    add("zip/trailing-garbage", x);
    add("zip/eocd-comment-with-signature", {
        let mut c = b"fake: ".to_vec();
        c.extend(0x0605_4b50u32.to_le_bytes());
        c.extend([0u8; 30]);
        build(&[Raw::stored("a.txt", b"x")], &Tail { comment: c, ..Tail::default() })
    });
    add("zip/comment-length-lies", {
        let mut b = build(&[Raw::stored("a.txt", b"x")], &Tail::default());
        let n = b.len();
        b[n - 2..].copy_from_slice(&40u16.to_le_bytes());
        b
    });
    add("zip/dir-offset-past-eof", build(&[Raw::stored("a.txt", b"x")], &Tail { dir_offset: Some(0xFFFF_0000), ..Tail::default() }));
    add("zip/dir-offset-zero", build(&[Raw::stored("a.txt", b"x")], &Tail { dir_offset: Some(0), ..Tail::default() }));
    add("zip/dir-size-huge", build(&[Raw::stored("a.txt", b"x")], &Tail { dir_size: Some(0x7FFF_FFFF), ..Tail::default() }));
    add("zip/count-too-big", build(&[Raw::stored("a.txt", b"x")], &Tail { count: Some(50), ..Tail::default() }));
    add("zip/count-too-small", build(&[Raw::stored("a.txt", b"x"), Raw::stored("b.txt", b"y")], &Tail { count: Some(1), ..Tail::default() }));
    add("zip/multi-disk", build(&[Raw::stored("a.txt", b"x")], &Tail { disk: 1, dir_disk: 1, ..Tail::default() }));
    add("zip/local-signature-corrupt", {
        let mut b = base.to_vec();
        b[0] = b'X';
        b
    });
    add("zip/many-entries-65534", build(&(0..65534).map(|i| Raw::stored(&format!("{i}"), b"")).collect::<Vec<_>>(), &Tail::default()));
    add("zip/many-entries-65535", build(&(0..65535).map(|i| Raw::stored(&format!("{i}"), b"")).collect::<Vec<_>>(), &Tail::default()));
    add("zip/many-entries-wrapped-count", build(&(0..66000).map(|i| Raw::stored(&format!("{i}"), b"")).collect::<Vec<_>>(), &Tail::default()));
    if with_bombs {
        let bomb = 300usize << 20;
        let z = zero_deflate(bomb);
        let mut e = Raw::deflated("bomb.bin", &[]);
        e.body = Some(z.clone());
        e.crc = Some(0);
        e.size = Some(bomb as u32);
        add("zip/bomb-honest-300mib", one(e));
        let mut e = Raw::deflated("bomb.bin", &[]);
        e.body = Some(z.clone());
        e.crc = Some(0);
        e.size = Some(16);
        add("zip/bomb-understated-size", one(e));
        let mut es = vec![];
        for i in 0..6 {
            let mut e = Raw::deflated(&format!("part{i}.bin"), &[]);
            e.body = Some(zero_deflate(60 << 20));
            e.crc = Some(0);
            e.size = Some(60 << 20);
            es.push(e);
        }
        add("zip/bomb-many-parts-360mib", build(&es, &Tail::default()));
    }
    // Single-byte corruption of the base at every offset.
    for at in 0..base.len() {
        let mut b = base.to_vec();
        b[at] ^= 0xFF;
        add(&format!("zip-flip/{at:04}"), b);
    }
    // Header-field sweep: every 4-byte window set to extreme values (central and local headers).
    for at in (0..base.len().saturating_sub(4)).step_by(2) {
        for (tag, val) in [("zero", 0u32), ("max", u32::MAX), ("big", 0x7FFF_FFF0)] {
            let mut b = base.to_vec();
            b[at..at + 4].copy_from_slice(&val.to_le_bytes());
            add(&format!("zip-field/{at:04}-{tag}"), b);
        }
    }
    v
}

// ------------------------------------------------------------------------ compound files

fn sample(version: cfb_ref::Version, big: usize) -> Vec<u8> {
    let mut cf = cfb_ref::CompoundFile::create_with_version(version, Cursor::new(Vec::new())).unwrap();
    cf.create_storage("/Storage").unwrap();
    let streams: Vec<(String, Vec<u8>)> = vec![
        ("/Workbook".into(), (0..big).map(|i| (i % 251) as u8).collect()),
        ("/\u{5}SummaryInformation".into(), (0..300).map(|i| (i * 7 % 253) as u8).collect()),
        ("/EncryptionInfo".into(), vec![4, 0, 4, 0, 0x40, 0, 0, 0]),
        ("/EncryptedPackage".into(), (0..5000).map(|i| (i % 249) as u8).collect()),
        ("/Storage/Inner".into(), b"inner stream data ".repeat(50)),
        ("/Tiny".into(), b"abc".to_vec()),
    ];
    for (p, d) in &streams {
        let mut s = cf.create_stream(p).unwrap();
        s.write_all(d).unwrap();
    }
    cf.flush().unwrap();
    cf.into_inner().into_inner()
}

/// A valid compound file with many streams (several FAT sectors, one DIFAT-free).
fn big_sample() -> Vec<u8> {
    let mut cf = cfb_ref::CompoundFile::create(Cursor::new(Vec::new())).unwrap();
    for i in 0..40 {
        let mut s = cf.create_stream(format!("/S{i:02}")).unwrap();
        s.write_all(&vec![(i % 200) as u8; 1000 + i * 2000]).unwrap();
    }
    cf.flush().unwrap();
    cf.into_inner().into_inner()
}

fn put32(b: &mut [u8], at: usize, v: u32) {
    if let Some(d) = b.get_mut(at..at + 4) {
        d.copy_from_slice(&v.to_le_bytes());
    }
}
fn put16(b: &mut [u8], at: usize, v: u16) {
    if let Some(d) = b.get_mut(at..at + 2) {
        d.copy_from_slice(&v.to_le_bytes());
    }
}
fn get32(b: &[u8], at: usize) -> u32 {
    b.get(at..at + 4).map(|d| u32::from_le_bytes(d.try_into().unwrap())).unwrap_or(0)
}

pub fn cfb_bases() -> Vec<(String, Vec<u8>)> {
    vec![("v3".into(), sample(cfb_ref::Version::V3, 6000)), ("v4".into(), sample(cfb_ref::Version::V4, 9000)), ("v3big".into(), big_sample())]
}

/// Hostile and damaged compound files: named structural attacks, truncations and field sweeps.
pub fn cfb_cases() -> Vec<Case> {
    let mut v: Vec<Case> = Vec::new();
    let mut add = |n: &str, b: Vec<u8>| v.push((n.to_string(), b));
    let bases = cfb_bases();
    add("cfb/zero-bytes", vec![]);
    add("cfb/signature-only", crate::cfbside::MAGIC.to_vec());
    add("cfb/not-cfb", b"definitely not a compound file".repeat(40));
    add("cfb/zip-bytes", base_zip());
    for (tag, base) in &bases {
        add(&format!("cfb/{tag}/valid"), base.clone());
        let n = base.len();
        for cut in [1usize, 7, 8, 100, 511, 512, 513, 1024, 1536, n / 2, n - 513, n - 512, n - 100, n - 1] {
            if cut < n {
                add(&format!("cfb/{tag}/truncated-{cut}"), base[..cut].to_vec());
            }
        }
        // Short tail: the last sector cut (unpadded writers) at several depths.
        for cut in [1usize, 2, 17, 64, 100, 300, 511] {
            if n > 1024 + cut {
                add(&format!("cfb/{tag}/short-tail-{cut}"), base[..n - cut].to_vec());
            }
        }
        // Header fields (ABSOLUTE offsets of [MS-CFB] 2.2) set to extremes.
        for at in (0..512usize).step_by(2).filter(|a| *a + 4 <= 512) {
            for (t, val) in [("zero", 0u32), ("one", 1), ("two", 2), ("max", u32::MAX), ("eoc", 0xFFFF_FFFE), ("fat", 0xFFFF_FFFD), ("regmax", 0xFFFF_FFFA), ("big", 0x0100_0000)] {
                let mut b = base.clone();
                put32(&mut b, at, val);
                add(&format!("cfb/{tag}/header-{at:03}-{t}"), b);
            }
        }
        // Sector shifts and version lies.
        for sh in [0u16, 1, 7, 8, 10, 11, 12, 13, 16, 31, 0xFFFF] {
            let mut b = base.clone();
            put16(&mut b, 30, sh);
            add(&format!("cfb/{tag}/sector-shift-{sh}"), b);
        }
        for ver in [0u16, 1, 2, 3, 4, 5, 0xFFFF] {
            let mut b = base.clone();
            put16(&mut b, 26, ver);
            add(&format!("cfb/{tag}/major-version-{ver}"), b);
        }
        // Per-byte flips across the header sector and the first 4 KiB of everything else.
        for at in (0..n.min(512 + 4096)).step_by(1) {
            if at % 3 == 0 || at < 128 {
                let mut b = base.clone();
                b[at] ^= 0xFF;
                add(&format!("cfb/{tag}/flip-{at:05}"), b);
            }
        }
        // Directory sector: find the first directory sector and corrupt its entries.
        let sector = if get32(base, 30) as u16 == 12 || base.get(26).copied() == Some(4) { 4096usize } else { 512 };
        let dir = (get32(base, 48) as usize + 1) * sector;
        for entry in 0..4usize {
            let e = dir + entry * 128;
            if e + 128 > n {
                continue;
            }
            for (t, off, val) in [
                ("left-self", 68usize, entry as u32),
                ("right-self", 72, entry as u32),
                ("child-self", 76, entry as u32),
                ("child-far", 76, 0x7FFF_FFFF),
                ("start-far", 116, 0x00FF_FFFF),
                ("start-eoc", 116, 0xFFFF_FFFE),
                ("start-fat", 116, 0xFFFF_FFFD),
                ("size-huge", 120, 0xFFFF_FFFF),
                ("size-zero", 120, 0),
                ("size-mid", 120, 0x0010_0000),
                ("size-hi", 124, 0x0000_0100),
                ("namelen-0", 64, 0),
                ("namelen-odd", 64, 7),
                ("namelen-big", 64, 200),
                ("kind-0", 66, 0),
                ("kind-7", 66, 7),
            ] {
                let mut b = base.clone();
                if off == 64 || off == 66 {
                    put16(&mut b, e + off, val as u16);
                } else {
                    put32(&mut b, e + off, val);
                }
                add(&format!("cfb/{tag}/dir{entry}-{t}"), b);
            }
        }
        // FAT: chain cycles and bad links (FAT sector id read from the header DIFAT slot 0).
        let fat0 = (get32(base, 76) as usize + 1) * sector;
        for slot in 0..16usize {
            let at = fat0 + slot * 4;
            if at + 4 > n {
                continue;
            }
            for (t, val) in [("self", slot as u32), ("zero", 0), ("far", 0x00FF_FFFF), ("free", 0xFFFF_FFFF), ("difsect", 0xFFFF_FFFC), ("eoc", 0xFFFF_FFFE)] {
                let mut b = base.clone();
                put32(&mut b, at, val);
                add(&format!("cfb/{tag}/fat{slot:02}-{t}"), b);
            }
        }
    }
    // Named multi-sector attacks.
    let base = &bases[0].1;
    // DIFAT chain that loops on itself: header says 1 DIFAT sector which points to itself.
    {
        let mut b = base.clone();
        let extra = b.len() / 512 - 1; // index of the sector appended below
        b.extend(vec![0xFFu8; 512]);
        let at = b.len() - 4;
        put32(&mut b, at, extra as u32);
        put32(&mut b, 68, extra as u32);
        put32(&mut b, 72, 1);
        add("cfb/difat-self-loop", b);
    }
    // The wordcraft #3642 shape: a DIFAT chain of 2048 sectors listing one FAT sector over and over.
    {
        const S: usize = 512;
        const CHAIN: u32 = 2048;
        let mut f = vec![0u8; S * (CHAIN as usize + 2)];
        f[..8].copy_from_slice(&crate::cfbside::MAGIC);
        put16(&mut f, 0x18, 0x3E);
        put16(&mut f, 0x1A, 3);
        put16(&mut f, 0x1C, 0xFFFE);
        put16(&mut f, 0x1E, 9);
        put16(&mut f, 0x20, 6);
        put32(&mut f, 0x2C, u32::MAX);
        put32(&mut f, 0x38, 4096);
        put32(&mut f, 0x44, 1);
        put32(&mut f, 0x48, CHAIN);
        for d in 1..=CHAIN {
            let next = if d == CHAIN { 0xFFFF_FFFE } else { d + 1 };
            put32(&mut f, (d as usize + 2) * S - 4, next);
        }
        add("cfb/difat-repeating-fat-sector", f);
    }
    // FAT sector listed twice in the header DIFAT.
    {
        let mut b = base.clone();
        let first = get32(&b, 76);
        put32(&mut b, 80, first);
        put32(&mut b, 44, 2);
        add("cfb/fat-sector-listed-twice", b);
    }
    // Number of FAT sectors claimed far above the file.
    {
        let mut b = base.clone();
        put32(&mut b, 44, 100_000);
        add("cfb/fat-count-huge", b);
    }
    // Mini stream container shorter than the mini FAT says; mini stream cutoff 0.
    {
        let mut b = base.clone();
        let dir = (get32(&b, 48) as usize + 1) * 512;
        put32(&mut b, dir + 116, 0xFFFF_FFFE); // root has no mini stream container
        add("cfb/mini-container-missing", b);
        let mut b = base.clone();
        put32(&mut b, 56, 0);
        add("cfb/mini-cutoff-zero", b);
        let mut b = base.clone();
        put32(&mut b, 56, u32::MAX);
        add("cfb/mini-cutoff-max", b);
    }
    v
}
