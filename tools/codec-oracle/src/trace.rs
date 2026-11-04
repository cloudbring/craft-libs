//! What one side did with one stream: how it ended, and every picture / PCM sample it produced.

use sha2::{Digest, Sha256};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Once;

/// How decoding ended.
#[derive(Clone, Debug, PartialEq)]
pub enum End {
    Complete,
    /// The decoder returned an error: variant name (the class), position (access unit / chunk /
    /// packet index; -1 for an error reported after the last unit) and the message.
    Error {
        class: String,
        at: i64,
        msg: String,
    },
    Panic {
        msg: String,
        loc: String,
    },
}

impl End {
    /// Short label used in metrics: `ok`, `err-<Class>` or `panic`.
    pub fn label(&self) -> String {
        match self {
            End::Complete => "ok".into(),
            End::Error { class, .. } => format!("err-{class}"),
            End::Panic { .. } => "panic".into(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct PlaneSig {
    pub len: usize,
    pub sha: [u8; 32],
    /// The samples (little-endian bytes for 16-bit planes), kept while the trace is under budget.
    pub data: Option<Vec<u8>>,
}

#[derive(Clone, Debug)]
pub struct FrameSig {
    pub w: u32,
    pub h: u32,
    pub depth: u32,
    pub pts: i64,
    pub key: bool,
    pub planes: Vec<PlaneSig>,
}

#[derive(Clone, Debug)]
pub struct Trace {
    pub end: End,
    pub frames: Vec<FrameSig>,
    /// Opus: interleaved or per-channel-concatenated PCM of all packets.
    pub pcm: Vec<f32>,
    /// Opus: final range-coder state after each non-empty packet.
    pub ranges: Vec<u32>,
    /// Units fed to the decoder (access units, chunks, packets).
    pub units: usize,
    /// FNV-1a of the input bytes (lets the judge's details be matched to a file name).
    pub input: u64,
    /// The same stream through the shared crate before its hardening commit (attribution only).
    pub pre: Option<std::sync::Arc<Trace>>,
    kept: usize,
}

/// Plane bytes retained per trace so a difference can be measured, not only detected.
const KEEP_BYTES: usize = 96 << 20;

impl Trace {
    pub fn new() -> Self {
        Self { end: End::Complete, frames: Vec::new(), pcm: Vec::new(), ranges: Vec::new(), units: 0, input: 0, pre: None, kept: 0 }
    }

    pub fn push_frame(&mut self, w: u32, h: u32, depth: u32, pts: i64, key: bool, planes: &[&[u8]]) {
        let sigs = planes
            .iter()
            .map(|p| {
                let keep = self.kept + p.len() <= KEEP_BYTES;
                if keep {
                    self.kept += p.len();
                }
                PlaneSig { len: p.len(), sha: Sha256::digest(p).into(), data: keep.then(|| p.to_vec()) }
            })
            .collect();
        self.frames.push(FrameSig { w, h, depth, pts, key, planes: sigs });
    }

    pub fn error(&mut self, e: &dyn std::fmt::Debug, msg: String, at: i64) {
        let dbg = format!("{e:?}");
        let class: String = dbg.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
        self.end = End::Error { class, at, msg };
    }
}

impl Default for Trace {
    fn default() -> Self {
        Self::new()
    }
}

pub fn le16(v: &[u16]) -> Vec<u8> {
    v.iter().flat_map(|s| s.to_le_bytes()).collect()
}

thread_local! {
    static LAST_PANIC: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
}

/// Silence the default panic output and remember where the panic happened.
fn silence_panics() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        std::panic::set_hook(Box::new(|info| {
            let loc = info.location().map(|l| format!("{}:{}", l.file(), l.line())).unwrap_or_default();
            LAST_PANIC.with(|c| *c.borrow_mut() = loc);
        }))
    });
}

/// Run a decoder driver; a panic becomes `End::Panic` (frames decoded before it are lost, which
/// is the same for both sides).
pub fn guarded(f: impl FnOnce() -> Trace) -> Result<Trace, String> {
    silence_panics();
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(t) => Ok(t),
        Err(p) => {
            let msg = p
                .downcast_ref::<&str>()
                .map(|s| (*s).to_string())
                .or_else(|| p.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "non-string panic payload".into());
            let mut t = Trace::new();
            let loc = LAST_PANIC.with(|c| c.borrow().clone());
            t.end = End::Panic { msg, loc };
            Ok(t)
        }
    }
}
