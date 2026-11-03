// Provenance: photocraft/xtask/src/layers.rs (rule engine); lightcraft, vectorcraft, designcraft,
// deckcraft, cadcraft, wordcraft, pdfcraft, gridcraft, figcraft, texturecraft and materialcraft/xtask/src/layers.rs (policies).
use std::collections::BTreeSet;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{Cargo, Result};

/// Layers are ordered bottom to top. Globs match full Cargo package names, not dependency aliases.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct LayersConfig {
    pub root: PathBuf,
    pub layers: Vec<(String, Vec<String>)>,
    pub allowed_edges: Vec<(String, String)>,
    pub allowed_dev_edges: Vec<(String, String)>,
    pub allow_lower_layers: bool,
    pub exempt: Vec<String>,
    pub standalone: Vec<String>,
    pub testkits: Vec<String>,
    pub testkits_may_depend_on_testkits: bool,
    pub standalone_may_use_testkits: bool,
    pub ignore_dev: bool,
    pub reject_unregistered: bool,
    pub external_rules: Vec<ExternalRule>,
    pub workspace_rules: Vec<WorkspaceRule>,
}

impl Default for LayersConfig {
    fn default() -> Self {
        Self {
            root: ".".into(),
            layers: Vec::new(),
            allowed_edges: Vec::new(),
            allowed_dev_edges: Vec::new(),
            allow_lower_layers: true,
            exempt: Vec::new(),
            standalone: Vec::new(),
            testkits: Vec::new(),
            testkits_may_depend_on_testkits: false,
            standalone_may_use_testkits: false,
            ignore_dev: false,
            reject_unregistered: true,
            external_rules: Vec::new(),
            workspace_rules: Vec::new(),
        }
    }
}

