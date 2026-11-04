//! The compound-file scenarios: per app, an old side and a new side producing the same observation.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Cursor, Read};

use craft_compare::Verdict;

pub const MAGIC: [u8; 8] = [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];
const CAP: u64 = 64 * 1024 * 1024;

const PROBES: [&str; 9] =
    ["Workbook", "Book", "EncryptionInfo", "EncryptedPackage", "WordDocument", "0Table", "1Table", "Data", "\u{5}SummaryInformation"];

/// Root-level stream names to ask each reader for: the independent reference's, craft-cfb's, and
/// fixed probes. Pure function of the input, so both sides are asked the same questions.
pub fn root_stream_names(bytes: &[u8]) -> Vec<String> {
    let mut set: BTreeSet<String> = PROBES.iter().map(|s| (*s).to_string()).collect();
    if let Ok(cf) = craft_cfb::Cfb::open(bytes) {
        set.extend(cf.entries().filter(|e| e.kind() == craft_cfb::EntryKind::Stream && !e.path().contains('/')).map(|e| e.path().to_string()));
    }
    if crate::old_wordcraft_cfb_guard::check(bytes).is_ok() {
        if let Ok(cf) = cfb_ref::CompoundFile::open(Cursor::new(bytes)) {
            set.extend(
                cf.walk()
                    .filter(|e| e.is_stream())
                    .filter_map(|e| e.path().to_str().map(str::to_string))
                    .filter(|p| p.matches('/').count() == 1)
                    .map(|p| p.trim_start_matches('/').to_string()),
            );
        }
    }
    set.into_iter().collect()
}

pub type StreamRead = Result<Option<Vec<u8>>, String>;
pub use crate::zipside::{Res, lift, norm};

// ---------------------------------------------------------------- gridcraft

#[derive(Clone, Debug, PartialEq)]
pub struct Streams(pub Vec<(String, StreamRead)>);

pub mod gridcraft {
    use super::*;

    pub fn sniff_old(bytes: &[u8]) -> Result<bool, String> {
        Ok(crate::old_gridcraft_cfb::is_encrypted(bytes))
    }
    pub fn sniff_new(bytes: &[u8]) -> Result<bool, String> {
        Ok(crate::new_gridcraft_cfb::is_encrypted(bytes))
    }
    /// `read_xls`'s container half: `is_xls`, then the Workbook stream.
    pub fn streams_old(bytes: &[u8]) -> Result<Res<Streams>, String> {
        let cf = match crate::old_gridcraft_cfb::Compound::open(bytes) {
            Ok(c) => c,
            Err(e) => return Ok(Err(e)),
        };
        Ok(Ok(Streams(root_stream_names(bytes).into_iter().map(|n| { let r = cf.root_stream(&n); (n, r) }).collect())))
    }
    pub fn streams_new(bytes: &[u8]) -> Result<Res<Streams>, String> {
        let cf = match craft_cfb::Cfb::open(bytes) {
            Ok(c) => c,
            Err(e) => return Ok(Err(e.0.to_string())),
        };
        Ok(Ok(Streams(
            root_stream_names(bytes)
                .into_iter()
                .map(|n| {
                    let r = match cf.read_stream(&n) {
                        Ok(d) => Ok(Some(d)),
                        Err(craft_cfb::Error("stream not found")) => Ok(None),
                        Err(e) => Err(e.0.to_string()),
                    };
                    (n, r)
                })
                .collect(),
        )))
    }
    /// gridcraft's `is_xls`.
    pub fn is_xls_old(bytes: &[u8]) -> Result<bool, String> {
        Ok(crate::old_gridcraft_cfb::Compound::open(bytes).is_ok_and(|cf| cf.has_root_stream("Workbook") || cf.has_root_stream("Book")))
    }
    pub fn is_xls_new(bytes: &[u8]) -> Result<bool, String> {
        let has = |cf: &craft_cfb::Cfb, name: &str| cf.entries().any(|e| e.kind() == craft_cfb::EntryKind::Stream && e.path().eq_ignore_ascii_case(name));
        Ok(craft_cfb::Cfb::open(bytes).is_ok_and(|cf| has(&cf, "Workbook") || has(&cf, "Book")))
    }
}

pub mod deckcraft {
    use super::*;
    pub fn sniff_old(bytes: &[u8]) -> Result<bool, String> {
        Ok(crate::old_deckcraft_cfb::is_encrypted(bytes))
    }
    pub fn sniff_new(bytes: &[u8]) -> Result<bool, String> {
        Ok(crate::new_deckcraft_cfb::is_encrypted(bytes))
    }
}

// ---------------------------------------------------------------- wordcraft (crates.io `cfb` + guard -> craft-cfb)

pub mod wordcraft {
    use super::*;

