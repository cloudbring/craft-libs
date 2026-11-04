//! `craft-bench compare BASE.json CAND.json [--fail-over PCT] [--cpu] [--absolute] [--allow-contended]`
//! and `craft-bench show REPORT.json`.
//!
//! Exit codes: 0 ok, 1 regression past the gate, 2 usage or unreadable/unpublishable input.

use std::process::ExitCode;

use craft_bench::Report;
use craft_bench::compare::{Basis, Clock, compare};

const USAGE: &str =
    "usage:\n  craft-bench compare BASE.json CAND.json [--fail-over PCT] [--cpu] [--absolute] [--allow-contended]\n  craft-bench show REPORT.json";

fn load(path: &str) -> Result<Report, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
    Report::from_json(&text).map_err(|e| format!("{path}: {e}"))
}

fn run(args: &[String]) -> Result<bool, String> {
    match args.first().map(String::as_str) {
        Some("show") => {
            let path = args.get(1).ok_or(USAGE)?;
            print!("{}", load(path)?.to_markdown());
            Ok(true)
        }
        Some("compare") => run_compare(&args[1..]),
        _ => Err(USAGE.to_string()),
    }
}

fn run_compare(args: &[String]) -> Result<bool, String> {
    let (mut paths, mut fail_over, mut clock, mut basis, mut allow) = (Vec::new(), None, Clock::Wall, Basis::Ratio, false);
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--fail-over" => {
                let v = it.next().ok_or("--fail-over needs a value")?;
                fail_over = Some(v.parse::<f64>().map_err(|e| format!("--fail-over {v}: {e}"))?);
            }
            "--cpu" => clock = Clock::Cpu,
            "--absolute" => basis = Basis::NewMedian,
            "--allow-contended" => allow = true,
            s if s.starts_with("--") => return Err(format!("unknown flag {s}\n{USAGE}")),
            s => paths.push(s),
        }
    }
    let [base_path, cand_path] = paths[..] else { return Err(USAGE.to_string()) };
    let (base, cand) = (load(base_path)?, load(cand_path)?);
    if !allow {
        for (p, r) in [(base_path, &base), (cand_path, &cand)] {
            if r.contended {
                return Err(format!("{p} is contended ({}); pass --allow-contended to compare anyway", r.guard.reasons.join("; ")));
            }
        }
    }
    let c = compare(&base, &cand, basis, clock, fail_over);
    print!("{}", c.to_markdown());
    Ok(!c.failed())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(1),
        Err(e) => {
            eprintln!("{e}");
            ExitCode::from(2)
        }
    }
}
