//! Input sets: the ffmpeg matrix, conformance vectors and deterministic hostile mutants.

use std::path::{Path, PathBuf};

pub type Named = (String, Vec<u8>);

pub fn fnv(b: &[u8]) -> u64 {
    b.iter().fold(0xcbf29ce484222325u64, |h, &x| (h ^ u64::from(x)).wrapping_mul(0x100000001b3))
}

pub fn list(dir: &Path, ext: &str) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file() && p.extension().is_some_and(|x| x == ext))
        .collect();
    v.sort();
    v
}

pub fn read_all(files: Vec<PathBuf>) -> impl Iterator<Item = Named> {
    files.into_iter().map(|p| {
        let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        (name, std::fs::read(&p).unwrap_or_default())
    })
}

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        if n == 0 { 0 } else { (self.next() % n as u64) as usize }
    }
}

/// Deterministic damage to one stream. `head` = bytes at the start that carry headers.
pub fn mutants(base: &str, data: &[u8]) -> Vec<Named> {
    let mut out: Vec<Named> = Vec::new();
    let mut rng = Rng(fnv(base.as_bytes()) | 1);
    let n = data.len();
    if n < 64 {
        return out;
    }
    let mut add = |op: String, d: Vec<u8>| out.push((format!("{base}#{op}"), d));
    for (i, pct) in [10usize, 25, 50, 75, 90, 99].into_iter().enumerate() {
        add(format!("trunc{i}-{pct}"), data[..n * pct / 100].to_vec());
    }
    for k in 0..8 {
        let mut d = data.to_vec();
        let pos = rng.below(n);
        d[pos] ^= 1 << rng.below(8);
        add(format!("flip1-{k}"), d);
    }
    for k in 0..6 {
        let mut d = data.to_vec();
        for _ in 0..8 {
            let pos = rng.below(n);
            d[pos] ^= (rng.next() as u8) | 1;
        }
        add(format!("flip8-{k}"), d);
    }
    for k in 0..4 {
        let mut d = data.to_vec();
        let pos = rng.below(n.saturating_sub(32));
        d[pos..pos + 32].fill(0);
        add(format!("zero32-{k}"), d);
    }
    for k in 0..4 {
        let mut d = data.to_vec();
        let pos = rng.below(n.saturating_sub(4));
        d[pos..pos + 4].fill(0xff);
        add(format!("ff4-{k}"), d);
    }
    // header-field sweep: the first bytes carry the parameter sets / sequence headers
    let head = n.min(132);
    for (k, pos) in (0..head).step_by(8).enumerate() {
        let mut d = data.to_vec();
        d[pos] = 0xff;
        add(format!("hdrff-{k:02}@{pos}"), d);
        let mut d = data.to_vec();
        d[pos] = 0x00;
        add(format!("hdr00-{k:02}@{pos}"), d);
    }
    for k in 0..2 {
        let pos = rng.below(n.saturating_sub(64));
        let mut d = data[..pos].to_vec();
        d.extend_from_slice(&data[pos + 64..]);
        add(format!("cut64-{k}"), d);
        let pos = rng.below(n.saturating_sub(256));
        let mut d = data[..pos + 256].to_vec();
        d.extend_from_slice(&data[pos..]);
        add(format!("dup256-{k}"), d);
    }
    out
}

/// Up to `count` small streams of a codec's matrix (spread over the sorted list) to damage.
pub fn mutant_bases(dir: &Path, ext: &str, count: usize, max_bytes: u64) -> Vec<PathBuf> {
    let small: Vec<PathBuf> =
        list(dir, ext).into_iter().filter(|p| std::fs::metadata(p).is_ok_and(|m| m.len() <= max_bytes && m.len() >= 64)).collect();
    if small.len() <= count {
        return small;
    }
    (0..count).map(|i| small[i * small.len() / count].clone()).collect()
}

pub fn mutant_stream(bases: Vec<PathBuf>) -> impl Iterator<Item = Named> {
    bases.into_iter().flat_map(|p| {
        let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let data = std::fs::read(&p).unwrap_or_default();
        mutants(&name, &data)
    })
}

/// Where parameter sets and frame/slice headers start in a stream: Annex-B NAL payloads (h264,
/// hevc) or IVF frame payloads (vp8, vp9, av1). Header fields are what the hardening fixes bound
/// (QP, POC, reference lists, dimensions, tile counts), so the sweep concentrates there.
pub fn header_offsets(codec: &str, data: &[u8], max: usize) -> Vec<usize> {
    let mut out = Vec::new();
    match codec {
        "h264" | "hevc" => {
            let mut i = 0;
            while i + 3 <= data.len() && out.len() < max {
                if data[i] == 0 && data[i + 1] == 0 && data[i + 2] == 1 {
                    out.push(i + 3);
                    i += 3;
                } else {
                    i += 1;
                }
            }
        }
        _ => {
            out.push(0);
            if let Some(h) = data.get(6..8) {
                let mut pos = usize::from(u16::from_le_bytes([h[0], h[1]]));
                while pos + 12 <= data.len() && out.len() < max {
                    out.push(pos);
                    let size = u32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as usize;
                    pos = (pos + 12).saturating_add(size);
                    if pos >= data.len() {
                        break;
                    }
                }
                // frame payloads start 12 bytes after each frame header
                let hdrs = out.split_off(1);
                out.extend(hdrs.iter().flat_map(|&p| [p, p + 12]));
            }
        }
    }
    out
}