pub type Config = LayersConfig;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExternalRule {
    pub dependencies: Vec<String>,
    pub allowed_from: Vec<String>,
    #[serde(default)]
    pub allowed_dev_from: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceRule {
    pub from: String,
    pub allowed: Vec<String>,
    #[serde(default)]
    pub allow_dev: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
pub enum DepKind {
    Normal,
    Dev,
    Build,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Dep {
    pub name: String,
    pub kind: DepKind,
    pub workspace: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Crate {
    pub name: String,
    pub deps: Vec<Dep>,
}

fn glob(pattern: &str, name: &str) -> bool {
    crate::matches(&[pattern.to_owned()], name)
}

impl LayersConfig {
    fn layer(&self, name: &str) -> Option<usize> {
        self.layers.iter().position(|(_, patterns)| crate::matches(patterns, name))
    }

    fn allowed(&self, from: &str, to: &str) -> bool {
        self.allowed_edges.iter().any(|(source, dest)| glob(source, from) && glob(dest, to))
    }

    pub fn validate(&self) -> Result {
        if self.layers.is_empty() {
            return Err("layers: configure at least one layer".into());
        }
        let mut names = BTreeSet::new();
        for (name, _) in &self.layers {
            if name.is_empty() || !names.insert(name) {
                return Err(format!("empty or duplicate layer: {name}"));
            }
        }
        let mut patterns: Vec<&String> = self.layers.iter().flat_map(|(_, patterns)| patterns).collect();
        patterns.extend(self.allowed_edges.iter().flat_map(|(source, dest)| [source, dest]));
        patterns.extend(self.allowed_dev_edges.iter().flat_map(|(source, dest)| [source, dest]));
        patterns.extend(self.exempt.iter().chain(&self.standalone).chain(&self.testkits));
        for rule in &self.external_rules {
            patterns.extend(rule.dependencies.iter().chain(&rule.allowed_from).chain(&rule.allowed_dev_from));
        }
        for rule in &self.workspace_rules {
            patterns.push(&rule.from);
            patterns.extend(&rule.allowed);
        }
        for pattern in patterns {
            glob::Pattern::new(pattern).map_err(|error| format!("invalid crate glob {pattern}: {error}"))?;
        }
        Ok(())
    }
}

/// Inspect all normal, build, optional and target-specific declared dependencies, including dev by default.
pub fn check(cfg: &Config, crates: &[Crate]) -> Result<Vec<String>> {
    cfg.validate()?;
    let mut violations = Vec::new();
    for krate in crates {
        if crate::matches(&cfg.exempt, &krate.name) {
            continue;
        }
        let count = cfg.layers.iter().filter(|(_, patterns)| crate::matches(patterns, &krate.name)).count();
        if count > 1 {
            return Err(format!("{} matches multiple layers", krate.name));
        }
        let Some(from) = cfg.layer(&krate.name) else {
            if cfg.reject_unregistered {
                violations.push(format!("{}: unregistered workspace crate", krate.name));
            }
            continue;
        };
        for dep in &krate.deps {
            if dep.name == krate.name || cfg.ignore_dev && dep.kind == DepKind::Dev {
                continue;
            }
            if dep.workspace {
                check_workspace(cfg, krate, dep, from, &mut violations);
            } else {
                for rule in &cfg.external_rules {
                    if crate::matches(&rule.dependencies, &dep.name)
                        && !crate::matches(&rule.allowed_from, &krate.name)
                        && !(dep.kind == DepKind::Dev && crate::matches(&rule.allowed_dev_from, &krate.name))
                    {
                        violations.push(format!("{} -> {} [{:?}]: forbidden external dependency", krate.name, dep.name, dep.kind));
                    }
                }
            }
        }
    }
    violations.sort();
    violations.dedup();
    Ok(violations)
}

fn check_workspace(cfg: &Config, krate: &Crate, dep: &Dep, from: usize, violations: &mut Vec<String>) {
    let testkit = crate::matches(&cfg.testkits, &dep.name);
    let testkit_ok = testkit && (dep.kind == DepKind::Dev || cfg.testkits_may_depend_on_testkits && crate::matches(&cfg.testkits, &krate.name));
    let standalone = crate::matches(&cfg.standalone, &krate.name);
    let permitted = cfg.allowed(&krate.name, &dep.name);
    if standalone && !(permitted || cfg.standalone_may_use_testkits && testkit_ok) {
        violations.push(format!("{} -> {}: standalone dependency", krate.name, dep.name));
        return;
    }
    if testkit && !testkit_ok {
        violations.push(format!("{} -> {}: testkit is dev-only", krate.name, dep.name));
        return;
    }
    for rule in &cfg.workspace_rules {
        if glob(&rule.from, &krate.name) && !(rule.allow_dev && dep.kind == DepKind::Dev || crate::matches(&rule.allowed, &dep.name)) {
            violations.push(format!("{} -> {}: workspace dependency allowlist", krate.name, dep.name));
        }
    }
    let dev_permitted =
        dep.kind == DepKind::Dev && cfg.allowed_dev_edges.iter().any(|(source, dest)| glob(source, &krate.name) && glob(dest, &dep.name));
    if testkit_ok || permitted || dev_permitted {
        return;
    }
    match cfg.layer(&dep.name) {
        Some(to) if cfg.allow_lower_layers && to < from => {}
        None if !cfg.reject_unregistered && !crate::matches(&cfg.exempt, &dep.name) => {}
        _ => violations.push(format!("{} -> {} [{:?}]: disallowed layer edge", krate.name, dep.name, dep.kind)),
    }
}

/// Supports both full metadata and --no-deps; only workspace members are checked.
/// A path outside the workspace is external (not an unregistered local crate).
pub fn from_metadata(meta: &Value) -> Result<Vec<Crate>> {
    let packages = meta["packages"].as_array().ok_or("metadata: no packages array")?;
    let members = meta["workspace_members"].as_array().ok_or("metadata: no workspace_members array")?;
    let packages: Vec<_> = packages.iter().filter(|package| members.contains(&package["id"])).collect();
    let names: BTreeSet<_> = packages.iter().filter_map(|package| package["name"].as_str()).collect();
    let mut graph = Vec::new();
    for package in &packages {
        let name = package["name"].as_str().ok_or("package without name")?.to_owned();
        let mut deps = Vec::new();
        for dep in package["dependencies"].as_array().ok_or("package without dependencies")? {
            let dep_name = dep["name"].as_str().ok_or("dependency without name")?.to_owned();
            let kind = match dep["kind"].as_str() {
                None => DepKind::Normal,
                Some("dev") => DepKind::Dev,
                Some("build") => DepKind::Build,
                Some(other) => return Err(format!("unknown dependency kind: {other}")),
            };
            let workspace = names.contains(dep_name.as_str()) && dep["source"].is_null();
            deps.push(Dep { name: dep_name, kind, workspace });
        }
        graph.push(Crate { name, deps });
    }
    graph.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(graph)
}

pub fn cli_help() -> &'static str {
    "layers [--metadata FILE.json]"
}

pub fn run(cfg: &Config, args: &[String]) -> Result {
    let metadata = match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        [] => Cargo { root: cfg.root.clone(), ..Cargo::default() }.metadata(false)?,
        ["--metadata", path] => serde_json::from_str(&crate::read(&cfg.root.join(path))?).map_err(|error| error.to_string())?,
        _ => return Err(format!("usage: {}", cli_help())),
    };
    let violations = check(cfg, &from_metadata(&metadata)?)?;
    if !violations.is_empty() {
        return Err(violations.join("\n"));
    }
    println!("layers: ok");
    Ok(())
}
