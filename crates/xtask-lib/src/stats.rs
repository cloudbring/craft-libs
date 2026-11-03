// Provenance: lightcraft/xtask/src/stats.rs (counts/report), cadcraft and gridcraft/xtask/src/stats.rs (bounded walks).
use std::collections::BTreeSet;
use std::path::Path;

use crate::{Cargo, Result};

#[derive(Debug, Clone)]
pub struct StatsConfig {
    pub cargo: Cargo,
    pub excluded_dirs: Vec<String>,
    pub follow_symlinks: bool,
    pub max_entries: usize,
    pub max_directories: usize,
    pub fail_on_budget: bool,
}

impl Default for StatsConfig {
    fn default() -> Self {
        Self {
            cargo: Cargo::default(),
            excluded_dirs: vec!["target".into(), "fuzz".into(), ".*".into()],
            follow_symlinks: false,
            max_entries: 2_000_000,
            max_directories: 10_000,
            fail_on_budget: true,
        }
    }
}

pub type Config = StatsConfig;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct Counts {
    pub files: usize,
    pub lines: usize,
    pub tests: usize,
    pub proptests: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct Args {
    pub exact: bool,
}

impl Args {
    pub fn parse(args: &[String]) -> Result<Self> {
        match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
            [] => Ok(Self::default()),
            ["--exact"] => Ok(Self { exact: true }),
            _ => Err(format!("usage: {}", cli_help())),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Row {
    pub name: String,
    pub counts: Counts,
    pub harness: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Report {
    pub rows: Vec<Row>,
    pub exact: bool,
    pub total: Counts,
    pub total_harness: usize,
}

pub fn compute(cfg: &Config, args: &Args) -> Result<Report> {
    from_metadata(cfg, &cfg.cargo.metadata(false)?, args)
}

pub fn count_tests(src: &str) -> (usize, usize) {
    let mut tests = 0;
    let mut props = 0;
    for line in src.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("//") {
            continue;
        }
        tests += trimmed.matches("#[test]").count();
        if trimmed.starts_with("proptest!") {
            props += 1;
        }
    }
    (tests, props)
}

pub fn count(cfg: &Config, dir: &Path) -> Result<Counts> {
    let mut counts = Counts::default();
    let mut pending = vec![dir.to_path_buf()];
    let mut seen = BTreeSet::new();
    let mut entries = cfg.max_entries;
    let mut directories = cfg.max_directories;
    while let Some(dir) = pending.pop() {
        let Ok(identity) = dir.canonicalize() else { continue };
        if !seen.insert(identity) {
            continue;
        }
        if directories == 0 {
            return exhausted(cfg, counts);
        }
        directories -= 1;
        let Ok(read) = std::fs::read_dir(&dir) else { continue };
        let mut children: Vec<_> = read.flatten().collect();
        children.sort_by_key(|entry| entry.file_name());
        for entry in children {
            if entries == 0 {
                return exhausted(cfg, counts);
            }
            entries -= 1;
            let Ok(kind) = entry.file_type() else { continue };
            if kind.is_symlink() && !cfg.follow_symlinks {
                continue;
            }
            let path = entry.path();
            if path.is_dir() {
                if !crate::matches(&cfg.excluded_dirs, &entry.file_name().to_string_lossy()) {
                    pending.push(path);
                }
            } else if path.extension().is_some_and(|extension| extension == "rs")
                && path.is_file()
                && let Ok(text) = std::fs::read_to_string(path)
            {
                let (tests, proptests) = count_tests(&text);
                counts.files += 1;
                counts.lines += text.lines().count();
                counts.tests += tests;
                counts.proptests += proptests;
            }
        }
    }
    Ok(counts)
}

fn exhausted(cfg: &Config, counts: Counts) -> Result<Counts> {
    if cfg.fail_on_budget { Err("stats traversal budget exhausted".into()) } else { Ok(counts) }
}

/// The original table, returned as text so callers can capture or test it without global stdout hooks.
pub fn report(cfg: &Config, metadata: &serde_json::Value, exact: bool) -> Result<String> {
    Ok(render(&from_metadata(cfg, metadata, &Args { exact })?))
}

pub fn from_metadata(cfg: &Config, metadata: &serde_json::Value, args: &Args) -> Result<Report> {
    let packages = metadata["packages"].as_array().ok_or("metadata: no packages array")?;
    let mut rows = Vec::new();
    for package in packages {
        let name = package["name"].as_str().ok_or("package without name")?;
        let path = package["manifest_path"].as_str().ok_or("package without manifest_path")?;
        let path = cfg.cargo.root.join(path);
        let dir = path.parent().ok_or("manifest without parent")?;
        let counts = count(cfg, dir)?;
        let harness = if args.exact {
            cfg.cargo
                .output(&["test", "-q", "-p", name, "--", "--list"])
                .ok()
                .map(|output| String::from_utf8_lossy(&output.stdout).lines().filter(|line| line.ends_with(": test")).count())
        } else {
            None
        };
        rows.push(Row { name: name.to_owned(), counts, harness });
    }
    rows.sort_by(|left, right| left.name.cmp(&right.name));
    let mut total = Counts::default();
    for row in &rows {
        total.files += row.counts.files;
        total.lines += row.counts.lines;
        total.tests += row.counts.tests;
        total.proptests += row.counts.proptests;
    }
    let total_harness = rows.iter().filter_map(|row| row.harness).sum();
    Ok(Report { rows, exact: args.exact, total, total_harness })
}

pub fn render(report: &Report) -> String {
    let exact = report.exact;
    let mut output = String::new();
    let exact_col = if exact { format!(" {:>8}", "harness") } else { String::new() };
    let line = "-".repeat(62 + if exact { 9 } else { 0 });
    output.push_str(&format!("{:<26} {:>6} {:>8} {:>7} {:>10}{exact_col}\n", "crate", "files", "lines", "#[test]", "proptest!"));
    output.push_str(&format!("{line}\n"));
    for Row { name, counts, harness } in &report.rows {
        let column = match (exact, harness) {
            (false, _) => String::new(),
            (true, Some(count)) => {
                format!(" {count:>8}")
            }
            (true, None) => format!(" {:>8}", "error"),
        };
        output.push_str(&format!("{name:<26} {:>6} {:>8} {:>7} {:>10}{column}\n", counts.files, counts.lines, counts.tests, counts.proptests));
    }
    let total = report.total;
    let total_exact = report.total_harness;
    let column = if exact { format!(" {total_exact:>8}") } else { String::new() };
    output.push_str(&format!("{line}\n{:<26} {:>6} {:>8} {:>7} {:>10}{column}\n", "TOTAL", total.files, total.lines, total.tests, total.proptests));
    if !exact {
        output.push_str("\n(#[test] counts source attributes; macro-generated tests count once. Use --exact for harness counts.)\n");
    }
    output
}

pub fn cli_help() -> &'static str {
    "stats [--exact]"
}

pub fn run(cfg: &Config, args: &[String]) -> Result {
    print!("{}", render(&compute(cfg, &Args::parse(args)?)?));
    Ok(())
}
