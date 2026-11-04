//! The report: versioned JSON schema, verdicts, markdown table.

use std::fmt::Write as _;

use serde::{Deserialize, Serialize};

use crate::probe::{Fingerprint, Guard};
use crate::schedule::Side;

/// Bumped on any change to field names or meaning. Readers reject other versions.
pub const SCHEMA_VERSION: u32 = 1;
/// The evidence standard (research/VERIFICATION.md 1.2) wants at least this many repetitions.
pub const MIN_REPETITIONS: usize = 10;
/// ...and the ticket wants at least this many bootstrap resamples.
pub const MIN_RESAMPLES: usize = 2000;

/// Git revisions of the two implementations, passed in by the caller.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Revs {
    pub old: Option<String>,
    pub new: Option<String>,
}

/// How the run was configured, as executed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConfigRecord {
    /// What the caller asked for.
    pub requested_repetitions: usize,
    /// Timed runs per side per input actually executed (requested, rounded up to even).
    pub runs_per_side: usize,
    pub warmup_runs: usize,
    pub resamples: usize,
    pub seed: u64,
    /// True when an output-equality check passed for every input before timing.
    pub output_checked: bool,
    /// `"process"` when CPU time was recorded, `"unavailable"` on targets without a process
    /// CPU clock (then every `cpu` field is null).
    pub cpu_time: String,
}

/// Summary of one side's runs, in nanoseconds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SideStats {
    pub n: usize,
    pub median_ns: f64,
    /// Raw median absolute deviation (not scaled to a standard deviation).
    pub mad_ns: f64,
    pub min_ns: f64,
    pub max_ns: f64,
}

/// A ratio `new / old` with its bootstrap 95% CI. Below 1.0 means new is faster.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Ratio {
    pub point: f64,
    pub ci_lo: f64,
    pub ci_hi: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Faster,
    Slower,
    /// The CI includes 1.0: not a speedup, not a regression.
    Inconclusive,
}

impl Ratio {
    pub fn verdict(&self) -> Verdict {
        if self.ci_hi < 1.0 {
            Verdict::Faster
        } else if self.ci_lo > 1.0 {
            Verdict::Slower
        } else {
            Verdict::Inconclusive
        }
    }
}

impl Verdict {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Faster => "faster",
            Self::Slower => "slower",
            Self::Inconclusive => "inconclusive (CI includes 1.0)",
        }
    }
}

/// Old and new stats for one clock, plus the ratio.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MetricResult {
    pub old: SideStats,
    pub new: SideStats,
    pub ratio: Ratio,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InputResult {
    pub name: String,
    pub wall: MetricResult,
    /// Process CPU time (all threads). Null where unavailable.
    pub cpu: Option<MetricResult>,
}

/// Geometric-mean ratio across inputs, with a bootstrap CI.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Aggregate {
    pub inputs: usize,
    pub wall: Ratio,
    pub cpu: Option<Ratio>,
}

/// One timed run, in execution order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunRecord {
    pub order: usize,
    pub input: String,
    pub side: Side,
    pub wall_ns: u64,
    pub cpu_ns: Option<u64>,
}

/// The whole result. Field names are the JSON schema (see the crate README).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Report {
    pub schema_version: u32,
    pub name: String,
    pub revs: Revs,
    pub fingerprint: Fingerprint,
    pub config: ConfigRecord,
    pub guard: Guard,
    /// Mirrors `guard.contended`, for consumers that read one flag.
    pub contended: bool,
    /// Quiet machine, at least [`MIN_REPETITIONS`] repetitions, at least [`MIN_RESAMPLES`]
    /// resamples, and an output check. Only a publishable report may back a speedup claim.
    pub publishable: bool,
    pub inputs: Vec<InputResult>,
    pub aggregate: Aggregate,
    pub runs: Vec<RunRecord>,
}

/// Why a report cannot be published or read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ReportError {
    #[error("report is not publishable: {0}")]
    NotPublishable(String),
    #[error("unsupported schema_version {found} (this build reads {SCHEMA_VERSION})")]
    Schema { found: u32 },
    #[error("invalid JSON: {0}")]
    Json(String),
}

impl Report {
    /// Why this report is not publishable, empty if it is.
    pub fn publish_problems(&self) -> Vec<String> {
        let mut p = self.guard.reasons.clone();
        if self.config.runs_per_side < MIN_REPETITIONS {
            p.push(format!("{} repetitions per side, need at least {MIN_REPETITIONS}", self.config.runs_per_side));
        }
        if self.config.resamples < MIN_RESAMPLES {
            p.push(format!("{} bootstrap resamples, need at least {MIN_RESAMPLES}", self.config.resamples));
        }
        if !self.config.output_checked {
            p.push("no output-equality check was run".to_string());
        }
        p
    }

