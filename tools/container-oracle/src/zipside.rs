//! The zip scenarios: per app, an old side and a new side producing the same observation type.

use std::collections::BTreeSet;
use std::io::{Cursor, Read};

use craft_compare::Verdict;

pub type Entries = Vec<(String, Vec<u8>)>;

/// What a side did with the input: `Err` is the side's own rejection, with its message.
pub type Res<T> = Result<T, String>;

/// Digits and spacing normalised, so messages that differ only in offsets or counts are one class.
pub fn norm(msg: &str) -> String {
    let mut out = String::new();
    let mut digits = false;
    for c in msg.chars().take(90) {
        if c.is_ascii_digit() {
            if !digits {
                out.push('#');
            }
            digits = true;
        } else {
            digits = false;
            out.push(if c.is_whitespace() { ' ' } else { c });
        }
    }
    out
}

/// Lift a judge over rejections: both reject (Within), one rejects (Exceeds, named by the message),
/// both accept (the inner judge).
pub fn lift<T>(old: &Res<T>, new: &Res<T>, inner: fn(&T, &T) -> Verdict) -> Verdict {
    match (old, new) {
        (Ok(a), Ok(b)) => inner(a, b),
        (Err(a), Err(b)) if norm(a) == norm(b) => Verdict::Within { metric: "both_reject".into(), value: 1.0 },
        (Err(_), Err(_)) => Verdict::Within { metric: "both_reject_msgs_differ".into(), value: 1.0 },
        (Ok(_), Err(b)) => Verdict::Exceeds { metric: format!("new_rejects:{}", norm(b)), value: 1.0, limit: 0.0 },
        (Err(a), Ok(_)) => Verdict::Exceeds { metric: format!("new_accepts:{}", norm(a)), value: 1.0, limit: 0.0 },
    }
}

// ------------------------------------------------------------------ independent zip reference

/// One central-directory record, parsed here (no zip crate, no shared code with either side).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Central {
    pub made_by: u16,
    pub needed: u16,
    pub flags: u16,
    pub method: u16,
    pub time: u16,
    pub date: u16,
    pub crc: u32,
    pub csize: u32,
    pub size: u32,
    pub ext_attr: u32,
    pub name: Vec<u8>,
    pub local_off: u32,
}

fn u16_at(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(at..at.checked_add(2)?)?.try_into().ok()?))
}
fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at.checked_add(4)?)?.try_into().ok()?))
}

/// Central records of `b`, or `None` when no end record / central directory can be followed.
pub fn central_records(b: &[u8]) -> Option<Vec<Central>> {
    let last = b.len().checked_sub(22)?;
    let eocd = (last.saturating_sub(0xFFFF)..=last).rev().find(|&i| u32_at(b, i) == Some(0x0605_4b50))?;
    let count = usize::from(u16_at(b, eocd + 10)?);
    let mut at = usize::try_from(u32_at(b, eocd + 16)?).ok()?;
    let mut out = Vec::new();
    for _ in 0..count {
        if u32_at(b, at)? != 0x0201_4b50 {
            return None;
        }
        let (n, x, c) = (usize::from(u16_at(b, at + 28)?), usize::from(u16_at(b, at + 30)?), usize::from(u16_at(b, at + 32)?));
        out.push(Central {
            made_by: u16_at(b, at + 4)?,
            needed: u16_at(b, at + 6)?,
            flags: u16_at(b, at + 8)?,
            method: u16_at(b, at + 10)?,
            time: u16_at(b, at + 12)?,
            date: u16_at(b, at + 14)?,
            crc: u32_at(b, at + 16)?,
            csize: u32_at(b, at + 20)?,
            size: u32_at(b, at + 24)?,
            ext_attr: u32_at(b, at + 38)?,
            local_off: u32_at(b, at + 42)?,
            name: b.get(at + 46..at + 46 + n)?.to_vec(),
        });
        at = at.checked_add(46 + n + x + c)?;
    }
    Some(out)
}

/// The independent reference reader (zip 3.x): every entry's name and bytes, in central order. An
/// entry that cannot be decoded (bad CRC, unsupported method, ...) carries its error.
pub fn reference_entries(bytes: &[u8]) -> Result<Vec<(String, Result<Vec<u8>, String>)>, String> {
    let mut archive = zip_ref::ZipArchive::new(Cursor::new(bytes)).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for i in 0..archive.len() {
        match archive.by_index(i) {
            Ok(mut f) => {
                let name = f.name().to_string();
                if f.size() > MAX_SOURCE_ENTRY {
                    out.push((name, Err("entry too large for the harness".into())));
                    continue;
                }
                let mut data = Vec::new();
                let r = f.read_to_end(&mut data).map(|_| data).map_err(|e| e.to_string());
                out.push((name, r));
            }
            Err(e) => out.push((format!("#{i}"), Err(e.to_string()))),
        }
    }
    Ok(out)
}

