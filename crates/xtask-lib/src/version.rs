// Provenance: lightcraft/xtask/src/version.rs (shared byte-preserving parser).
// Variant policies: photocraft, soundcraft, texturecraft, materialcraft and pdfcraft/xtask/src/version.rs.
use std::path::{Path, PathBuf};

use crate::{Cargo, Result};

#[derive(Debug, Clone)]
pub struct VersionConfig {
    pub cargo: Cargo,
    pub manifest_paths: Vec<PathBuf>,
    pub temporary_dir: PathBuf,
    pub refresh_lockfile: bool,
    pub online_fallback: bool,
    pub rollback_on_failure: bool,
    pub update_path_pins: bool,
    pub strict_prerelease: bool,
    pub bounded_core: bool,
    pub changelog_path: Option<PathBuf>,
    pub tag_format: String,
}

pub type Config = VersionConfig;

impl Default for VersionConfig {
    fn default() -> Self {
        Self {
            cargo: Cargo::default(),
            manifest_paths: vec!["Cargo.toml".into()],
            temporary_dir: "target".into(),
            refresh_lockfile: true,
            online_fallback: true,
            rollback_on_failure: false,
            update_path_pins: false,
            strict_prerelease: false,
            bounded_core: false,
            changelog_path: None,
            tag_format: "v{version}".into(),
        }
    }
}

pub fn cli_help() -> &'static str {
    "version [set X.Y.Z[-pre] | tag]"
}

pub fn rewrite(cfg: &Config, manifest: &str, new: &str) -> Result<String> {
    validate(new)?;
    let (core, pre) = new.split_once('-').map_or((new, None), |(core, pre)| (core, Some(pre)));
    if cfg.bounded_core && core.split('.').any(|part| part.parse::<u64>().is_err()) {
        return Err("version components must fit in u64".into());
    }
    if cfg.strict_prerelease
        && pre.is_some_and(|pre| pre.split('.').any(|id| id.len() > 1 && id.starts_with('0') && id.bytes().all(|byte| byte.is_ascii_digit())))
    {
        return Err("numeric prerelease identifiers must not have leading zeroes".into());
    }
    let mut updated = replace(manifest, new)?;
    if cfg.update_path_pins {
        let old_pin = format!("version = \"{}\"", read(manifest)?);
        let new_pin = format!("version = \"{new}\"");
        let mut in_deps = false;
        updated = updated
            .split_inclusive('\n')
            .map(|line| {
                if line.trim().starts_with('[') {
                    in_deps = line.trim() == "[workspace.dependencies]";
                }
                if in_deps && line.contains("path = \"") { line.replace(&old_pin, &new_pin) } else { line.to_owned() }
            })
            .collect();
    }
    Ok(updated)
}

fn atomic_write(cfg: &Config, path: &Path, text: &str) -> Result {
    let temporary_dir = cfg.cargo.root.join(&cfg.temporary_dir);
    std::fs::create_dir_all(&temporary_dir).map_err(|error| format!("create {}: {error}", temporary_dir.display()))?;
    let temporary = temporary_dir.join("Cargo.toml.xtask-version");
    crate::write(&temporary, text)?;
    std::fs::rename(&temporary, path).map_err(|error| format!("replace {}: {error}", path.display()))
}

pub fn run(cfg: &Config, args: &[String]) -> Result {
    let first = cfg.manifest_paths.first().ok_or("version: no manifest paths")?;
    let text = crate::read(&cfg.cargo.root.join(first))?;
    let args: Vec<_> = args.iter().map(String::as_str).collect();
    match args.as_slice() {
        [] => println!("{}", read(&text)?),
        ["tag"] => println!("{}", cfg.tag_format.replace("{version}", &read(&text)?)),
        ["set", new] => set(cfg, new)?,
        _ => return Err(format!("usage: cargo xtask {}", cli_help())),
    }
    Ok(())
}

