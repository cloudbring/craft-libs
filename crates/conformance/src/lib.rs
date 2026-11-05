//! Official codec conformance streams run against the Craft decoders (ticket #195).
//!
//! Each suite is a pinned `craft-corpus` manifest (`crates/corpus/manifests/conformance-<codec>.toml`)
//! fetched into `$CRAFT_CORPUS_DIR/conformance-<codec>` (default `<workspace>/corpus`). A suite
//! decodes every stream and compares the output with reference MD5s:
//!
//! - VP8, VP9, AV1: the per-frame `.md5` files published next to the vectors.
//! - H.264, HEVC: `expected/<codec>.tsv`, MD5s of the whole decoded output (and the frame count) of
//!   the stream decoded by ffmpeg, produced by `scripts/gen_expected.py`. The FATE mirror ships the
//!   bitstreams only.
//!
//! A stream is `Pass`, `Fail` or `Unsupported` (the decoder says so, or the stream uses a feature the
//! crate README lists as a known gap). Absent corpora skip unless `CRAFT_CORPUS_REQUIRE=1` (or
//! `CONFORMANCE_<CODEC>_CORPUS_REQUIRED=1`) is set; a present corpus that does not match its manifest
//! always fails.

pub mod av1;
pub mod h264;
pub mod hevc;
pub mod md5;
pub mod vp8;
pub mod vp9;

use std::collections::BTreeMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

pub use md5::{Md5, md5_hex};

/// Outcome of one stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    Pass,
    Fail(String),
    Unsupported(String),
}

/// All streams of one suite.
#[derive(Debug, Default)]
pub struct Suite {
    pub codec: String,
    pub results: Vec<(String, Verdict)>,
}

impl Suite {
    pub fn counts(&self) -> (usize, usize, usize) {
        let mut c = (0, 0, 0);
        for (_, v) in &self.results {
            match v {
                Verdict::Pass => c.0 += 1,
                Verdict::Fail(_) => c.1 += 1,
                Verdict::Unsupported(_) => c.2 += 1,
            }
        }
        c
    }

    /// Prints one `RESULT` line per stream and a `CONFORMANCE` summary (and appends the lines to
    /// `$CRAFT_CONFORMANCE_REPORT` when set).
    pub fn print(&self) {
        let (p, f, u) = self.counts();
        let mut text = String::new();
        for (name, v) in &self.results {
            let (k, d) = match v {
                Verdict::Pass => ("pass", ""),
                Verdict::Fail(d) => ("fail", d.as_str()),
                Verdict::Unsupported(d) => ("unsupported", d.as_str()),
            };
            text.push_str(&format!("RESULT\t{}\t{name}\t{k}\t{d}\n", self.codec));
        }
        text.push_str(&format!("CONFORMANCE\t{}\tstreams={}\tpass={p}\tfail={f}\tunsupported={u}\n", self.codec, self.results.len()));
        print!("{text}");
        if let Some(path) = std::env::var_os("CRAFT_CONFORMANCE_REPORT") {
            use std::io::Write;
            if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
                let _ = file.write_all(text.as_bytes());
            }
        }
    }

    /// Fails unless every failure is in `known` (stream, reason), every `known` entry still fails,
    /// and at least `min_pass` streams pass (the ratchet: raise it when more streams pass).
    pub fn assert_clean(&self, known: &[(&str, &str)], min_pass: usize) {
        self.print();
        let failing: BTreeMap<&str, &str> =
            self.results.iter().filter_map(|(n, v)| if let Verdict::Fail(d) = v { Some((n.as_str(), d.as_str())) } else { None }).collect();
        let known: BTreeMap<&str, &str> = known.iter().copied().collect();
        let new: Vec<_> = failing.iter().filter(|(n, _)| !known.contains_key(*n)).collect();
        let stale: Vec<_> = known.keys().filter(|n| !failing.contains_key(*n)).collect();
        assert!(new.is_empty(), "{}: new conformance failures: {new:#?}", self.codec);
        assert!(stale.is_empty(), "{}: known failures that now pass (remove them): {stale:?}", self.codec);
        let (p, ..) = self.counts();
        assert!(p >= min_pass, "{}: {p} streams pass, floor is {min_pass}", self.codec);
    }
}

/// Where the corpora live: `$CRAFT_CORPUS_DIR`, else `<workspace>/corpus`.
pub fn corpus_dir() -> PathBuf {
    std::env::var_os("CRAFT_CORPUS_DIR").map(PathBuf::from).unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus"))
}

/// A verified conformance corpus: file path by corpus-relative name.
pub struct Files {
    pub root: PathBuf,
    pub names: Vec<String>,
}

impl Files {
    pub fn path(&self, rel: &str) -> PathBuf {
        self.root.join(rel)
    }
    pub fn read(&self, rel: &str) -> Vec<u8> {
        std::fs::read(self.path(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
    }
}

/// Loads `conformance-<codec>` (verified against its shipped manifest). `None` = absent and not
/// required (a `SKIP` line is printed); panics when required-but-absent or when the tree is stale.
pub fn load(codec: &str) -> Option<Files> {
    let name = format!("conformance-{codec}");
    let manifest_path = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("../corpus/manifests/{name}.toml"));
    let manifest = craft_corpus::Manifest::load(&manifest_path).unwrap_or_else(|e| panic!("{}: {e}", manifest_path.display()));
    let root = corpus_dir().join(&name);
    let corpus = match craft_corpus::load_or_skip(&name, &root, Some(&manifest)) {
        Ok(Some(c)) => c,
        Ok(None) => return None,
        Err(e) => panic!("{e}"),
    };
    let names = corpus.files().into_iter().map(|f| f.rel).collect();
    Some(Files { root, names })
}

/// Runs `f` on every item with a small thread pool; results keep the item order.
pub fn par_map<T: Sync, R: Send>(items: &[T], f: impl Fn(&T) -> R + Sync) -> Vec<R> {
    let next = AtomicUsize::new(0);
    let slots: Vec<std::sync::Mutex<Option<R>>> = items.iter().map(|_| std::sync::Mutex::new(None)).collect();
    let workers = std::thread::available_parallelism().map_or(2, |n| n.get()).clamp(1, 6);
    std::thread::scope(|s| {
        for _ in 0..workers {
            s.spawn(|| {
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    if i >= items.len() {
                        break;
                    }
                    *slots[i].lock().unwrap() = Some(f(&items[i]));
                }
            });
        }
    });
    slots.into_iter().map(|m| m.into_inner().unwrap().unwrap()).collect()
}

