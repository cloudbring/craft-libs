//! Corpus discovery, scenario running and the combined report.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::time::Duration;

use craft_compare::Verdict;
use craft_oracle::{Differential, FnJudge, FnSide, Outcome, Policy, Registry, Report};

pub const TREES: [&str; 3] = ["office-poi", "office-openxml-sdk", "office-libreoffice"];

#[derive(Default)]
pub struct Files {
    /// Starts with `PK` (any OOXML/zip).
    pub zip: Vec<(String, PathBuf)>,
    /// Starts with the compound-file signature.
    pub cfb: Vec<(String, PathBuf)>,
    /// Neither (kept for the count).
    pub other: Vec<(String, PathBuf)>,
}

pub fn discover(root: &Path) -> Files {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(rd) = std::fs::read_dir(dir) else { return };
        let mut entries: Vec<_> = rd.filter_map(Result::ok).map(|e| e.path()).collect();
        entries.sort();
        for p in entries {
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
    let mut all = Vec::new();
    for t in TREES {
        walk(&root.join(t), &mut all);
    }
    let mut f = Files::default();
    for p in all {
        let rel = p.strip_prefix(root).unwrap_or(&p).to_string_lossy().into_owned();
        let mut head = [0u8; 8];
        let n = std::fs::File::open(&p).and_then(|mut h| std::io::Read::read(&mut h, &mut head)).unwrap_or(0);
        if n >= 4 && head.starts_with(b"PK") {
            f.zip.push((rel, p));
        } else if n == 8 && head == crate::cfbside::MAGIC {
            f.cfb.push((rel, p));
        } else {
            f.other.push((rel, p));
        }
    }
    f
}

pub fn read_lazy(files: &[(String, PathBuf)]) -> impl Iterator<Item = (String, Vec<u8>)> + '_ {
    files.iter().map(|(n, p)| (n.clone(), std::fs::read(p).unwrap_or_default()))
}

pub struct Run {
    pub scenario: String,
    pub app: &'static str,
    pub format: &'static str,
    pub report: Report,
}

pub struct Suite {
    pub registry: Registry,
    pub policy: Policy,
    pub runs: Vec<Run>,
    pub out: PathBuf,
    pub timeout: Duration,
}

impl Suite {
    pub fn new(registry: Registry, out: PathBuf) -> Self {
        std::fs::create_dir_all(&out).expect("output dir");
        Self { registry, policy: Policy::default(), runs: vec![], out, timeout: Duration::from_secs(120) }
    }

    /// Run one scenario over `inputs` (names are prefixed with the scenario so the registry's globs can
    /// target it).
    pub fn run<O, I>(
        &mut self,
        scenario: &str,
        app: &'static str,
        format: &'static str,
        inputs: I,
        old: fn(&[u8]) -> Result<O, String>,
        new: fn(&[u8]) -> Result<O, String>,
        judge: fn(&O, &O) -> Verdict,
    ) where
        O: Send + 'static,
        I: IntoIterator<Item = (String, Vec<u8>)>,
    {
        if let Ok(only) = std::env::var("CONTAINER_ORACLE_ONLY") {
            if !only.split(',').any(|k| !k.is_empty() && scenario.contains(k)) {
                return;
            }
        }
        let diff = Differential::new(FnSide(old), FnSide(new), FnJudge(judge))
            .timeout(self.timeout)
            .expected(self.registry.clone())
            .policy(self.policy);
        let report = diff.run_files(inputs.into_iter().map(|(n, b)| (format!("{scenario}/{n}"), b)));
        eprintln!("{scenario}: {}", report.counts.iter().filter(|(_, v)| **v > 0).map(|(k, v)| format!("{k} {v}")).collect::<Vec<_>>().join(", "));
        self.runs.push(Run { scenario: scenario.to_string(), app, format, report });
    }

