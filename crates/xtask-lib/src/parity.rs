// Provenance: designcraft/xtask/src/parity.rs and deckcraft/xtask/src/parity.rs (weighted checklist).
// Command-output mode: wordcraft/xtask/src/parity.rs.
use crate::{Cargo, Result};
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct ParityConfig {
    pub cargo: Cargo,
    pub checklist_path: PathBuf,
    pub summary_marker: String,
    pub rows_marker: String,
    /// An app-owned report generator, e.g. ["run", "-q", "-p", "wordcraft-cli", "--", "parity", "--markdown"].
    pub command: Option<Vec<String>>,
}

impl Default for ParityConfig {
    fn default() -> Self {
        Self {
            cargo: Cargo::default(),
            checklist_path: "docs/parity-checklist.md".into(),
            summary_marker: "<!-- SUMMARY -->".into(),
            rows_marker: "\n## Rows".into(),
            command: None,
        }
    }
}

pub type Config = ParityConfig;

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct Args {
    pub check: bool,
}

impl Args {
    pub fn parse(args: &[String]) -> Result<Self> {
        match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
            [] => Ok(Self::default()),
            ["--check"] => Ok(Self { check: true }),
            _ => Err(format!("usage: {}", cli_help())),
        }
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Report {
    pub rows: Vec<(String, String, u8, f64)>,
    /// Weighted completion percentage, in the range 0–100.
    pub weighted_score: f64,
    pub path: PathBuf,
    pub updated: String,
    pub stale: bool,
}

pub fn render(report: &Report) -> String {
    report.updated.clone()
}

pub fn apply(report: &Report) -> Result {
    crate::write(&report.path, &report.updated)
}

pub fn cli_help() -> &'static str {
    "parity [--check]"
}

pub fn rewrite(cfg: &Config, text: &str) -> Result<String> {
    if cfg.summary_marker.is_empty() || cfg.rows_marker.is_empty() {
        return Err("parity markers must not be empty".into());
    }
    let start = text.find(&cfg.summary_marker).ok_or("missing summary marker")?;
    let end = text.find(&cfg.rows_marker).ok_or("missing rows marker")?;
    if start + cfg.summary_marker.len() > end {
        return Err("summary marker must precede rows".into());
    }
    Ok(format!("{}{}\n{}{}", &text[..start], cfg.summary_marker, summary(&rows(&text[end..])), &text[end..]))
}

pub fn run(cfg: &Config, args: &[String]) -> Result {
    let args = Args::parse(args)?;
    let report = compute(cfg, &args)?;
    if !args.check {
        apply(&report)?;
    }
    print!("{}", render(&report));
    Ok(())
}

/// Compute the replacement checklist without writing it.
pub fn compute(cfg: &Config, args: &Args) -> Result<Report> {
    let path = cfg.cargo.root.join(&cfg.checklist_path);
    let updated = if let Some(command) = &cfg.command {
        if command.is_empty() {
            return Err("parity command must not be empty".into());
        }
        String::from_utf8(cfg.cargo.output(&command.iter().map(String::as_str).collect::<Vec<_>>())?.stdout).map_err(|error| error.to_string())?
    } else {
        rewrite(cfg, &crate::read(&path)?)?
    };
    let stale = if args.check { crate::read(&path)? != updated } else { std::fs::read_to_string(&path).map_or(true, |text| text != updated) };
    if args.check && stale {
        return Err("parity checklist is stale".into());
    }
    let row_text = if cfg.command.is_none() { updated.find(&cfg.rows_marker).map_or(updated.as_str(), |start| &updated[start..]) } else { &updated };
    let rows = rows(row_text);
    let weighted_score = percentage(&rows.iter().collect::<Vec<_>>());
    Ok(Report { rows, weighted_score, path, updated, stale })
}
/// (area, feature, priority 0–2, value 0 / 0.5 / 1) for each table row.
pub fn rows(text: &str) -> Vec<(String, String, u8, f64)> {
    let mut out = Vec::new();
    for l in text.lines() {
        let cells: Vec<&str> = l.trim().trim_matches('|').split('|').map(str::trim).collect();
        if cells.len() != 4 {
            continue;
        }
        let pri = match cells[2] {
            "P0" => 0,
            "P1" => 1,
            "P2" => 2,
            _ => continue,
        };
        let v = match cells[3] {
            "D" => 1.0,
            "P" => 0.5,
            "M" => 0.0,
            _ => continue,
        };
        out.push((cells[0].to_string(), cells[1].to_string(), pri, v));
    }
    out
}

fn weight(p: u8) -> f64 {
    [3.0, 2.0, 1.0].get(p as usize).copied().unwrap_or(0.0)
}

fn percentage(rows: &[&(String, String, u8, f64)]) -> f64 {
    let total: f64 = rows.iter().map(|row| weight(row.2)).sum();
    if total == 0.0 { 0.0 } else { 100.0 * rows.iter().map(|row| weight(row.2) * row.3).sum::<f64>() / total }
}

pub fn summary(rows: &[(String, String, u8, f64)]) -> String {
    let all: Vec<_> = rows.iter().collect();
    let mut s = format!(
        "**Weighted breadth parity: {:.0}%** over {} features.\n\n| Priority | Features | Done | Partial | Missing | Parity |\n|---|---|---|---|---|---|\n",
        percentage(&all),
        rows.len()
    );
    for p in 0..3u8 {
        let rr: Vec<_> = rows.iter().filter(|r| r.2 == p).collect();
        let n = |v: f64| rr.iter().filter(|r| r.3 == v).count();
        let unweighted = if rr.is_empty() { 0.0 } else { 100.0 * rr.iter().map(|r| r.3).sum::<f64>() / rr.len() as f64 };
        s += &format!("| P{p} | {} | {} | {} | {} | {unweighted:.0}% |\n", rr.len(), n(1.0), n(0.5), n(0.0));
    }
    s += "\n| Area | Parity |\n|---|---|\n";
    let mut areas: Vec<&str> = Vec::new();
    for r in rows {
        if !areas.contains(&r.0.as_str()) {
            areas.push(&r.0);
        }
    }
    for a in areas {
        let rr: Vec<_> = rows.iter().filter(|r| r.0 == a).collect();
        s += &format!("| {a} | {:.0}% |\n", percentage(&rr));
    }
    let open: Vec<&str> = rows.iter().filter(|r| r.2 == 0 && r.3 < 1.0).map(|r| r.1.as_str()).collect();
    s += &format!("\nOpen P0 items: {}.\n", open.join(", "));
    s
}