/// Runs `f`, turning a panic into `Fail("panic: ...")`.
pub fn guarded(f: impl FnOnce() -> Verdict) -> Verdict {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(v) => v,
        Err(p) => {
            let msg = p.downcast_ref::<String>().cloned().or_else(|| p.downcast_ref::<&str>().map(|s| (*s).to_string())).unwrap_or_default();
            Verdict::Fail(format!("panic: {}", msg.lines().next().unwrap_or("")))
        }
    }
}

/// The IVF frames of `data` (32-byte file header, 12-byte frame headers).
pub fn ivf_frames(data: &[u8]) -> Vec<&[u8]> {
    let mut out = Vec::new();
    let Some(h) = data.get(6..8) else { return out };
    let mut pos = usize::from(u16::from_le_bytes([h[0], h[1]]));
    while pos + 12 <= data.len() {
        let size = u32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as usize;
        pos += 12;
        let end = pos.saturating_add(size).min(data.len());
        out.push(&data[pos..end]);
        pos = end;
    }
    out
}

/// The first video track's packets of a Matroska / WebM file.
pub fn matroska_video_packets(data: &[u8]) -> Result<Vec<Vec<u8>>, String> {
    let mut demux = craft_media::matroska::Demuxer::from_slice(data).map_err(|e| format!("demux: {e}"))?;
    let track = demux.tracks().iter().position(|t| t.video.is_some()).ok_or_else(|| "no video track".to_string())?;
    let mut out = Vec::new();
    while let Some(p) = demux.next_packet().map_err(|e| format!("demux: {e}"))? {
        if p.track == track {
            out.push(p.data);
        }
    }
    Ok(out)
}

/// Parses `md5  name` lines (the libvpx / libaom per-frame files): the first column of each line.
pub fn md5_lines(text: &str) -> Vec<String> {
    text.lines().filter_map(|l| l.split_whitespace().next()).map(str::to_string).collect()
}

/// Appends `rows` rows of `row_len` samples from a strided 8-bit plane.
pub fn feed_plane_u8(m: &mut Md5, plane: &[u8], stride: usize, row_len: usize, rows: usize) {
    for r in 0..rows {
        m.update(&plane[r * stride..r * stride + row_len]);
    }
}

/// Appends `rows` rows of `row_len` 16-bit samples as little-endian bytes.
pub fn feed_plane_u16(m: &mut Md5, plane: &[u16], stride: usize, row_len: usize, rows: usize) {
    let mut row = Vec::with_capacity(row_len * 2);
    for r in 0..rows {
        row.clear();
        row.extend(plane[r * stride..r * stride + row_len].iter().flat_map(|s| s.to_le_bytes()));
        m.update(&row);
    }
}

/// Expected whole-output digests: `name<TAB>frames<TAB>md5`; `frames` is `-` when the reference
/// decoder rejected the stream.
pub fn parse_expected(text: &str) -> BTreeMap<String, (Option<usize>, String)> {
    text.lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .map(|l| {
            let mut c = l.split('\t');
            let name = c.next().unwrap().to_string();
            let frames = c.next().unwrap_or("-").parse().ok();
            (name, (frames, c.next().unwrap_or("").to_string()))
        })
        .collect()
}

/// Compares a whole-output digest with `expected`.
pub fn check_whole(name: &str, expected: &BTreeMap<String, (Option<usize>, String)>, frames: usize, md5: &str) -> Verdict {
    match expected.get(name) {
        None => Verdict::Fail("no reference digest for this stream (regenerate expected/*.tsv)".into()),
        Some((None, _)) => Verdict::Unsupported("no reference output: the reference decoder (ffmpeg) rejects this stream".into()),
        Some((Some(n), want)) if *n != frames => Verdict::Fail(format!("{frames} frames, reference {n}")),
        Some((Some(_), want)) if want != md5 => Verdict::Fail(format!("output md5 {md5}, reference {want}")),
        Some(_) => Verdict::Pass,
    }
}

/// Compares per-frame digests; the first mismatch is reported.
pub fn check_frames(got: &[String], want: &[String]) -> Verdict {
    if let Some(i) = got.iter().zip(want).position(|(a, b)| a != b) {
        return Verdict::Fail(format!("frame {i} md5 mismatch ({} decoded, {} reference)", got.len(), want.len()));
    }
    if got.len() != want.len() {
        return Verdict::Fail(format!("{} frames decoded, {} in the reference", got.len(), want.len()));
    }
    Verdict::Pass
}

/// A decode error on a stream the reference decoder also rejects is `Unsupported` (no reference
/// output exists to hold the decoder to); on any other stream it is a failure.
pub fn rejected_or_fail(name: &str, expected: &BTreeMap<String, (Option<usize>, String)>, error: String) -> Verdict {
    match expected.get(name) {
        Some((None, _)) => Verdict::Unsupported(format!("the reference decoder (ffmpeg) rejects this stream too; ours: {error}")),
        _ => Verdict::Fail(error),
    }
}
