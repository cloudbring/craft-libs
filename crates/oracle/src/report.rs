use crate::Outcome;
use serde::Serialize;
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

/// Outcome for one named input.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct FileResult {
    pub name: String,
    pub outcome: Outcome,
}

/// Which outcomes fail the run. `Regression`, `OnlyNewFails`, `Panic` (either side) and
/// `Timeout` of the new side always fail.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Policy {
    /// Also fail on `BothFail` (use when the corpus is expected to be clean).
    pub fail_on_both_fail: bool,
    /// Also fail when a registered expected difference matched nothing.
    pub fail_on_stale: bool,
}

impl Policy {
    pub fn is_failure(&self, o: &Outcome) -> bool {
        match o {
            Outcome::Regression { .. } | Outcome::OnlyNewFails { .. } | Outcome::Panic { .. } => true,
            Outcome::Timeout { side } => *side == crate::SideId::New,
            Outcome::BothFail { .. } => self.fail_on_both_fail,
            _ => false,
        }
    }
}

/// Full result of a run.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Report {
    pub total: usize,
    pub counts: BTreeMap<String, usize>,
    pub results: Vec<FileResult>,
    /// Ids of registry entries that matched no file.
    pub stale: Vec<String>,
}

impl Report {
    pub(crate) fn new(results: Vec<FileResult>, stale: Vec<String>) -> Self {
        let mut counts: BTreeMap<String, usize> = Outcome::NAMES.iter().map(|n| ((*n).to_string(), 0)).collect();
        for r in &results {
            *counts.entry(r.outcome.name().to_string()).or_insert(0) += 1;
        }
        Self { total: results.len(), counts, results, stale }
    }

    pub fn count(&self, name: &str) -> usize {
        self.counts.get(name).copied().unwrap_or(0)
    }

    /// Results the policy treats as failures.
    pub fn failures(&self, policy: &Policy) -> Vec<&FileResult> {
        self.results.iter().filter(|r| policy.is_failure(&r.outcome)).collect()
    }

    /// 0 when the run passes the policy, 1 otherwise.
    pub fn exit_code(&self, policy: &Policy) -> i32 {
        i32::from(!self.failures(policy).is_empty() || (policy.fail_on_stale && !self.stale.is_empty()))
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|e| format!("{{\"error\":\"{e}\"}}"))
    }

    /// Markdown summary: counts per outcome, worst cases, stale entries.
    pub fn to_markdown(&self, policy: &Policy) -> String {
        let mut s = String::from("# Differential report\n\n| Outcome | Files |\n| --- | ---: |\n");
        for n in Outcome::NAMES {
            let _ = writeln!(s, "| {n} | {} |", self.count(n));
        }
        let _ = writeln!(s, "| **Total** | {} |\n", self.total);
        let mut worst: Vec<&FileResult> = self.failures(policy);
        worst.sort_by(|a, b| severity(&b.outcome).total_cmp(&severity(&a.outcome)));
        if worst.is_empty() {
            s.push_str("No failing cases.\n\n");
        } else {
            s.push_str("## Worst cases\n\n");
            for r in worst.iter().take(10) {
                let _ = writeln!(s, "- `{}`: {}", r.name, describe(&r.outcome));
            }
            if worst.len() > 10 {
                let _ = writeln!(s, "- ... and {} more", worst.len() - 10);
            }
            s.push('\n');
        }
        if !self.stale.is_empty() {
            s.push_str("## Stale expected differences\n\nThese registry entries matched no file:\n\n");
            for id in &self.stale {
                let _ = writeln!(s, "- `{id}`");
            }
        }
        s
    }

    /// Write `report.json` and `report.md` into `dir`.
    pub fn write(&self, dir: &Path, policy: &Policy) -> std::io::Result<()> {
        std::fs::create_dir_all(dir)?;
        std::fs::write(dir.join("report.json"), self.to_json())?;
        std::fs::write(dir.join("report.md"), self.to_markdown(policy))
    }
}

fn severity(o: &Outcome) -> f64 {
    match o {
        Outcome::Regression { value, limit, .. } => {
            let r = if *limit > 0.0 { value / limit } else { *value };
            if r.is_nan() { f64::MAX } else { r }
        }
        _ => f64::MAX,
    }
}

fn describe(o: &Outcome) -> String {
    match o {
        Outcome::Regression { metric, value, limit } => format!("Regression, {metric} = {value} (limit {limit})"),
        Outcome::OnlyNewFails { new_err } => format!("OnlyNewFails: {new_err}"),
        Outcome::Panic { side, msg } => format!("Panic in {side:?}: {msg}"),
        Outcome::Timeout { side } => format!("Timeout in {side:?}"),
        Outcome::BothFail { old_err, new_err } => format!("BothFail: old `{old_err}`, new `{new_err}`"),
        other => other.name().to_string(),
    }
}