    /// Registry entries that no run matched.
    pub fn stale(&self) -> Vec<String> {
        let mut stale: Option<Vec<String>> = None;
        for r in &self.runs {
            stale = Some(match stale {
                None => r.report.stale.clone(),
                Some(s) => s.into_iter().filter(|id| r.report.stale.contains(id)).collect(),
            });
        }
        stale.unwrap_or_default()
    }

    pub fn failures(&self) -> usize {
        self.runs.iter().map(|r| r.report.failures(&self.policy).len()).sum()
    }

    /// Markdown tables: per scenario outcome counts; per format x outcome; expected ids; failures.
    pub fn markdown(&self) -> String {
        let mut s = String::new();
        let names = Outcome::NAMES;
        let _ = writeln!(s, "| scenario | app | files | {} |", names.join(" | "));
        let _ = writeln!(s, "|---|---|---:|{}|", names.iter().map(|_| "---:").collect::<Vec<_>>().join("|"));
        for r in &self.runs {
            let _ = writeln!(s, "| {} | {} | {} | {} |", r.scenario, r.app, r.report.total, names.iter().map(|n| r.report.count(n).to_string()).collect::<Vec<_>>().join(" | "));
        }
        let mut ids: BTreeMap<(String, String), usize> = BTreeMap::new();
        for r in &self.runs {
            for f in &r.report.results {
                if let Outcome::ExpectedDifference { id } = &f.outcome {
                    *ids.entry((r.scenario.clone(), id.clone())).or_default() += 1;
                }
            }
        }
        let _ = writeln!(s, "\n| scenario | expected-difference id | files |\n|---|---|---:|");
        for ((sc, id), n) in &ids {
            let _ = writeln!(s, "| {sc} | {id} | {n} |");
        }
        let stale = self.stale();
        let _ = writeln!(s, "\nstale registry entries: {}", if stale.is_empty() { "none".to_string() } else { stale.join(", ") });
        let _ = writeln!(s, "\nfailures (Regression, OnlyNewFails, Panic): {}", self.failures());
        s
    }

    /// One line per non-identical file with its outcome, and metric tallies.
    pub fn details(&self) -> String {
        let mut s = String::new();
        for r in &self.runs {
            let mut tally: BTreeMap<String, usize> = BTreeMap::new();
            let _ = writeln!(s, "\n=== {} ({}, {}) {} files", r.scenario, r.app, r.format, r.report.total);
            let mut bf_same = 0usize;
            let mut bf_diff = 0usize;
            for f in &r.report.results {
                let key = match &f.outcome {
                    Outcome::Identical => continue,
                    Outcome::WithinTolerance { metric, .. } => format!("Within {metric}"),
                    Outcome::ExpectedDifference { id } => format!("Expected {id}"),
                    Outcome::Regression { metric, .. } => format!("REGRESSION {metric}"),
                    Outcome::BothFail { old_err, new_err } => {
                        if old_err == new_err {
                            bf_same += 1;
                        } else {
                            bf_diff += 1;
                        }
                        continue;
                    }
                    o => format!("{} ", o.name()),
                };
                *tally.entry(key.clone()).or_default() += 1;
                let _ = writeln!(s, "{} [{key}] {:?}", f.name, f.outcome);
            }
            let _ = writeln!(s, "both-fail with identical messages: {bf_same}; with different messages: {bf_diff}");
            for (k, v) in &tally {
                let _ = writeln!(s, "  {k}: {v}");
            }
        }
        s
    }

    pub fn write(&self) {
        for (name, text) in [("report.md", self.markdown()), ("details.txt", self.details())] {
            if let Err(e) = std::fs::write(self.out.join(name), text) {
                eprintln!("cannot write {}: {e}", self.out.join(name).display());
            }
        }
        let json: Vec<_> = self.runs.iter().map(|r| serde_json::json!({"scenario": r.scenario, "app": r.app, "format": r.format, "report": serde_json::from_str::<serde_json::Value>(&r.report.to_json()).unwrap_or_default()})).collect();
        let _ = std::fs::write(self.out.join("report.json"), serde_json::to_string(&json).unwrap_or_default());
    }
}
