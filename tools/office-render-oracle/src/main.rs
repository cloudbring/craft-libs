//! LibreOffice headless render oracle for DOCX/XLSX/PPTX (artcraft-algorithms#194).
//!
//! LibreOffice's PDF is a reference, not ground truth: every number here is agreement with it.
//!
//! ```text
//! office-render-oracle sample --manifests M --corpus C --seed 194 --out sample.json
//! office-render-oracle run    --sample sample.json --work W --bin-dir B --out results.jsonl
//! office-render-oracle one    FILE --work W --bin-dir B
//! office-render-oracle report --results results.jsonl [--baseline baseline.json]
//! office-render-oracle triage --results results.jsonl --work W --out DIR [--n 10]
//! ```

mod report;
mod run;
mod sample;
mod triage;

use std::collections::HashMap;

/// One corpus file chosen for the sample.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct Entry {
    pub id: String,
    pub source: String,
    pub rel: String,
    pub path: String,
    pub ext: String,
    pub app: String,
    pub size: u64,
    pub tags: Vec<String>,
    pub stratum: String,
}

/// App owning an extension.
pub fn app_for(ext: &str) -> Option<&'static str> {
    match ext {
        "docx" => Some("wordcraft"),
        "xlsx" => Some("gridcraft"),
        "pptx" => Some("deckcraft"),
        _ => None,
    }
}

/// The feature tags the report slices by.
pub const FOCUS_TAGS: [&str; 8] = ["tables", "charts", "rtl", "fields", "math", "tracked-changes", "images", "large"];

fn flags(args: &[String]) -> (Vec<String>, HashMap<String, String>) {
    let (mut pos, mut map) = (Vec::new(), HashMap::new());
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if let Some(k) = a.strip_prefix("--") {
            map.insert(k.to_string(), it.next().cloned().unwrap_or_default());
        } else {
            pos.push(a.clone());
        }
    }
    (pos, map)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some((cmd, rest)) = args.split_first() else {
        eprintln!("usage: office-render-oracle sample|run|one|report|triage ... (see src/main.rs)");
        std::process::exit(2);
    };
    let (pos, f) = flags(rest);
    let get = |k: &str| f.get(k).cloned().ok_or_else(|| format!("missing --{k}"));
    let res = match cmd.as_str() {
        "sample" => (|| {
            let seed = f.get("seed").map_or(Ok(194), |s| s.parse::<u64>()).map_err(|e| e.to_string())?;
            let n = |k: &str, d: usize| f.get(k).map_or(d, |s| s.parse().unwrap_or(d));
            sample::run(&get("manifests")?, &get("corpus")?, seed, [n("docx", 150), n("xlsx", 100), n("pptx", 100)], &get("out")?)
        })(),
        "run" => (|| {
            run::run_all(&get("sample")?, &get("work")?, &get("bin-dir")?, &get("out")?, f.get("timeout").and_then(|s| s.parse().ok()).unwrap_or(90))
        })(),
        "one" => (|| run::one(pos.first().ok_or("one FILE")?, &get("work")?, &get("bin-dir")?))(),
        "report" => (|| report::run(&get("results")?, f.get("baseline").map(String::as_str)))(),
        "triage" => (|| triage::run(&get("results")?, &get("work")?, &get("out")?, f.get("n").and_then(|s| s.parse().ok()).unwrap_or(10)))(),
        other => Err(format!("unknown command {other}")),
    };
    if let Err(e) = res {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}
