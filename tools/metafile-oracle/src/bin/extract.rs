//! Extract every embedded EMF/WMF (and EMZ/WMZ) from the Office corpus into a content-addressed
//! cache: `extract <corpus-root> <out-dir>`. Writes `<out>/<sha256>.<emf|wmf>` and
//! `<out>/manifest.tsv` (source file, part, sha256, kind, bytes). Never commit the output.
//!
//! Sources: OOXML zip media parts by extension (`.emf .wmf .emz .wmz`, gunzipped when compressed),
//! and OfficeArt EMF/WMF BLIP records found in any stream of an OLE/CFB container (`.doc .ppt .xls`).
use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::io::Read as _;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    let mut v: Vec<_> = rd.filter_map(Result::ok).map(|e| e.path()).collect();
    v.sort();
    for p in v {
        let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        if name.starts_with('.') || name.starts_with("~$") {
            continue;
        }
        if p.is_dir() {
            walk(&p, out);
        } else {
            out.push(p);
        }
    }
}

fn gunzip(b: &[u8]) -> Option<Vec<u8>> {
    let mut o = Vec::new();
    flate2::read::GzDecoder::new(b).take(256 << 20).read_to_end(&mut o).ok()?;
    Some(o)
}

fn zlib(b: &[u8], cap: u64) -> Option<Vec<u8>> {
    let mut o = Vec::new();
    flate2::read::ZlibDecoder::new(b).take(cap).read_to_end(&mut o).ok()?;
    Some(o)
}

/// OfficeArt BLIP records for metafiles: (offset, is_emf, payload).
fn blips(s: &[u8]) -> Vec<(usize, bool, Vec<u8>)> {
    let mut out = Vec::new();
    let pats: [([u8; 4], bool, bool); 4] = [
        ([0x40, 0x3D, 0x1A, 0xF0], true, false),
        ([0x50, 0x3D, 0x1A, 0xF0], true, true),
        ([0x60, 0x21, 0x1B, 0xF0], false, false),
        ([0x70, 0x21, 0x1B, 0xF0], false, true),
    ];
    let mut i = 0;
    while i + 8 < s.len() {
        let mut hit = None;
        for (p, emf, two) in pats {
            if s[i..i + 4] == p {
                hit = Some((emf, two));
            }
        }
        if let Some((emf, two)) = hit {
            let len = u32::from_le_bytes(s[i + 4..i + 8].try_into().unwrap()) as usize;
            let uid = if two { 32 } else { 16 };
            let h = i + 8 + uid;
            if len >= uid + 34 && i + 8 + len <= s.len() && h + 34 <= s.len() {
                let cb_save = u32::from_le_bytes(s[h + 28..h + 32].try_into().unwrap()) as usize;
                let comp = s[h + 32];
                let d = h + 34;
                let end = (d + cb_save).min(i + 8 + len);
                let raw = &s[d..end];
                let data = if comp == 0 {
                    zlib(raw, 256 << 20)
                } else if comp == 0xFE {
                    Some(raw.to_vec())
                } else {
                    None
                };
                if let Some(data) = data.filter(|d| d.len() > 16) {
                    out.push((i, emf, data));
                }
                i += 8 + len;
                continue;
            }
        }
        i += 1;
    }
    out
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (root, out) = (PathBuf::from(&args[1]), PathBuf::from(&args[2]));
    std::fs::create_dir_all(&out).unwrap();
    let mut files = Vec::new();
    for t in ["office-poi", "office-openxml-sdk", "office-libreoffice"] {
        walk(&root.join(t), &mut files);
    }
    let mut manifest = String::new();
    let mut seen = BTreeSet::new();
    let (mut scanned, mut with_any, mut occurrences) = (0usize, 0usize, 0usize);
    let mut add = |src: &str, part: &str, bytes: &[u8], kind: &str, any: &mut bool| {
        let h = Sha256::digest(bytes);
        let hex: String = h.iter().map(|b| format!("{b:02x}")).collect();
        occurrences += 1;
        *any = true;
        if seen.insert(hex.clone()) {
            std::fs::write(out.join(format!("{hex}.{kind}")), bytes).unwrap();
        }
        let _ = writeln!(manifest, "{src}\t{part}\t{hex}\t{kind}\t{}", bytes.len());
    };
    for p in &files {
        let ext = p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
        let rel = p.strip_prefix(&root).unwrap_or(p).to_string_lossy().into_owned();
        let Ok(bytes) = std::fs::read(p) else { continue };
        let mut any = false;
        match ext.as_str() {
            "docx" | "docm" | "pptx" | "pptm" | "xlsx" | "xlsm" => {
                scanned += 1;
                let Ok(mut z) = zip::ZipArchive::new(std::io::Cursor::new(&bytes)) else { continue };
                for i in 0..z.len() {
                    let Ok(f) = z.by_index(i) else { continue };
                    let name = f.name().to_string();
                    let lower = name.to_lowercase();
                    let Some(e) = lower.rsplit('.').next() else { continue };
                    if !["emf", "wmf", "emz", "wmz"].contains(&e) {
                        continue;
                    }
                    let mut d = Vec::new();
                    if f.take(256 << 20).read_to_end(&mut d).is_err() {
                        continue;
                    }
                    let (d, kind) = if e.ends_with('z') || d.starts_with(&[0x1f, 0x8b]) { (gunzip(&d).unwrap_or(d), &e[..2]) } else { (d, e) };
                    let kind = if kind == "em" {
                        "emf"
                    } else if kind == "wm" {
                        "wmf"
                    } else {
                        kind
                    };
                    let kind = if kind == "emf" || kind == "wmf" { kind } else { "bin" };
                    add(&rel, &name, &d, kind, &mut any);
                }
            }
            "doc" | "ppt" | "xls" => {
                scanned += 1;
                let Ok(cfb) = craft_cfb::Cfb::open(&bytes) else { continue };
                let paths: Vec<String> = cfb.entries().filter(|e| e.kind() == craft_cfb::EntryKind::Stream).map(|e| e.path().to_string()).collect();
                for sp in paths {
                    let Ok(s) = cfb.read_stream(&sp) else { continue };
                    for (off, emf, data) in blips(&s) {
                        add(&rel, &format!("{sp}@{off}"), &data, if emf { "emf" } else { "wmf" }, &mut any);
                    }
                }
            }
            _ => {}
        }
        with_any += usize::from(any);
    }
    std::fs::write(out.join("manifest.tsv"), &manifest).unwrap();
    eprintln!("scanned {scanned} office files; {with_any} contain metafiles; {occurrences} occurrences; {} unique", seen.len());
}
