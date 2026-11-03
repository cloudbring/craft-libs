use craft_xtask::{Configs, ico, parity, stats, version};

const MANIFEST: &str = "[workspace.package]\nversion = \"0.1.0\"\n\n[workspace.dependencies]\nown = { path = \"crates/own\", version = \"0.1.0\" }\nexternal = { version = \"0.1.0\" }\n";

#[test]
fn version_variant_validation_and_path_pins() {
    let mut config = version::Config { strict_prerelease: true, bounded_core: true, update_path_pins: true, ..Default::default() };
    for invalid in ["1.0.0-01", "1.0.0-rc.01", "18446744073709551616.0.0"] {
        assert!(version::rewrite(&config, MANIFEST, invalid).is_err());
    }
    let text = version::rewrite(&config, MANIFEST, "2.0.0-01a").unwrap();
    assert!(text.contains("own = { path = \"crates/own\", version = \"2.0.0-01a\" }"));
    assert!(text.contains("external = { version = \"0.1.0\" }"));
    config.strict_prerelease = false;
    config.bounded_core = false;
    assert!(version::rewrite(&config, MANIFEST, "18446744073709551616.0.0-01").is_ok());
}

#[test]
fn version_multiple_manifests_changelog_and_adjacent_staging() {
    let temp = tempfile::tempdir().unwrap();
    for file in ["Cargo.toml", "extra.toml"] {
        std::fs::write(temp.path().join(file), MANIFEST).unwrap();
    }
    std::fs::write(temp.path().join("CHANGELOG.md"), "# Changes\n## [Unreleased]\nFixed things.\n").unwrap();
    let mut config = version::Config {
        manifest_paths: vec!["Cargo.toml".into(), "extra.toml".into()],
        temporary_dir: ".".into(),
        refresh_lockfile: false,
        changelog_path: Some("CHANGELOG.md".into()),
        ..Default::default()
    };
    config.cargo.root = temp.path().to_owned();
    version::run(&config, &["set".into(), "2.3.4".into()]).unwrap();
    for file in ["Cargo.toml", "extra.toml"] {
        assert_eq!(version::read(&std::fs::read_to_string(temp.path().join(file)).unwrap()).unwrap(), "2.3.4");
    }
    assert!(std::fs::read_to_string(temp.path().join("CHANGELOG.md")).unwrap().contains("## [2.3.4]"));
    assert!(!temp.path().join("target").exists());
}

#[test]
fn version_invalid_arguments_do_not_write() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("Cargo.toml");
    std::fs::write(&path, MANIFEST).unwrap();
    let mut config = version::Config::default();
    config.cargo.root = temp.path().to_owned();
    for args in [vec!["set".into()], vec!["set".into(), "bad".into()], vec!["bad".into()]] {
        assert!(version::run(&config, &args).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), MANIFEST);
    }
}

#[test]
fn stats_exclusions_counts_and_budget() {
    let temp = tempfile::tempdir().unwrap();
    for dir in ["src", "target", "fuzz", ".hidden"] {
        std::fs::create_dir(temp.path().join(dir)).unwrap();
        std::fs::write(temp.path().join(dir).join("test.rs"), "// #[test]\n#[test]\nproptest! {\n}\n").unwrap();
    }
    let mut config = stats::Config::default();
    assert_eq!(stats::count(&config, temp.path()).unwrap(), stats::Counts { files: 1, lines: 4, tests: 1, proptests: 1 });
    config.max_entries = 0;
    assert!(stats::count(&config, temp.path()).is_err());
    config.fail_on_budget = false;
    assert_eq!(stats::count(&config, temp.path()).unwrap(), stats::Counts::default());
}

#[cfg(unix)]
#[test]
fn stats_symlink_cycles_are_bounded_and_not_double_counted() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir(temp.path().join("src")).unwrap();
    std::fs::write(temp.path().join("src/test.rs"), "#[test]\n").unwrap();
    std::os::unix::fs::symlink(temp.path(), temp.path().join("src/cycle")).unwrap();
    std::os::unix::fs::symlink(temp.path().join("src"), temp.path().join("alias")).unwrap();
    for follow_symlinks in [true, false] {
        assert_eq!(stats::count(&stats::Config { follow_symlinks, ..Default::default() }, temp.path()).unwrap().files, 1);
    }
}

fn png(width: u32, height: u32) -> Vec<u8> {
    let mut bytes = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".to_vec();
    bytes.extend(width.to_be_bytes());
    bytes.extend(height.to_be_bytes());
    bytes
}

#[test]
fn icon_rejects_empty_truncated_nonsquare_and_invalid_sizes() {
    assert!(ico::pack(&[]).is_err());
    assert!(ico::pack_icns(&[]).is_err());
    let bytes = png(16, 16);
    for length in 0..24 {
        assert!(ico::png_size(&bytes[..length]).is_err());
    }
    for bytes in [png(0, 0), png(16, 32), png(512, 512)] {
        assert!(ico::pack(&[bytes]).is_err());
    }
    assert!(ico::pack_icns(&[png(48, 48)]).is_err());
}