/// Entries too large for the harness's own bookkeeping are not fed to the writers.
pub const MAX_SOURCE_ENTRY: u64 = 48 * 1024 * 1024;
pub const MAX_SOURCE_TOTAL: usize = 128 * 1024 * 1024;

/// The entries of a corpus archive as (name, bytes), the writers' input. Entries the reference cannot
/// decode are dropped; an archive the reference cannot open, or one over the harness cap, is `Err`.
pub fn source_entries(bytes: &[u8]) -> Result<Entries, String> {
    let mut total = 0usize;
    let mut out = Vec::new();
    for (name, data) in reference_entries(bytes).map_err(|e| format!("reference cannot open: {e}"))? {
        if let Ok(data) = data {
            total += data.len();
            if total > MAX_SOURCE_TOTAL {
                return Err("archive too large for the writer scenarios".into());
            }
            out.push((name, data));
        }
    }
    if out.is_empty() {
        return Err("no decodable entries".into());
    }
    Ok(out)
}

// ------------------------------------------------------------------ writer scenarios

#[derive(Clone, Debug, PartialEq)]
pub struct WEntry {
    pub rec: Central,
    pub data: Result<Vec<u8>, String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WriteObs {
    pub entries: Vec<WEntry>,
    pub raw: Vec<u8>,
}

/// Observe a written archive: central records plus the independent decode of every entry.
pub fn observe_archive(raw: Vec<u8>) -> Result<Res<WriteObs>, String> {
    Ok(observe(raw))
}

fn observe(raw: Vec<u8>) -> Res<WriteObs> {
    let recs = central_records(&raw).ok_or("unreadable-output: no readable central directory")?;
    let decoded = reference_entries(&raw).map_err(|e| format!("unreadable-output: {e}"))?;
    if decoded.len() != recs.len() {
        return Err("unreadable-output: entry count disagrees with the central directory".into());
    }
    let entries = recs.into_iter().zip(decoded).map(|(rec, (_, data))| WEntry { rec, data }).collect();
    Ok(WriteObs { entries, raw })
}

fn is_media(name: &str) -> bool {
    let ext = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "gif" | "emf" | "wmf" | "bmp" | "tif" | "tiff" | "wdp" | "mp4" | "mp3")
}

pub mod pdfcraft {
    use super::*;
    pub fn old(bytes: &[u8]) -> Result<Res<WriteObs>, String> {
        let mut z = crate::old_pdfcraft_zip::Zip::default();
        for (n, d) in source_entries(bytes)? {
            z.add(&n, &d, !is_media(&n));
        }
        observe_archive(z.finish())
    }
    pub fn new(bytes: &[u8]) -> Result<Res<WriteObs>, String> {
        let mut z = crate::new_pdfcraft_zip::Zip::default();
        for (n, d) in source_entries(bytes)? {
            z.add(&n, &d, !is_media(&n));
        }
        match z.try_finish() {
            Ok(b) => observe_archive(b),
            Err(e) => Ok(Err(e.to_string())),
        }
    }
}

pub mod effectcraft_write {
    use super::*;
    pub fn old(bytes: &[u8]) -> Result<Res<WriteObs>, String> {
        observe_archive(crate::old_effectcraft_zip::store(&source_entries(bytes)?))
    }
    pub fn new(bytes: &[u8]) -> Result<Res<WriteObs>, String> {
        let src = source_entries(bytes)?;
        let out = crate::new_effectcraft_zip::store(&src);
        // The new `store` reports failure as an empty archive (documented in the swap's comment).
        if out.is_empty() && !src.is_empty() {
            return Ok(Err("store returned an empty archive".into()));
        }
        observe_archive(out)
    }
}