    fn open_old(bytes: &[u8]) -> Result<cfb_ref::CompoundFile<Cursor<&[u8]>>, String> {
        crate::old_wordcraft_cfb_guard::check(bytes)?;
        cfb_ref::CompoundFile::open(Cursor::new(bytes)).map_err(|e| e.to_string())
    }

    /// `(is_xls, docx is_encrypted)` as the swap's call sites compute them.
    pub fn sniff_old(bytes: &[u8]) -> Result<(bool, bool), String> {
        if !bytes.starts_with(&MAGIC) {
            return Ok((false, false));
        }
        let Ok(comp) = open_old(bytes) else { return Ok((false, false)) };
        Ok((comp.is_stream("/Workbook") || comp.is_stream("/Book"), comp.is_stream("/EncryptionInfo") && comp.is_stream("/EncryptedPackage")))
    }
    pub fn sniff_new(bytes: &[u8]) -> Result<(bool, bool), String> {
        if !bytes.starts_with(&MAGIC) {
            return Ok((false, false));
        }
        let Ok(comp) = craft_cfb::Cfb::open(bytes) else { return Ok((false, false)) };
        let has = |name: &str| comp.entries().any(|e| e.kind() == craft_cfb::EntryKind::Stream && e.path().eq_ignore_ascii_case(name));
        Ok((has("Workbook") || has("Book"), has("EncryptionInfo") && has("EncryptedPackage")))
    }