#[test]
fn configured_icon_paths_sizes_and_dispatch() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("16.png"), png(16, 16)).unwrap();
    let mut configs = Configs::for_root(temp.path());
    configs.ico.source_pngs = vec!["16.png".into()];
    configs.ico.output_path = Some("out.ico".into());
    configs.ico.sizes = vec![16];
    craft_xtask::dispatch("ico", &configs, &[]).unwrap();
    assert_eq!(&std::fs::read(temp.path().join("out.ico")).unwrap()[..6], &[0, 0, 1, 0, 1, 0]);
    craft_xtask::dispatch("icns", &configs, &[]).unwrap();
    assert_eq!(&std::fs::read(temp.path().join("out.ico")).unwrap()[..4], b"icns");
    configs.ico.sizes = vec![32];
    assert!(ico::run(&configs.ico, &[]).is_err());
}

#[test]
fn parity_markers_are_ordered_and_empty_rows_supported() {
    let cfg = parity::Config::default();
    assert!(parity::rewrite(&cfg, "\n## Rows\n<!-- SUMMARY -->").is_err());
    assert!(parity::rewrite(&cfg, "no markers").is_err());
    assert!(parity::rewrite(&cfg, "<!-- SUMMARY -->\n## Rows\n").unwrap().contains("0%"));
    assert!(parity::summary(&[("A".into(), "x".into(), 255, 1.0)]).contains("0%"));
}

#[test]
fn file_sha256_uses_streaming_workspace_hasher() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("data");
    std::fs::write(&path, b"abc").unwrap();
    assert_eq!(craft_xtask::sha256_hex(&path).unwrap(), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    std::fs::remove_file(&path).unwrap();
    assert!(craft_xtask::sha256_hex(&path).is_err());
}

#[test]
fn dispatcher_help_and_unknown_task() {
    assert!(craft_xtask::dispatch("help", &Default::default(), &[]).is_ok());
    assert!(craft_xtask::dispatch("bogus", &Default::default(), &[]).unwrap_err().contains("unknown task"));
    for help in [
        version::cli_help(),
        stats::cli_help(),
        ico::cli_help(),
        parity::cli_help(),
        craft_xtask::layers::cli_help(),
        craft_xtask::pdf_deps::cli_help(),
    ] {
        assert!(!help.is_empty());
    }
}

#[cfg(unix)]
mod commands {
    use super::{MANIFEST, stats, version};
    use std::os::unix::fs::PermissionsExt;

    fn script(temp: &tempfile::TempDir, body: &str) -> std::path::PathBuf {
        let path = temp.path().join("cargo-stub");
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
        path
    }

    #[test]
    fn version_offline_fallback_and_rollback_policy() {
        let temp = tempfile::tempdir().unwrap();
        let manifest = temp.path().join("Cargo.toml");
        std::fs::write(&manifest, MANIFEST).unwrap();
        let mut config = version::Config::default();
        config.cargo.root = temp.path().to_owned();
        config.cargo.program = script(&temp, "printf '%s\\n' \"$*\" >> calls\ncase \"$*\" in *--offline*) exit 1;; esac\nexit 0");
        version::run(&config, &["set".into(), "2.0.0".into()]).unwrap();
        assert_eq!(std::fs::read_to_string(temp.path().join("calls")).unwrap(), "update --workspace --offline\nupdate --workspace\n");
        config.cargo.program = script(&temp, "exit 1");
        config.rollback_on_failure = true;
        assert!(version::run(&config, &["set".into(), "3.0.0".into()]).is_err());
        assert_eq!(version::read(&std::fs::read_to_string(&manifest).unwrap()).unwrap(), "2.0.0");
        config.rollback_on_failure = false;
        assert!(version::run(&config, &["set".into(), "4.0.0".into()]).is_err());
        assert_eq!(version::read(&std::fs::read_to_string(&manifest).unwrap()).unwrap(), "4.0.0");
    }

    #[test]
    fn stats_exact_harness_counts_and_failure_column() {
        let temp = tempfile::tempdir().unwrap();
        let mut cfg = stats::Config::default();
        cfg.cargo.root = temp.path().to_owned();
        cfg.cargo.program = script(&temp, "printf 'one: test\\ntwo: test\\none: benchmark\\n'");
        let metadata = serde_json::json!({"packages":[{"name":"example", "manifest_path":temp.path().join("Cargo.toml")}]});
        let report = stats::report(&cfg, &metadata, true).unwrap();
        assert!(report.contains("harness"));
        assert!(report.lines().any(|line| line.starts_with("example") && line.ends_with("       2")));
        cfg.cargo.program = script(&temp, "exit 1");
        assert!(stats::report(&cfg, &metadata, true).unwrap().contains("error"));
    }
}
