//! Comparing two stored reports: the `--fail-over` regression gate.

use std::fmt::Write as _;

use crate::report::{MetricResult, Report, fmt_ns};
use crate::stats::geomean;

/// What the gate judges.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Basis {
    /// The `new/old` ratio. Both sides ran on one machine in each report, so machine speed
    /// cancels. Default.
    Ratio,
    /// The `new` side's median time. Only meaningful when both reports come from the same machine.
    NewMedian,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Clock {
    Wall,
    Cpu,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub input: String,
    /// Baseline and candidate value of the judged quantity (ratio, or ns).
    pub base: f64,
    pub cand: f64,
    /// `(cand / base - 1) * 100`; positive is worse.
    pub change_pct: f64,
    /// Change of the `old` side median, a sign the machine itself moved.
    pub old_drift_pct: f64,
    pub regressed: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Comparison {
    pub basis: Basis,
    pub clock: Clock,
    pub fail_over: Option<f64>,
    pub rows: Vec<Row>,
    /// Geometric-mean change over common inputs, percent.
    pub geomean_change_pct: f64,
    pub warnings: Vec<String>,
}

fn pick(r: &crate::report::InputResult, clock: Clock) -> Option<&MetricResult> {
    match clock {
        Clock::Wall => Some(&r.wall),
        Clock::Cpu => r.cpu.as_ref(),
    }
}

/// Compares `cand` against `base`. A row regresses when its change exceeds `fail_over` percent.
pub fn compare(base: &Report, cand: &Report, basis: Basis, clock: Clock, fail_over: Option<f64>) -> Comparison {
    let mut warnings = Vec::new();
    if base.fingerprint.cpu_model != cand.fingerprint.cpu_model || base.fingerprint.os != cand.fingerprint.os {
        warnings.push("reports come from different machines; absolute times are not comparable".to_string());
    }
    if base.contended || cand.contended {
        warnings.push("a report is marked contended".to_string());
    }
    let mut rows = Vec::new();
    for b in &base.inputs {
        let Some(c) = cand.inputs.iter().find(|c| c.name == b.name) else {
            warnings.push(format!("input {:?} is missing from the candidate", b.name));
            continue;
        };
        let (Some(bm), Some(cm)) = (pick(b, clock), pick(c, clock)) else {
            warnings.push(format!("input {:?} has no CPU time in one report", b.name));
            continue;
        };
        let (bv, cv) = match basis {
            Basis::Ratio => (bm.ratio.point, cm.ratio.point),
            Basis::NewMedian => (bm.new.median_ns, cm.new.median_ns),
        };
        let change_pct = (cv / bv - 1.0) * 100.0;
        rows.push(Row {
            input: b.name.clone(),
            base: bv,
            cand: cv,
            change_pct,
            old_drift_pct: (cm.old.median_ns / bm.old.median_ns.max(1.0) - 1.0) * 100.0,
            regressed: fail_over.is_some_and(|t| change_pct > t),
        });
    }
    for c in &cand.inputs {
        if !base.inputs.iter().any(|b| b.name == c.name) {
            warnings.push(format!("input {:?} is new in the candidate", c.name));
        }
    }
    let ratios: Vec<f64> = rows.iter().map(|r| r.cand / r.base).collect();
    let geomean_change_pct = (geomean(&ratios) - 1.0) * 100.0;
    Comparison { basis, clock, fail_over, rows, geomean_change_pct, warnings }
}

impl Comparison {
    /// True when any row regressed past the gate.
    pub fn failed(&self) -> bool {
        self.rows.iter().any(|r| r.regressed)
    }

    pub fn to_markdown(&self) -> String {
        let (unit, fmt): (&str, fn(f64) -> String) = match self.basis {
            Basis::Ratio => ("new/old", |v| format!("{v:.3}")),
            Basis::NewMedian => ("new median", fmt_ns),
        };
        let mut s = String::new();
        for w in &self.warnings {
            let _ = writeln!(s, "warning: {w}");
        }
        let _ = writeln!(s, "| input | baseline {unit} | candidate {unit} | change | old-side drift | |");
        let _ = writeln!(s, "|---|---:|---:|---:|---:|---|");
        for r in &self.rows {
            let flag = if r.regressed { "REGRESSION" } else { "" };
            let _ = writeln!(s, "| {} | {} | {} | {:+.1}% | {:+.1}% | {flag} |", r.input, fmt(r.base), fmt(r.cand), r.change_pct, r.old_drift_pct);
        }
        let _ = writeln!(s, "\nGeomean change: {:+.1}%.", self.geomean_change_pct);
        if let Some(t) = self.fail_over {
            let _ = writeln!(s, "Gate: fail over {t}%: {}.", if self.failed() { "FAILED" } else { "ok" });
        }
        s
    }
}
