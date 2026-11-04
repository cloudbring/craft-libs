//! Per-run counters and the markdown summary.

use std::collections::BTreeMap;
use std::fmt::Write as _;

#[derive(Default, Clone)]
pub struct Acc {
    pub streams: usize,
    pub units: usize,
    pub frames_old: usize,
    pub frames_new: usize,
    pub old_ok: usize,
    pub old_err: usize,
    pub old_panic: usize,
    pub new_ok: usize,
    pub new_err: usize,
    pub new_panic: usize,
    pub pcm_samples: usize,
}

pub struct Row {
    pub scenario: String,
    pub app: String,
    pub codec: String,
    pub set: String,
    pub total: usize,
    pub counts: BTreeMap<String, usize>,
    pub acc: Acc,
}

pub fn markdown(rows: &[Row]) -> String {
    let mut s = String::from(
        "| scenario | set | streams | pictures old/new | PCM samples | old ok/err/panic | new ok/err/panic | identical | expected | other |\n|---|---|---:|---:|---:|---|---|---:|---:|---:|\n",
    );
    for r in rows {
        let c = |k: &str| r.counts.get(k).copied().unwrap_or(0);
        let other: usize = r.total - c("Identical") - c("ExpectedDifference");
        let _ = writeln!(
            s,
            "| {} | {} | {} | {}/{} | {} | {}/{}/{} | {}/{}/{} | {} | {} | {} |",
            r.scenario,
            r.set,
            r.total,
            r.acc.frames_old,
            r.acc.frames_new,
            r.acc.pcm_samples,
            r.acc.old_ok,
            r.acc.old_err,
            r.acc.old_panic,
            r.acc.new_ok,
            r.acc.new_err,
            r.acc.new_panic,
            c("Identical"),
            c("ExpectedDifference"),
            other
        );
    }
    s
}