    pub fn require_publishable(&self) -> Result<(), ReportError> {
        let p = self.publish_problems();
        if p.is_empty() { Ok(()) } else { Err(ReportError::NotPublishable(p.join("; "))) }
    }

    pub fn to_json(&self) -> String {
        // Plain data with finite numbers: serialisation cannot fail.
        serde_json::to_string_pretty(self).unwrap_or_default()
    }

    pub fn from_json(s: &str) -> Result<Self, ReportError> {
        let v: serde_json::Value = serde_json::from_str(s).map_err(|e| ReportError::Json(e.to_string()))?;
        let found = v.get("schema_version").and_then(serde_json::Value::as_u64).unwrap_or(0);
        if found != u64::from(SCHEMA_VERSION) {
            return Err(ReportError::Schema { found: u32::try_from(found).unwrap_or(u32::MAX) });
        }
        serde_json::from_value(v).map_err(|e| ReportError::Json(e.to_string()))
    }

    /// Markdown table of per-input medians, MADs, ratios and the aggregate.
    pub fn to_markdown(&self) -> String {
        let mut s = String::new();
        let _ = writeln!(s, "### {}", self.name);
        let _ = writeln!(s);
        if self.publishable {
            let _ = writeln!(s, "Publishable.");
        } else {
            let _ = writeln!(s, "**NOT PUBLISHABLE:** {}", self.publish_problems().join("; "));
        }
        let f = &self.fingerprint;
        let _ = writeln!(
            s,
            "\n{} {} ({}), {}, {} logical cores, {}, {} opt-level {}, {}.",
            f.os,
            f.arch,
            f.kernel.as_deref().unwrap_or("kernel unknown"),
            f.cpu_model.as_deref().unwrap_or("CPU unknown"),
            f.logical_cores,
            f.ram_bytes.map_or("RAM unknown".to_string(), |b| format!("{:.1} GiB", b as f64 / (1u64 << 30) as f64)),
            f.profile,
            f.opt_level,
            f.date
        );
        let _ = writeln!(
            s,
            "Revs old `{}` new `{}`. Load before {} after {} (limit {:.2}). {} runs per side, {} resamples, seed {}.\n",
            self.revs.old.as_deref().unwrap_or("?"),
            self.revs.new.as_deref().unwrap_or("?"),
            self.guard.load_before.map_or("?".to_string(), |l| format!("{l:.2}")),
            self.guard.load_after.map_or("?".to_string(), |l| format!("{l:.2}")),
            self.guard.load_threshold,
            self.config.runs_per_side,
            self.config.resamples,
            self.config.seed
        );
        let _ = writeln!(s, "| input | old median | old MAD | new median | new MAD | new/old (95% CI) | verdict |");
        let _ = writeln!(s, "|---|---:|---:|---:|---:|---:|---|");
        for i in &self.inputs {
            let w = &i.wall;
            let _ = writeln!(
                s,
                "| {} | {} | {} | {} | {} | {} | {} |",
                i.name,
                fmt_ns(w.old.median_ns),
                fmt_ns(w.old.mad_ns),
                fmt_ns(w.new.median_ns),
                fmt_ns(w.new.mad_ns),
                fmt_ratio(&w.ratio),
                w.ratio.verdict().label()
            );
        }
        let a = &self.aggregate.wall;
        let _ = writeln!(s, "| **geomean of {}** | | | | | {} | {} |", self.aggregate.inputs, fmt_ratio(a), a.verdict().label());
        if let Some(c) = &self.aggregate.cpu {
            let _ = writeln!(s, "\nProcess CPU time, geomean new/old: {} ({}).", fmt_ratio(c), c.verdict().label());
        }
        s
    }
}

pub fn fmt_ratio(r: &Ratio) -> String {
    format!("{:.3} [{:.3}, {:.3}]", r.point, r.ci_lo, r.ci_hi)
}

pub fn fmt_ns(ns: f64) -> String {
    if ns >= 1e9 {
        format!("{:.3} s", ns / 1e9)
    } else if ns >= 1e6 {
        format!("{:.3} ms", ns / 1e6)
    } else if ns >= 1e3 {
        format!("{:.3} us", ns / 1e3)
    } else {
        format!("{ns:.0} ns")
    }
}