    /// Root streams read with a size cap, as docbin/docx/xls readers do.
    pub fn streams_old(bytes: &[u8]) -> Result<Res<Streams>, String> {
        let mut comp = match open_old(bytes) {
            Ok(c) => c,
            Err(e) => return Ok(Err(e)),
        };
        let mut out = Vec::new();
        for n in root_stream_names(bytes) {
            let r = match comp.open_stream(format!("/{n}")) {
                Ok(s) => {
                    let mut v = Vec::new();
                    match s.take(CAP + 1).read_to_end(&mut v) {
                        Ok(_) if v.len() as u64 > CAP => Err("limit".to_string()),
                        Ok(_) => Ok(Some(v)),
                        Err(e) => Err(format!("read: {e}")),
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
                Err(e) => Err(format!("open_stream: {e}")),
            };
            out.push((n, r));
        }
        Ok(Ok(Streams(out)))
    }
    pub fn streams_new(bytes: &[u8]) -> Result<Res<Streams>, String> {
        let comp = match craft_cfb::Cfb::open(bytes) {
            Ok(c) => c,
            Err(e) => return Ok(Err(e.0.to_string())),
        };
        let mut out = Vec::new();
        for n in root_stream_names(bytes) {
            let entry = comp.entries().find(|e| e.kind() == craft_cfb::EntryKind::Stream && e.path().eq_ignore_ascii_case(&n));
            let r = match entry {
                None => Ok(None),
                Some(e) if e.size() > CAP => Err("limit".to_string()),
                Some(_) => comp.read_stream(&n).map(Some).map_err(|e| e.0.to_string()),
            };
            out.push((n, r));
        }
        Ok(Ok(Streams(out)))
    }
}

pub fn judge_streams_l(o: &Res<Streams>, n: &Res<Streams>) -> Verdict {
    lift(o, n, judge_streams)
}
pub fn judge_film_l(o: &Res<FilmObs>, n: &Res<FilmObs>) -> Verdict {
    lift(o, n, judge_film)
}
pub fn judge_tree_l(o: &Res<Tree>, n: &Res<Tree>) -> Verdict {
    lift(o, n, judge_tree)
}

pub fn judge_bool<T: PartialEq + std::fmt::Debug>(old: &T, new: &T) -> Verdict {
    if old == new {
        Verdict::Identical
    } else {
        Verdict::Exceeds { metric: "sniff".into(), value: 1.0, limit: 0.0 }
    }
}

/// Stream-read observation: classes `presence` (found vs not), `bytes`, `err_class`.
pub fn judge_streams(old: &Streams, new: &Streams) -> Verdict {
    let mut classes = BTreeSet::new();
    let mut soft = false;
    let mut n = 0;
    for (o, w) in old.0.iter().zip(&new.0) {
        if o == w {
            continue;
        }
        n += 1;
        match (&o.1, &w.1) {
            (Ok(Some(a)), Ok(Some(b))) if a != b => classes.insert("bytes"),
            (Ok(Some(_)), Ok(None)) | (Ok(None), Ok(Some(_))) => classes.insert("presence"),
            (Ok(_), Err(_)) => classes.insert("old_ok_new_err"),
            (Err(_), Ok(_)) => classes.insert("old_err_new_ok"),
            (Err(a), Err(b)) if norm(a) != norm(b) => {
                soft = true;
                false
            }
            _ => classes.insert("err_class"),
        };
    }
    if old.0.len() != new.0.len() {
        classes.insert("names");
    }
    if classes.is_empty() {
        return if soft { Verdict::Within { metric: "stream_err_msgs_differ".into(), value: n as f64 } } else { Verdict::Identical };
    }
    Verdict::Exceeds { metric: classes.into_iter().collect::<Vec<_>>().join("+"), value: n.max(1) as f64, limit: 0.0 }
}

// ---------------------------------------------------------------- filmcraft (tree + metadata, by path)

#[derive(Clone, Debug, PartialEq)]
pub struct FilmNode {
    pub kind: u8,
    pub clsid: [u8; 16],
    pub state_bits: u32,
    pub created: u64,
    pub modified: u64,
    pub start: u32,
    pub size: u64,
    pub child_names: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FilmObs {
    pub version: u16,
    pub header_clsid: [u8; 16],
    /// Entries in the crate's own `entries` order, as the path each has in the tree (`?` for one the
    /// tree does not reach).
    pub order: Vec<String>,
    pub tree: BTreeMap<String, FilmNode>,
    pub streams: BTreeMap<String, Result<Vec<u8>, String>>,
}

macro_rules! film {
    ($fn:ident, $krate:ident) => {
        pub fn $fn(bytes: &[u8]) -> Result<Res<FilmObs>, String> {
            use $krate::{CompoundFile, EntryKind};
            let cf = match CompoundFile::open(bytes) {
                Ok(c) => c,
                Err(e) => return Ok(Err(film_message(&format!("{e:?}")))),
            };
            let kind = |e: &$krate::Entry| match e.kind {
                EntryKind::Root => 0u8,
                EntryKind::Storage => 1,
                EntryKind::Stream => 2,
                _ => 3,
            };
            let mut order = vec!["?".to_string(); cf.entries.len()];
            let mut tree = BTreeMap::new();
            let mut streams = BTreeMap::new();
            // Walk from the root through `children`, bounded by the entry count.
            let mut stack = vec![(0usize, String::new())];
            let mut seen = std::collections::BTreeSet::new();
            while let Some((id, path)) = stack.pop() {
                if !seen.insert(id) {
                    continue;
                }
                let Some(e) = cf.entries.get(id) else { continue };
                if let Some(o) = order.get_mut(id) {
                    *o = path.clone();
                }
                let child_names: Vec<String> = e.children.iter().filter_map(|c| cf.entries.get(*c)).map(|c| c.name.clone()).collect();
                if kind(e) == 2 {
                    let r = if e.size > 256 * 1024 * 1024 { Err("too large for the harness".to_string()) } else { cf.read(id).map_err(|e| format!("{e:?}")) };
                    streams.insert(path.clone(), r);
                }
                tree.insert(
                    path.clone(),
                    FilmNode { kind: kind(e), clsid: e.clsid, state_bits: e.state_bits, created: e.created, modified: e.modified, start: e.start, size: e.size, child_names },
                );
                for c in &e.children {
                    if let Some(ce) = cf.entries.get(*c) {
                        stack.push((*c, if path.is_empty() { ce.name.clone() } else { format!("{path}/{}", ce.name) }));
                    }
                }
            }
            Ok(Ok(FilmObs { version: cf.version, header_clsid: cf.header_clsid, order, tree, streams }))
        }
    };
}
/// `Invalid("msg")` -> `msg`, `NotCfb` -> `NotCfb`: filmcraft wraps craft-cfb's message.
pub fn film_message(debug: &str) -> String {
    debug.strip_prefix("Invalid(\"").and_then(|s| s.strip_suffix("\")")).map(str::to_string).unwrap_or_else(|| debug.to_string())
}

pub mod filmcraft {
    use super::*;
    film!(old, filmcraft_cfb_old);
    film!(new, filmcraft_cfb_new);
}

pub fn judge_film(old: &FilmObs, new: &FilmObs) -> Verdict {
    if old == new {
        return Verdict::Identical;
    }
    let mut classes = BTreeSet::new();
    if (old.version, old.header_clsid) != (new.version, new.header_clsid) {
        classes.insert("header");
    }
    if old.tree.keys().ne(new.tree.keys()) {
        classes.insert("entry_set");
    }
    if old.order.len() != new.order.len() {
        classes.insert("unreachable_entries_dropped");
    }
    for (path, o) in &old.tree {
        let Some(n) = new.tree.get(path) else { continue };
        if o.kind != n.kind {
            classes.insert("kind");
        }
        if o.size != n.size {
            classes.insert("size");
        }
        if o.clsid != n.clsid {
            classes.insert("clsid");
        }
        if (o.created, o.modified, o.state_bits) != (n.created, n.modified, n.state_bits) {
            classes.insert("times_state");
        }
        if o.start != n.start {
            classes.insert("start_sector");
        }
        if o.child_names != n.child_names {
            let (mut a, mut b) = (o.child_names.clone(), n.child_names.clone());
            a.sort();
            b.sort();
            classes.insert(if a == b { "child_order" } else { "child_set" });
        }
    }
    for (path, o) in &old.streams {
        match (o, new.streams.get(path)) {
            (Ok(a), Some(Ok(b))) if a != b => classes.insert("bytes"),
            (Ok(_), Some(Err(_))) => classes.insert("old_ok_new_err"),
            (Err(_), Some(Ok(_))) => classes.insert("old_err_new_ok"),
            _ => false,
        };
    }
    if classes.is_empty() && old.order != new.order {
        classes.insert("index_order");
    }
    if classes.is_empty() {
        classes.insert("other");
    }
    Verdict::Exceeds { metric: classes.into_iter().collect::<Vec<_>>().join("+"), value: 1.0, limit: 0.0 }
}

// ---------------------------------------------------------------- independent reference vs craft-cfb

#[derive(Clone, Debug, PartialEq)]
pub struct Tree(pub BTreeMap<String, (bool, u64, Result<Vec<u8>, String>)>);

/// The crates.io `cfb` crate (behind wordcraft's size guard): every reachable entry by path.
pub fn tree_reference(bytes: &[u8]) -> Result<Res<Tree>, String> {
    if let Err(e) = crate::old_wordcraft_cfb_guard::check(bytes) {
        return Ok(Err(e));
    }
    let mut cf = match cfb_ref::CompoundFile::open(Cursor::new(bytes)) {
        Ok(c) => c,
        Err(e) => return Ok(Err(e.to_string())),
    };
    let entries: Vec<(String, bool, u64)> = cf.walk().map(|e| (e.path().to_string_lossy().into_owned(), e.is_stream(), e.len())).collect();
    let mut m = BTreeMap::new();
    for (path, is_stream, len) in entries {
        let data = if is_stream && len <= 256 * 1024 * 1024 {
            match cf.open_stream(&path) {
                Ok(mut s) => {
                    let mut v = Vec::new();
                    s.read_to_end(&mut v).map(|_| v).map_err(|e| format!("read: {e}"))
                }
                Err(e) => Err(format!("open_stream: {e}")),
            }
        } else {
            Ok(Vec::new())
        };
        m.insert(path.to_ascii_uppercase(), (is_stream, len, data));
    }
    Ok(Ok(Tree(m)))
}
pub fn tree_craft_cfb(bytes: &[u8]) -> Result<Res<Tree>, String> {
    let cf = match craft_cfb::Cfb::open(bytes) {
        Ok(c) => c,
        Err(e) => return Ok(Err(e.0.to_string())),
    };
    let mut m = BTreeMap::new();
    for e in cf.entries() {
        let is_stream = e.kind() == craft_cfb::EntryKind::Stream;
        let path = format!("/{}", e.path());
        let data = if is_stream && e.size() <= 256 * 1024 * 1024 { cf.read_stream(e.path()).map_err(|e| e.0.to_string()) } else { Ok(Vec::new()) };
        m.insert(path.to_ascii_uppercase(), (is_stream, e.size(), data));
    }
    Ok(Ok(Tree(m)))
}
pub fn judge_tree(old: &Tree, new: &Tree) -> Verdict {
    if old == new {
        return Verdict::Identical;
    }
    let mut classes = BTreeSet::new();
    for (k, v) in &old.0 {
        match new.0.get(k) {
            None => classes.insert("ref_entry_missing_in_craft_cfb"),
            Some(w) if v.0 != w.0 => classes.insert("kind"),
            Some(w) if v.0 && v.1 != w.1 => classes.insert("stream_size"),
            Some(w) if v.0 && v.2 != w.2 => match (&v.2, &w.2) {
                (Ok(_), Ok(_)) => classes.insert("bytes"),
                (Ok(_), Err(_)) => classes.insert("ref_ok_craft_cfb_err"),
                (Err(_), Ok(_)) => classes.insert("ref_err_craft_cfb_ok"),
                // both fail to read this stream: only the wording differs
                (Err(_), Err(_)) => false,
            },
            _ => false,
        };
    }
    if new.0.keys().any(|k| !old.0.contains_key(k)) {
        classes.insert("craft_cfb_extra_entry");
    }
    if classes.is_empty() {
        return Verdict::Within { metric: "stream_err_msgs_differ".into(), value: 1.0 };
    }
    Verdict::Exceeds { metric: classes.into_iter().collect::<Vec<_>>().join("+"), value: 1.0, limit: 0.0 }
}
