#![cfg(test)]

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::process::Command;

thread_local! {
    static ROOT: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
    static OUTPUT: RefCell<String> = const { RefCell::new(String::new()) };
    static METADATA: RefCell<serde_json::Value> = const { RefCell::new(serde_json::Value::Null) };
}

macro_rules! println {
    ($($args:tt)*) => {{ let text = format!($($args)*); $crate::OUTPUT.with(|output| { use std::fmt::Write; writeln!(output.borrow_mut(), "{text}").unwrap(); }); }};
}

macro_rules! print {
    ($($args:tt)*) => {{ let text = format!($($args)*); $crate::OUTPUT.with(|output| output.borrow_mut().push_str(&text)); }};
}

#[path = "oracles/ico.rs"]
#[allow(clippy::wildcard_imports)]
mod ico_oracle;
use ico_oracle as ico;
#[path = "oracles/icns.rs"]
#[allow(clippy::wildcard_imports)]
mod icns_oracle;
#[path = "oracles/parity.rs"]
#[allow(clippy::wildcard_imports)]
mod parity_oracle;
#[path = "oracles/stats.rs"]
mod stats_oracle;
#[path = "oracles/version.rs"]
#[allow(clippy::wildcard_imports)]
mod version_oracle;

fn root() -> PathBuf {
    ROOT.with(|root| root.borrow().clone()).unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/lightcraft"))
}

fn cargo() -> Command {
    let mut command = Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()));
    command.current_dir(root());
    command
}

fn run(mut command: Command, what: &str) -> Result<(), String> {
    let output = command.output().map_err(|error| format!("{what}: {error}"))?;
    if output.status.success() { Ok(()) } else { Err(format!("{what}: {}", String::from_utf8_lossy(&output.stderr))) }
}

fn metadata() -> Result<serde_json::Value, String> {
    METADATA.with(|metadata| Ok(metadata.borrow().clone()))
}

fn copy_tree(source: &Path, destination: &Path) {
    std::fs::create_dir_all(destination).unwrap();
    for entry in std::fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &destination.join(entry.file_name()));
        } else {
            std::fs::copy(entry.path(), destination.join(entry.file_name())).unwrap();
        }
    }
}

#[test]
fn version_run_matches_original_manifest_and_real_cargo_lock_update() {
    let original = tempfile::tempdir().unwrap();
    let shared = tempfile::tempdir().unwrap();
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/lightcraft");
    copy_tree(&fixture, original.path());
    copy_tree(&fixture, shared.path());
    let metadata: serde_json::Value = serde_json::from_str(include_str!("fixtures/layers/lightcraft.json")).unwrap();
    for package in metadata["packages"].as_array().unwrap() {
        for target in package["targets"].as_array().unwrap() {
            let relative = target["src_path"].as_str().unwrap().strip_prefix("/workspace/lightcraft/").unwrap();
            for root in [original.path(), shared.path()] {
                let path = root.join(relative);
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                std::fs::write(path, "").unwrap();
            }
        }
    }
    ROOT.with(|root| *root.borrow_mut() = Some(original.path().to_owned()));
    OUTPUT.with(|output| output.borrow_mut().clear());
    version_oracle::run(original.path(), &["set", "9.8.7-rc.1"]).unwrap();
    let mut config = craft_xtask::version::Config::default();
    config.cargo.root = shared.path().to_owned();
    let plan = craft_xtask::version::compute(&config, &craft_xtask::version::Args::Set("9.8.7-rc.1".into())).unwrap();
    OUTPUT.with(|output| assert_eq!(craft_xtask::version::render(&plan), *output.borrow()));
    craft_xtask::version::run(&config, &["set".into(), "9.8.7-rc.1".into()]).unwrap();
    for name in ["Cargo.toml", "Cargo.lock"] {
        assert_eq!(std::fs::read(original.path().join(name)).unwrap(), std::fs::read(shared.path().join(name)).unwrap(), "{name}");
    }
    for entry in walk_manifests(&fixture) {
        if entry == Path::new("Cargo.toml") {
            continue;
        }
        assert_eq!(
            std::fs::read(shared.path().join(&entry)).unwrap(),
            std::fs::read(fixture.join(&entry)).unwrap(),
            "member manifests must not change"
        );
    }
    ROOT.with(|root| *root.borrow_mut() = None);
}

fn walk_manifests(root: &Path) -> Vec<PathBuf> {
    let mut pending = vec![root.to_owned()];
    let mut paths = Vec::new();
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            if entry.path().is_dir() {
                pending.push(entry.path());
            } else if entry.file_name() == "Cargo.toml" {
                paths.push(entry.path().strip_prefix(root).unwrap().to_owned());
            }
        }
    }
    paths
}

#[test]
fn version_parser_matches_oracle_edge_cases() {
    for text in [
        include_str!("fixtures/lightcraft/Cargo.toml").to_owned(),
        "[workspace.package]\r\n version = \"0.1.0\"\r\n".into(),
        "[workspace.package]\nversion = \"0.1.0\"".into(),
    ] {
        for new in ["1.2.3", "1.2.3-01", "2.0.0-rc.1", "1.0", "v1.2.3", "01.0.0", "1.2.3+build", "1.2.3-", "1.2.3-a..b"] {
            assert_eq!(craft_xtask::version::rewrite(&Default::default(), &text, new), version_oracle::replace(&text, new));
        }
        assert_eq!(craft_xtask::version::read(&text), version_oracle::read(&text));
    }
}