pub mod vectorcraft_write {
    use super::*;
    pub const TIME: (u16, u16) = (0x6000, 0x5821);
    pub fn old_store(bytes: &[u8]) -> Result<Res<WriteObs>, String> {
        let src = source_entries(bytes)?;
        match crate::old_vectorcraft_zip::store(&src) {
            Ok(b) => observe_archive(b),
            Err(e) => Ok(Err(e)),
        }
    }
    pub fn new_store(bytes: &[u8]) -> Result<Res<WriteObs>, String> {
        let src = source_entries(bytes)?;
        match crate::new_vectorcraft_zip::store(&src) {
            Ok(b) => observe_archive(b),
            Err(e) => Ok(Err(e)),
        }
    }
    pub fn old_deflate(bytes: &[u8]) -> Result<Res<WriteObs>, String> {
        let src = source_entries(bytes)?;
        match crate::old_vectorcraft_zip::deflate(&src, TIME, || false) {
            Ok(b) => observe_archive(b),
            Err(e) => Ok(Err(e)),
        }
    }
    pub fn new_deflate(bytes: &[u8]) -> Result<Res<WriteObs>, String> {
        let src = source_entries(bytes)?;
        match crate::new_vectorcraft_zip::deflate(&src, TIME, || false) {
            Ok(b) => observe_archive(b),
            Err(e) => Ok(Err(e)),
        }
    }
}

/// Judge two written archives. Metric = the sorted `+`-joined classes of everything that differs;
/// `archive_bytes` alone (same entries, same decoded bytes, only compressed bytes/sizes) is Within.
pub fn judge_write(old: &WriteObs, new: &WriteObs) -> Verdict {
    let mut classes = BTreeSet::new();
    let (on, nn): (Vec<_>, Vec<_>) = (old.entries.iter().map(|e| &e.rec.name).collect(), new.entries.iter().map(|e| &e.rec.name).collect());
    if on != nn {
        let (a, b): (BTreeSet<_>, BTreeSet<_>) = (on.iter().collect(), nn.iter().collect());
        classes.insert(if a != b || on.len() != nn.len() { "names" } else { "order" });
    }
    let mut differing = 0usize;
    let mut compressed_only = false;
    for (o, n) in old.entries.iter().zip(&new.entries) {
        let before = classes.len();
        let (a, b) = (&o.rec, &n.rec);
        if o.data != n.data {
            classes.insert("bytes");
        }
        if a.crc != b.crc {
            classes.insert("crc");
        }
        if a.size != b.size {
            classes.insert("size");
        }
        if a.method != b.method {
            classes.insert("method");
        }
        if (a.flags ^ b.flags) & 0x0800 != 0 {
            classes.insert("utf8_flag");
        }
        if (a.flags ^ b.flags) & !0x0800 != 0 {
            classes.insert("flags_other");
        }
        if (a.time, a.date) != (b.time, b.date) {
            classes.insert("dos_time");
        }
        if a.ext_attr != b.ext_attr {
            classes.insert("ext_attr");
        }
        if (a.made_by, a.needed) != (b.made_by, b.needed) {
            classes.insert("version");
        }
        if a.csize != b.csize {
            compressed_only = true;
        }
        differing += usize::from(classes.len() != before || a != b);
    }
    if classes.is_empty() {
        if old.raw == new.raw {
            return Verdict::Identical;
        }
        return Verdict::Within { metric: if compressed_only { "archive_bytes_compressed" } else { "archive_bytes" }.into(), value: 1.0 };
    }
    Verdict::Exceeds { metric: classes.into_iter().collect::<Vec<_>>().join("+"), value: differing.max(1) as f64, limit: 0.0 }
}

// ------------------------------------------------------------------ reader scenarios

fn variant(e: &dyn std::fmt::Debug) -> String {
    let s = format!("{e:?}");
    s.split(['(', ' ']).next().unwrap_or("").to_string()
}

/// Names a reader is asked for: the reference's names plus fixed probes.
pub fn probe_names(bytes: &[u8]) -> Vec<String> {
    let mut v: Vec<String> = central_records(bytes).map(|r| r.iter().map(|c| String::from_utf8_lossy(&c.name).into_owned()).collect()).unwrap_or_default();
    // Bounded: an archive of 65,534 names must not turn the scenario quadratic (see the per-call cost
    // measurement in docs/verification/container-oracle.md for that).
    if v.len() > 64 {
        let tail = v.split_off(v.len() - 24);
        v.truncate(40);
        v.extend(tail);
    }
    for p in ["[Content_Types].xml", "mimetype", "manifest.json", "a.txt", "x", "A/B"] {
        v.push(p.to_string());
    }
    let mut seen = BTreeSet::new();
    v.retain(|n| seen.insert(n.clone()));
    v
}

#[derive(Clone, Debug, PartialEq)]
pub struct VcRead {
    /// name, contains(name), contains(upper-cased name), read result (error = variant name)
    pub entries: Vec<(String, bool, bool, Result<Option<Vec<u8>>, String>)>,
}

