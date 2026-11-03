// Provenance: lightcraft/xtask/src/stats.rs (counts/report), cadcraft and gridcraft/xtask/src/stats.rs (bounded walks).
use std::collections::BTreeSet;
use std::fmt::Write;
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

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Counts {
    pub files: usize,
    pub lines: usize,
    pub tests: usize,
    pub proptests: usize,
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
    let packages = metadata["packages"].as_array().ok_or("metadata: no packages array")?;
    let mut rows = Vec::new();
    for package in packages {
        let name = package["name"].as_str().ok_or("package without name")?;
        let path = package["manifest_path"].as_str().ok_or("package without manifest_path")?;
        let path = cfg.cargo.root.join(path);
        let dir = path.parent().ok_or("manifest without parent")?;
        let counts = count(cfg, dir)?;
        let harness = if exact {
            cfg.cargo
                .output(&["test", "-q", "-p", name, "--", "--list"])
                .ok()
                .map(|output| String::from_utf8_lossy(&output.stdout).lines().filter(|line| line.ends_with(": test")).count())
        } else {
            None
        };
        rows.push((name, counts, harness));
    }
    rows.sort_by_key(|row| row.0);
    render(&rows, exact)
}

fn render(rows: &[(&str, Counts, Option<usize>)], exact: bool) -> Result<String> {
    let mut output = String::new();
    let exact_col = if exact { format!(" {:>8}", "harness") } else { String::new() };
    let line = "-".repeat(62 + if exact { 9 } else { 0 });
    writeln!(output, "{:<26} {:>6} {:>8} {:>7} {:>10}{exact_col}", "crate", "files", "lines", "#[test]", "proptest!")
        .map_err(|error| error.to_string())?;
    writeln!(output, "{line}").map_err(|error| error.to_string())?;
    let mut total = Counts::default();
    let mut total_exact = 0;
    for (name, counts, harness) in rows {
        let column = match (exact, harness) {
            (false, _) => String::new(),
            (true, Some(count)) => {
                total_exact += count;
                format!(" {count:>8}")
            }
            (true, None) => format!(" {:>8}", "error"),
        };
        writeln!(output, "{name:<26} {:>6} {:>8} {:>7} {:>10}{column}", counts.files, counts.lines, counts.tests, counts.proptests)
            .map_err(|error| error.to_string())?;
        total.files += counts.files;
        total.lines += counts.lines;
        total.tests += counts.tests;
        total.proptests += counts.proptests;
    }
    let column = if exact { format!(" {total_exact:>8}") } else { String::new() };
    writeln!(output, "{line}\n{:<26} {:>6} {:>8} {:>7} {:>10}{column}", "TOTAL", total.files, total.lines, total.tests, total.proptests)
        .map_err(|error| error.to_string())?;
    if !exact {
        output.push_str("\n(#[test] counts source attributes; macro-generated tests count once. Use --exact for harness counts.)\n");
    }
    Ok(output)
}

pub fn cli_help() -> &'static str {
    "stats [--exact]"
}

pub fn run(cfg: &Config, args: &[String]) -> Result {
    let exact = match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        [] => false,
        ["--exact"] => true,
        _ => return Err(format!("usage: {}", cli_help())),
    };
    print!("{}", report(cfg, &cfg.cargo.metadata(false)?, exact)?);
    Ok(())
}
