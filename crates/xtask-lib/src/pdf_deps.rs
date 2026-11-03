// Provenance: vectorcraft, designcraft, deckcraft and effectcraft/xtask/src/pdf_deps.rs.
use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{Cargo, Result};

#[derive(Debug, Clone, Default)]
pub struct PdfDepsConfig {
    pub cargo: Cargo,
    pub packages: Vec<PackagePin>,
    pub forbidden_active: Vec<String>,
    pub production_roots: Vec<String>,
    pub forbidden_production: Vec<String>,
    pub target_features: Vec<TargetFeatures>,
}

pub type Config = PdfDepsConfig;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Args;

impl Args {
    pub fn parse(args: &[String]) -> Result<Self> {
        if args.is_empty() { Ok(Self) } else { Err(format!("usage: {}", cli_help())) }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Package {
    pub name: String,
    pub version: String,
    pub source: String,
    pub features: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TargetReport {
    pub root_package: String,
    pub target: String,
    pub package: String,
    pub features: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Report {
    pub packages: Vec<Package>,
    pub production_roots: Vec<String>,
    pub target_features: Vec<TargetReport>,
}

pub fn render(_report: &Report) -> String {
    "PDF dependencies: pins, features and production graph verified\n".into()
}

pub fn compute(cfg: &Config, _args: &Args) -> Result<Report> {
    let mut report = from_metadata(cfg, &cfg.cargo.metadata(true)?)?;
    for rule in &cfg.target_features {
        report.target_features.push(check_target(&cfg.cargo, rule)?);
    }
    Ok(report)
}

/// Validate resolved pins and production reachability in an already loaded graph.
/// Target-specific checks require `compute`, which invokes Cargo for each target.
pub fn from_metadata(cfg: &Config, metadata: &Value) -> Result<Report> {
    validate(cfg, metadata)?;
    let packages = metadata["packages"].as_array().ok_or("metadata: no packages")?;
    let mut checked = Vec::new();
    for pin in &cfg.packages {
        let package = one(packages, &pin.name)?;
        let features = metadata
            .pointer("/resolve/nodes")
            .and_then(Value::as_array)
            .and_then(|nodes| nodes.iter().find(|node| node["id"] == package["id"]))
            .and_then(|node| node["features"].as_array())
            .map(|features| features.iter().filter_map(Value::as_str).map(str::to_owned).collect())
            .unwrap_or_default();
        checked.push(Package { name: pin.name.clone(), version: pin.version.clone(), source: pin.source.clone(), features });
    }
    Ok(Report { packages: checked, production_roots: cfg.production_roots.clone(), target_features: Vec::new() })
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PackagePin {
    pub name: String,
    pub version: String,
    pub source: String,
    #[serde(default)]
    pub required_features: Vec<String>,
    pub exact_features: Option<Vec<String>>,
}

#[derive(Debug, Clone)]
pub struct TargetFeatures {
    pub root_package: String,
    /// None selects rustc's host triple; Some selects an explicit target.
    pub target: Option<String>,
    pub package: String,
    pub features: Vec<String>,
    pub depth: usize,
}

pub fn git_source(repository: &str, revision: &str) -> String {
    format!("git+{repository}?rev={revision}#{revision}")
}

fn one<'a>(packages: &'a [Value], name: &str) -> Result<&'a Value> {
    let matching: Vec<_> = packages.iter().filter(|package| package["name"].as_str() == Some(name)).collect();
    match matching.as_slice() {
        [package] => Ok(package),
        _ => Err(format!("{name}: expected one resolved package, found {}", matching.len())),
    }
}

pub fn validate(cfg: &Config, metadata: &Value) -> Result {
    if cfg.packages.is_empty() {
        return Err("pdf_deps: configure package pins".into());
    }
    let packages = metadata["packages"].as_array().ok_or("metadata: no packages")?;
    for pin in &cfg.packages {
        validate_pin(pin, packages, metadata)?;
    }
    if !cfg.forbidden_active.is_empty() || !cfg.production_roots.is_empty() {
        let nodes = metadata.pointer("/resolve/nodes").and_then(Value::as_array).ok_or("metadata: no resolved nodes")?;
        for package in packages {
            let name = package["name"].as_str().ok_or("package without name")?;
            if crate::matches(&cfg.forbidden_active, name) && nodes.iter().any(|node| node["id"] == package["id"]) {
                return Err(format!("{name}: forbidden active PDF dependency"));
            }
        }
        for root in &cfg.production_roots {
            validate_production(cfg, root, packages, nodes)?;
        }
    }
    Ok(())
}

fn validate_pin(pin: &PackagePin, packages: &[Value], metadata: &Value) -> Result {
    let package = one(packages, &pin.name)?;
    if package["version"].as_str() != Some(&pin.version) || package["source"].as_str() != Some(&pin.source) {
        return Err(format!("{}: expected {} from {}; got {package}", pin.name, pin.version, pin.source));
    }
    if pin.exact_features.is_some() || !pin.required_features.is_empty() {
        let id = package["id"].as_str().ok_or("package without id")?;
        let nodes = metadata.pointer("/resolve/nodes").and_then(Value::as_array).ok_or("metadata: no resolved nodes")?;
        let node = nodes.iter().find(|node| node["id"].as_str() == Some(id)).ok_or_else(|| format!("{}: missing resolved node", pin.name))?;
        let features = node["features"].as_array().ok_or("node without features")?;
        let actual: BTreeSet<_> =
            features.iter().map(|feature| feature.as_str().ok_or("non-string feature")).collect::<std::result::Result<_, _>>()?;
        if pin.required_features.iter().any(|feature| !actual.contains(feature.as_str())) {
            return Err(format!("{}: missing required features", pin.name));
        }
        if let Some(expected) = &pin.exact_features
            && actual != expected.iter().map(String::as_str).collect()
        {
            return Err(format!("{}: expected exact features {expected:?}, got {actual:?}", pin.name));
        }
    }
    Ok(())
}

fn validate_production(cfg: &Config, root: &str, packages: &[Value], nodes: &[Value]) -> Result {
    let root = one(packages, root)?["id"].as_str().ok_or("root without id")?;
    let mut pending = vec![root];
    let mut seen = BTreeSet::new();
    while let Some(id) = pending.pop() {
        if !seen.insert(id) {
            continue;
        }
        let package = packages.iter().find(|package| package["id"].as_str() == Some(id)).ok_or_else(|| format!("unresolved package {id}"))?;
        let name = package["name"].as_str().ok_or("package without name")?;
        if crate::matches(&cfg.forbidden_production, name) {
            return Err(format!("unexpected production reader dependency {name}"));
        }
        let node = nodes.iter().find(|node| node["id"].as_str() == Some(id)).ok_or_else(|| format!("no resolved node for {id}"))?;
        for dep in node["deps"].as_array().ok_or("node without deps")? {
            let kinds = dep["dep_kinds"].as_array().filter(|kinds| !kinds.is_empty()).ok_or("dependency has no kinds")?;
            if kinds.iter().any(|kind| kind["kind"].as_str() != Some("dev")) {
                pending.push(dep["pkg"].as_str().ok_or("dependency without package id")?);
            }
        }
    }
    Ok(())
}

pub fn validate_target_features(rule: &TargetFeatures, tree: &str) -> Result {
    let prefix = format!("{} v", rule.package);
    let mut found = false;
    let expected: BTreeSet<_> = rule.features.iter().map(String::as_str).collect();
    for (package, features) in tree.lines().filter_map(|line| line.split_once('|')).filter(|(package, _)| package.starts_with(&prefix)) {
        found = true;
        let actual: BTreeSet<_> = features.split(',').filter(|feature| !feature.is_empty()).collect();
        if actual != expected {
            return Err(format!("{package}: expected features {expected:?}, got {actual:?}"));
        }
    }
    if !found {
        return Err(format!("missing {} in target dependency graph", rule.package));
    }
    Ok(())
}

fn check_target(cargo: &Cargo, rule: &TargetFeatures) -> Result<TargetReport> {
    let target = if let Some(target) = &rule.target {
        target.clone()
    } else {
        let output = std::process::Command::new(std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into()))
            .arg("-vV")
            .output()
            .map_err(|error| error.to_string())?;
        if !output.status.success() {
            return Err(format!("rustc host: {}", String::from_utf8_lossy(&output.stderr)));
        }
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .find_map(|line| line.strip_prefix("host: ").map(str::to_owned))
            .ok_or("rustc did not report host target")?
    };
    let output = cargo.output(&[
        "tree",
        "--locked",
        "--target",
        &target,
        "--edges",
        "normal,build",
        "--package",
        &rule.root_package,
        "--depth",
        &rule.depth.to_string(),
        "--no-dedupe",
        "--prefix",
        "none",
        "--format",
        "{p}|{f}",
    ])?;
    validate_target_features(rule, &String::from_utf8_lossy(&output.stdout)).map_err(|error| format!("{target}: {error}"))?;
    Ok(TargetReport { root_package: rule.root_package.clone(), target, package: rule.package.clone(), features: rule.features.clone() })
}

pub fn cli_help() -> &'static str {
    "pdf_deps (configured source, feature and production-graph assertions)"
}

pub fn run(cfg: &Config, args: &[String]) -> Result {
    print!("{}", render(&compute(cfg, &Args::parse(args)?)?));
    Ok(())
}