fn set(cfg: &Config, new: &str) -> Result {
    let mut edits = Vec::new();
    for relative in &cfg.manifest_paths {
        let path = cfg.cargo.root.join(relative);
        let old = crate::read(&path)?;
        let updated = rewrite(cfg, &old, new)?;
        edits.push((path, old, updated));
    }
    let old_version = read(&edits.first().ok_or("version: no manifest paths")?.1)?;
    if let Some(relative) = &cfg.changelog_path {
        let path = cfg.cargo.root.join(relative);
        let old = crate::read(&path)?;
        if !old.contains("## [Unreleased]") {
            return Err("changelog needs ## [Unreleased]".into());
        }
        let updated = old.replacen("## [Unreleased]", &format!("## [{new}]"), 1);
        edits.push((path, old, updated));
    }
    let result = (|| {
        for (path, _, updated) in &edits {
            atomic_write(cfg, path, updated)?;
        }
        if cfg.refresh_lockfile {
            let result = cfg.cargo.output(&["update", "--workspace", "--offline"]);
            if result.is_err() && cfg.online_fallback {
                cfg.cargo.output(&["update", "--workspace"])?;
            } else {
                result?;
            }
        }
        Ok(())
    })();
    if let Err(error) = result {
        if cfg.rollback_on_failure {
            for (path, old, _) in &edits {
                atomic_write(cfg, path, old).map_err(|restore| format!("{error}; restore failed: {restore}"))?;
            }
        }
        return Err(error);
    }
    println!("version: {old_version} -> {new}");
    Ok(())
}
/// The `version = "…"` value inside `[workspace.package]`.
pub fn read(manifest: &str) -> Result<String, String> {
    let (_, line) = find_line(manifest)?;
    let v = line.split_once('=').map(|(_, v)| v.trim().trim_matches('"').to_string()).unwrap_or_default();
    if v.is_empty() {
        return Err("empty [workspace.package] version".into());
    }
    Ok(v)
}

/// `manifest` with the workspace version replaced; everything else is byte-for-byte unchanged.
pub fn replace(manifest: &str, new: &str) -> Result<String, String> {
    validate(new)?;
    let (idx, _) = find_line(manifest)?;
    let mut out = String::with_capacity(manifest.len() + 8);
    for (i, line) in manifest.split_inclusive('\n').enumerate() {
        if i == idx {
            let eol = if line.ends_with("\r\n") {
                "\r\n"
            } else if line.ends_with('\n') {
                "\n"
            } else {
                ""
            };
            out.push_str(&format!("version = \"{new}\"{eol}"));
        } else {
            out.push_str(line);
        }
    }
    Ok(out)
}

/// Semver without build metadata: `MAJOR.MINOR.PATCH` plus an optional `-pre.release` tag.
pub fn validate(v: &str) -> Result<(), String> {
    let bad = || Err(format!("`{v}` is not a version like 1.2.3 or 1.2.3-rc.1"));
    let (core, pre) = match v.split_once('-') {
        Some((c, p)) => (c, Some(p)),
        None => (v, None),
    };
    let nums: Vec<&str> = core.split('.').collect();
    if nums.len() != 3 || nums.iter().any(|n| n.is_empty() || !n.bytes().all(|b| b.is_ascii_digit()) || (n.len() > 1 && n.starts_with('0'))) {
        return bad();
    }
    if let Some(p) = pre
        && (p.is_empty() || p.split('.').any(|id| id.is_empty() || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')))
    {
        return bad();
    }
    Ok(())
}

/// Index and text of the `version` line in `[workspace.package]`.
fn find_line(manifest: &str) -> Result<(usize, &str), String> {
    let mut in_section = false;
    for (i, line) in manifest.lines().enumerate() {
        let t = line.trim();
        if t.starts_with('[') {
            in_section = t == "[workspace.package]";
            continue;
        }
        if in_section
            && let Some(rest) = t.strip_prefix("version")
            && rest.trim_start().starts_with('=')
        {
            return Ok((i, line));
        }
    }
    Err("no `version = \"…\"` in [workspace.package] of the root Cargo.toml".into())
}