#[test]
fn stats_run_report_matches_original_on_copied_xtask_sources() {
    let temp = tempfile::tempdir().unwrap();
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/oracles");
    copy_tree(&source, &temp.path().join("xtask/src"));
    std::fs::create_dir_all(temp.path().join("xtask/target")).unwrap();
    std::fs::write(temp.path().join("xtask/target/ignored.rs"), "#[test]\nfn ignored() {}\n").unwrap();
    let meta = serde_json::json!({"packages":[{"name":"xtask","manifest_path":temp.path().join("xtask/Cargo.toml")} ]});
    METADATA.with(|metadata| *metadata.borrow_mut() = meta.clone());
    OUTPUT.with(|output| output.borrow_mut().clear());
    stats_oracle::run(temp.path(), false).unwrap();
    let actual = craft_xtask::stats::report(&Default::default(), &meta, false).unwrap();
    let report = craft_xtask::stats::from_metadata(&Default::default(), &meta, &Default::default()).unwrap();
    assert_eq!(craft_xtask::stats::render(&report), actual);
    OUTPUT.with(|output| assert_eq!(*output.borrow(), actual));
}

fn png(size: u32) -> Vec<u8> {
    let mut bytes = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".to_vec();
    bytes.extend(size.to_be_bytes());
    bytes.extend(size.to_be_bytes());
    bytes.extend([8, 6, 0, 0, 0, 1, 2, 3, 4]);
    bytes
}

#[test]
fn ico_run_bytes_match_original_for_all_supported_sizes() {
    let temp = tempfile::tempdir().unwrap();
    let sizes = [16, 24, 32, 48, 64, 128, 256];
    let paths: Vec<_> = sizes
        .iter()
        .map(|size| {
            let path = temp.path().join(format!("{size}.png"));
            std::fs::write(&path, png(*size)).unwrap();
            path.to_string_lossy().into_owned()
        })
        .collect();
    let mut args = vec![temp.path().join("oracle.ico").to_string_lossy().into_owned()];
    args.extend(paths);
    OUTPUT.with(|output| output.borrow_mut().clear());
    ico_oracle::run(&args.iter().map(String::as_str).collect::<Vec<_>>()).unwrap();
    args[0] = temp.path().join("shared.ico").to_string_lossy().into_owned();
    let report = craft_xtask::ico::compute(&Default::default(), &craft_xtask::ico::Args::parse(&args).unwrap()).unwrap();
    OUTPUT.with(|output| assert_eq!(craft_xtask::ico::render(&report), output.borrow().replace("oracle.ico", "shared.ico")));
    craft_xtask::ico::run(&Default::default(), &args).unwrap();
    assert_eq!(std::fs::read(temp.path().join("oracle.ico")).unwrap(), std::fs::read(temp.path().join("shared.ico")).unwrap());
}

#[test]
fn icns_run_matches_launcher_slots_and_bytes() {
    let temp = tempfile::tempdir().unwrap();
    let mut args = vec![temp.path().join("oracle.icns").to_string_lossy().into_owned()];
    for size in [16, 32, 64, 128, 256, 512, 1024] {
        let path = temp.path().join(format!("{size}.png"));
        std::fs::write(&path, png(size)).unwrap();
        args.push(path.to_string_lossy().into_owned());
    }
    OUTPUT.with(|output| output.borrow_mut().clear());
    icns_oracle::run(&args.iter().map(String::as_str).collect::<Vec<_>>()).unwrap();
    args[0] = temp.path().join("shared.icns").to_string_lossy().into_owned();
    let config = craft_xtask::ico::Config { icns: true, ..Default::default() };
    let report = craft_xtask::ico::compute(&config, &craft_xtask::ico::Args::parse(&args).unwrap()).unwrap();
    OUTPUT.with(|output| assert_eq!(craft_xtask::ico::render(&report), output.borrow().replace("oracle.icns", "shared.icns")));
    craft_xtask::ico::run_icns(&Default::default(), &args).unwrap();
    assert_eq!(std::fs::read(temp.path().join("oracle.icns")).unwrap(), std::fs::read(temp.path().join("shared.icns")).unwrap());
}

#[test]
fn parity_run_matches_original_checklist_bytes() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir(temp.path().join("docs")).unwrap();
    let text = "# Checklist\n<!-- SUMMARY -->\nold summary\n## Rows\n| A | x | P0 | D |\n| A | y | P1 | P |\n| B | z | P2 | M |\n";
    let path = temp.path().join("docs/parity-checklist.md");
    std::fs::write(&path, text).unwrap();
    parity_oracle::run(temp.path()).unwrap();
    let expected = std::fs::read(&path).unwrap();
    std::fs::write(&path, text).unwrap();
    let mut config = craft_xtask::parity::Config::default();
    config.cargo.root = temp.path().to_owned();
    craft_xtask::parity::run(&config, &[]).unwrap();
    let report = craft_xtask::parity::compute(&config, &Default::default()).unwrap();
    assert_eq!(craft_xtask::parity::render(&report).as_bytes(), expected);
    assert_eq!(expected, std::fs::read(path).unwrap());
    craft_xtask::parity::run(&config, &["--check".into()]).unwrap();
}