/// Exhaustive single-bit flips plus 0x00/0xff byte substitutions over the first `width` bytes at
/// each header offset (at most `max_off` offsets).
pub fn sweep(codec: &str, base: &str, data: &[u8], width: usize, max_off: usize) -> Vec<Named> {
    let mut out = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for off in header_offsets(codec, data, max_off) {
        for pos in off..(off + width).min(data.len()) {
            if !seen.insert(pos) {
                continue;
            }
            for bit in 0..8 {
                let mut d = data.to_vec();
                d[pos] ^= 1 << bit;
                out.push((format!("{base}#sweep-flip@{pos}.{bit}"), d));
            }
            for v in [0x00u8, 0xff] {
                if data[pos] != v {
                    let mut d = data.to_vec();
                    d[pos] = v;
                    out.push((format!("{base}#sweep-set{v:02x}@{pos}"), d));
                }
            }
        }
    }
    out
}

pub fn sweep_stream(codec: &'static str, bases: Vec<PathBuf>, width: usize, max_off: usize) -> impl Iterator<Item = Named> {
    bases.into_iter().flat_map(move |p| {
        let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let data = std::fs::read(&p).unwrap_or_default();
        sweep(codec, &name, &data, width, max_off)
    })
}

fn unescape(b: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(b.len());
    let mut zeros = 0;
    for &x in b {
        if zeros >= 2 && x == 3 {
            zeros = 0;
            continue;
        }
        zeros = if x == 0 { zeros + 1 } else { 0 };
        out.push(x);
    }
    out
}

fn escape(b: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(b.len() + 4);
    let mut zeros = 0;
    for &x in b {
        if zeros >= 2 && x <= 3 {
            out.push(3);
            zeros = 0;
        }
        zeros = if x == 0 { zeros + 1 } else { 0 };
        out.push(x);
    }
    out
}

/// Value-extreme havoc: patterns that make Exp-Golomb fields huge or negative (31 leading zeros,
/// all ones, sign flips) laid over the first bytes of each of the first NAL units' RBSP (Annex-B
/// streams, re-escaped so the framing stays intact) or over IVF frame payloads. Overflowing
/// QP, POC, reference-index and count fields are what the #48 hardening commits bound.
pub fn havoc(codec: &str, base: &str, data: &[u8], nals: usize, span: usize) -> Vec<Named> {
    const PATTERNS: [(&str, [u8; 4]); 6] = [
        ("z4", [0, 0, 0, 0]),
        ("o4", [0xff; 4]),
        ("ue31", [0, 0, 0, 1]),
        ("max", [0x7f, 0xff, 0xff, 0xff]),
        ("min", [0x80, 0, 0, 0]),
        ("z3f", [0, 0, 0, 0xff]),
    ];
    let mut out = Vec::new();
    match codec {
        "h264" | "hevc" => {
            let hdr = if codec == "h264" { 1 } else { 2 };
            let mut starts = Vec::new();
            let mut i = 0;
            while i + 3 <= data.len() {
                if data[i] == 0 && data[i + 1] == 0 && data[i + 2] == 1 {
                    starts.push(i + 3);
                    i += 3;
                } else {
                    i += 1;
                }
            }
            for (ni, &s) in starts.iter().enumerate().take(nals) {
                // payload ends at the next start code's leading zeros
                let mut e = starts.get(ni + 1).map_or(data.len(), |&n| n - 3);
                while e > s && data[e - 1] == 0 {
                    e -= 1;
                }
                if e <= s + hdr {
                    continue;
                }
                let rbsp = unescape(&data[s + hdr..e]);
                for p in 0..span.min(rbsp.len()) {
                    for (pn, pat) in PATTERNS {
                        let mut r = rbsp.clone();
                        for (k, v) in pat.iter().enumerate() {
                            if p + k < r.len() {
                                r[p + k] = *v;
                            }
                        }
                        let mut d = data[..s + hdr].to_vec();
                        d.extend(escape(&r));
                        d.extend_from_slice(&data[e..]);
                        out.push((format!("{base}#havoc-{pn}@nal{ni}.{p}"), d));
                    }
                }
            }
        }
        _ => {
            for off in header_offsets(codec, data, nals * 2) {
                for p in 0..span {
                    for (pn, pat) in PATTERNS {
                        if off + p + 4 > data.len() {
                            continue;
                        }
                        let mut d = data.to_vec();
                        d[off + p..off + p + 4].copy_from_slice(&pat);
                        out.push((format!("{base}#havoc-{pn}@{off}.{p}"), d));
                    }
                }
            }
        }
    }
    out
}

pub fn havoc_stream(codec: &'static str, bases: Vec<PathBuf>, nals: usize, span: usize) -> impl Iterator<Item = Named> {
    bases.into_iter().flat_map(move |p| {
        let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let data = std::fs::read(&p).unwrap_or_default();
        havoc(codec, &name, &data, nals, span)
    })
}
