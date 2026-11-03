#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

//! Native development tooling for app `xtask` crates, never an app runtime dependency.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

pub mod ico;
pub mod layers;
pub mod parity;
pub mod pdf_deps;
pub mod stats;
pub mod version;

pub type Result<T = (), E = String> = std::result::Result<T, E>;

/// Every child command has an explicit working directory; the process cwd is never changed.
#[derive(Debug, Clone)]
pub struct Cargo {
    pub root: PathBuf,
    pub program: PathBuf,
}

impl Default for Cargo {
    fn default() -> Self {
        Self { root: PathBuf::from("."), program: std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()).into() }
    }
}

impl Cargo {
    pub fn command(&self) -> Command {
        let mut command = Command::new(&self.program);
        command.current_dir(&self.root);
        command
    }

    pub fn output(&self, args: &[&str]) -> Result<Output> {
        let output = self.command().args(args).output().map_err(|error| format!("cargo {}: {error}", args.join(" ")))?;
        if !output.status.success() {
            return Err(format!("cargo {} ({}): {}", args.join(" "), output.status, String::from_utf8_lossy(&output.stderr)));
        }
        Ok(output)
    }

    pub fn metadata(&self, resolved: bool) -> Result<serde_json::Value> {
        let mut args = vec!["metadata", "--format-version", "1"];
        if resolved {
            args.push("--locked");
        } else {
            args.push("--no-deps");
        }
        serde_json::from_slice(&self.output(&args)?.stdout).map_err(|error| format!("cargo metadata: {error}"))
    }
}

/// Streaming file hashing, delegated to the workspace hashing crate.
pub fn sha256_hex(path: &Path) -> Result<String> {
    craft_hash::sha256_file(path).map_err(|error| format!("{}: {error}", path.display()))
}

#[derive(Debug, Clone, Default)]
pub struct Configs {
    pub version: version::VersionConfig,
    pub stats: stats::StatsConfig,
    pub ico: ico::IcoConfig,
    pub layers: layers::LayersConfig,
    pub parity: parity::ParityConfig,
    pub pdf_deps: pdf_deps::PdfDepsConfig,
}

impl Configs {
    pub fn for_root(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        let mut configs = Self::default();
        configs.version.cargo.root = root.clone();
        configs.stats.cargo.root = root.clone();
        configs.ico.root = root.clone();
        configs.layers.root = root.clone();
        configs.parity.cargo.root = root.clone();
        configs.pdf_deps.cargo.root = root;
        configs
    }
}

pub fn dispatch(task_name: &str, configs: &Configs, args: &[String]) -> Result {
    match task_name {
        "version" => version::run(&configs.version, args),
        "stats" => stats::run(&configs.stats, args),
        "ico" => ico::run(&configs.ico, args),
        "icns" => ico::run_icns(&configs.ico, args),
        "layers" => layers::run(&configs.layers, args),
        "parity" => parity::run(&configs.parity, args),
        "pdf_deps" | "pdf-deps" => pdf_deps::run(&configs.pdf_deps, args),
        "help" | "--help" | "-h" => {
            println!("{}", cli_help());
            Ok(())
        }
        _ => Err(format!("unknown task `{task_name}`\n{}", cli_help())),
    }
}

pub fn cli_help() -> String {
    [version::cli_help(), stats::cli_help(), ico::cli_help(), layers::cli_help(), parity::cli_help(), pdf_deps::cli_help()].join("\n")
}

pub(crate) fn write(path: &Path, bytes: impl AsRef<[u8]>) -> Result {
    std::fs::write(path, bytes).map_err(|error| format!("write {}: {error}", path.display()))
}

pub(crate) fn read(path: &Path) -> Result<String> {
    std::fs::read_to_string(path).map_err(|error| format!("read {}: {error}", path.display()))
}

pub(crate) fn matches(patterns: &[String], name: &str) -> bool {
    patterns.iter().any(|pattern| glob::Pattern::new(pattern).is_ok_and(|pattern| pattern.matches(name)))
}