macro_rules! vc_reader {
    ($fn:ident, $m:ident) => {
        pub fn $fn(bytes: &[u8]) -> Result<Res<VcRead>, String> {
            let a = match crate::$m::Archive::open(bytes, 65534) {
                Ok(a) => a,
                Err(e) => return Ok(Err(variant(&e))),
            };
            let mut entries = Vec::new();
            for n in probe_names(bytes) {
                let up = n.to_ascii_uppercase();
                let r = a.read(&n, 256 * 1024 * 1024).map_err(|e| variant(&e));
                entries.push((n.clone(), a.contains(&n), a.contains(&up), r));
            }
            Ok(Ok(VcRead { entries }))
        }
    };
}
pub mod vectorcraft_read {
    use super::*;
    vc_reader!(old, old_vectorcraft_zip);
    vc_reader!(new, new_vectorcraft_zip);
}

pub fn judge_vc(old: &VcRead, new: &VcRead) -> Verdict {
    let mut classes = BTreeSet::new();
    let mut n = 0usize;
    for (o, w) in old.entries.iter().zip(&new.entries) {
        if o == w {
            continue;
        }
        n += 1;
        if o.1 != w.1 || o.2 != w.2 {
            classes.insert("contains");
        }
        match (&o.3, &w.3) {
            (Ok(a), Ok(b)) if a != b => {
                classes.insert("bytes");
            }
            (Ok(_), Err(_)) => {
                classes.insert("old_ok_new_err");
            }
            (Err(_), Ok(_)) => {
                classes.insert("old_err_new_ok");
            }
            (Err(a), Err(b)) if a != b => {
                classes.insert("err_variant");
            }
            _ => {}
        }
    }
    if classes.is_empty() {
        return Verdict::Identical;
    }
    Verdict::Exceeds { metric: classes.into_iter().collect::<Vec<_>>().join("+"), value: n as f64, limit: 0.0 }
}

pub mod effectcraft_read {
    use super::*;
    pub fn old(bytes: &[u8]) -> Result<Res<Entries>, String> {
        Ok(Ok(crate::old_effectcraft_zip::read_stored(bytes)))
    }
    pub fn new(bytes: &[u8]) -> Result<Res<Entries>, String> {
        Ok(Ok(crate::new_effectcraft_zip::read_stored(bytes)))
    }
}

pub fn judge_entries(old: &Entries, new: &Entries) -> Verdict {
    if old == new {
        return Verdict::Identical;
    }
    let mut classes = BTreeSet::new();
    if !old.is_empty() && new.is_empty() {
        classes.insert("new_rejects_archive");
    } else {
        let (om, nm): (std::collections::BTreeMap<_, _>, std::collections::BTreeMap<_, _>) =
            (old.iter().cloned().collect(), new.iter().cloned().collect());
        if om.iter().any(|(k, v)| nm.get(k).is_some_and(|w| w != v)) {
            classes.insert("bytes");
        }
        if om.keys().any(|k| !nm.contains_key(k)) {
            classes.insert("old_entries_lost");
        }
        if nm.keys().any(|k| !om.contains_key(k)) {
            classes.insert("new_adds_entries");
        }
        if classes.is_empty() {
            classes.insert("order_or_duplicates");
        }
    }
    Verdict::Exceeds { metric: classes.into_iter().collect::<Vec<_>>().join("+"), value: 1.0, limit: 0.0 }
}

/// Archives as the pre-swap writers produced them (what users' saved files look like), built from a
/// corpus archive's entries: the input of the "legacy" reader scenarios.
pub mod legacy {
    use super::*;
    pub fn effectcraft_store(bytes: &[u8]) -> Result<Vec<u8>, String> {
        Ok(crate::old_effectcraft_zip::store(&source_entries(bytes)?))
    }
    pub fn vectorcraft_store(bytes: &[u8]) -> Result<Vec<u8>, String> {
        crate::old_vectorcraft_zip::store(&source_entries(bytes)?)
    }
    pub fn vectorcraft_deflate(bytes: &[u8]) -> Result<Vec<u8>, String> {
        crate::old_vectorcraft_zip::deflate(&source_entries(bytes)?, (0, 0x21), || false)
    }
}

pub fn judge_write_l(o: &Res<WriteObs>, n: &Res<WriteObs>) -> Verdict {
    lift(o, n, judge_write)
}
pub fn judge_vc_l(o: &Res<VcRead>, n: &Res<VcRead>) -> Verdict {
    lift(o, n, judge_vc)
}
pub fn judge_entries_l(o: &Res<Entries>, n: &Res<Entries>) -> Verdict {
    lift(o, n, judge_entries)
}
