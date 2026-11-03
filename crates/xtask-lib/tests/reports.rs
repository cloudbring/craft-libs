#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;

use craft_xtask::{Configs, ico, layers, parity, pdf_deps, stats, version};
use serde_json::json;

fn configs(root: &Path) -> Configs {
    let mut configs = Configs::for_root(root);
    configs.version.refresh_lockfile = false;
    configs.stats.cargo.program = root.join("cargo-fixture");
    configs.pdf_deps.cargo.program = root.join("cargo-fixture");
    configs.pdf_deps.packages = vec![pdf_deps::PackagePin {
        name: "pdf".into(),
        version: "1.0.0".into(),
        source: "registry+fixture".into(),
        required_features: vec!["read".into()],
        exact_features: None,
    }];
    configs.layers.layers = vec![("base".into(), vec!["base".into()]), ("top".into(), vec!["top".into()])];
    configs.ico.output_path = Some("icon.ico".into());
    configs.ico.source_pngs = vec!["icon.png".into()];
    configs
}

fn fixture(task: &str) -> tempfile::TempDir {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    std::fs::write(root.join("Cargo.toml"), "[workspace.package]\nversion = \"1.0.0\"\n").unwrap();
    std::fs::write(root.join("lib.rs"), "#[test]\nfn example() {}\n").unwrap();
    std::fs::create_dir(root.join("docs")).unwrap();
    std::fs::write(root.join("docs/parity-checklist.md"), "<!-- SUMMARY -->\nold\n## Rows\n| A | feature | P0 | P |\n").unwrap();
    let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".to_vec();
    png.extend(32u32.to_be_bytes());
    png.extend(32u32.to_be_bytes());
    std::fs::write(root.join("icon.png"), png).unwrap();
    let metadata = match task {
        "layers" => json!({"workspace_members":["base", "top"], "packages":[
            {"id":"base", "name":"base", "dependencies":[]},
            {"id":"top", "name":"top", "dependencies":[{"name":"base", "kind":null, "source":null}]}
        ]}),
        "pdf_deps" => json!({"packages":[{"id":"pdf", "name":"pdf", "version":"1.0.0", "source":"registry+fixture"}],
            "resolve":{"nodes":[{"id":"pdf", "features":["read"], "deps":[]}]}}),
        _ => json!({"packages":[{"name":"base", "manifest_path":root.join("Cargo.toml")}]}),
    };
    std::fs::write(root.join("metadata.json"), metadata.to_string()).unwrap();
    let program = root.join("cargo-fixture");
    std::fs::write(&program, "#!/bin/sh\ncase \"$1\" in\ntree) printf 'pdf v1.0.0|read\\n';;\n*) cat metadata.json;;\nesac\n").unwrap();
    std::fs::set_permissions(program, std::fs::Permissions::from_mode(0o755)).unwrap();
    temp
}

fn assert_data<T: std::fmt::Debug + Clone + PartialEq + serde::Serialize>(value: &T) {
    assert_eq!(value, &value.clone());
    assert!(serde_json::to_value(value).is_ok());
}

fn run_output(task: &str, root: &Path, args: &[&str]) -> String {
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "task_output_child", "--nocapture"])
        .env("CRAFT_REPORT_TASK", task)
        .env("CRAFT_REPORT_ROOT", root)
        .env("CRAFT_REPORT_ARGS", serde_json::to_string(args).unwrap())
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let stdout = String::from_utf8(output.stdout).unwrap();
    stdout.split_once("[report-start]\n").unwrap().1.split_once("[report-end]").unwrap().0.to_owned()
}

#[test]
fn task_output_child() {
    let Ok(task) = std::env::var("CRAFT_REPORT_TASK") else { return };
    let root = std::env::var_os("CRAFT_REPORT_ROOT").unwrap();
    let configs = configs(Path::new(&root));
    let args: Vec<String> = serde_json::from_str(&std::env::var("CRAFT_REPORT_ARGS").unwrap()).unwrap();
    println!("[report-start]");
    let result = match task.as_str() {
        "version" => version::run(&configs.version, &args),
        "stats" => stats::run(&configs.stats, &args),
        "ico" => ico::run(&configs.ico, &args),
        "icns" => ico::run_icns(&configs.ico, &args),
        "layers" => layers::run(&configs.layers, &args),
        "parity" => parity::run(&configs.parity, &args),
        "pdf_deps" => pdf_deps::run(&configs.pdf_deps, &args),
        _ => panic!("unknown task"),
    };
    if let Err(error) = result {
        print!("ERROR: {error}");
    }
    print!("[report-end]");
}

#[test]
fn version_plan_is_a_dry_run_and_preserves_run_output() {
    let temp = fixture("version");
    let config = configs(temp.path()).version;
    for (args, expected) in [(vec![], "1.0.0\n"), (vec!["tag"], "v1.0.0\n"), (vec!["set", "2.0.0"], "version: 1.0.0 -> 2.0.0\n")] {
        let parsed = version::Args::parse(&args.iter().map(|arg| (*arg).to_owned()).collect::<Vec<_>>()).unwrap();
        let plan = version::compute(&config, &parsed).unwrap();
        assert_data(&plan);
        assert_eq!(version::render(&plan), expected);
        assert_eq!(std::fs::read_to_string(temp.path().join("Cargo.toml")).unwrap(), "[workspace.package]\nversion = \"1.0.0\"\n");
        assert!(!temp.path().join("target").exists());
        assert_eq!(run_output("version", temp.path(), &args), expected);
        if matches!(parsed, version::Args::Set(_)) {
            assert_eq!(plan.tag, "v2.0.0");
            assert_eq!(plan.edits.len(), 1);
            assert_eq!(std::fs::read_to_string(&plan.edits[0].0).unwrap(), plan.edits[0].2);
        }
    }
}

#[test]
fn version_apply_rejects_stale_plans_before_writing() {
    let temp = fixture("version");
    let config = configs(temp.path()).version;
    let plan = version::compute(&config, &version::Args::Set("2.0.0".into())).unwrap();
    std::fs::write(temp.path().join("Cargo.toml"), "edited concurrently").unwrap();
    assert!(version::apply(&plan).unwrap_err().contains("stale"));
    assert_eq!(std::fs::read_to_string(temp.path().join("Cargo.toml")).unwrap(), "edited concurrently");
}

#[test]
fn stats_report_preserves_table_and_detects_changed_counts() {
    let temp = fixture("stats");
    let config = configs(temp.path()).stats;
    let report = stats::compute(&config, &stats::Args::default()).unwrap();
    assert_data(&report);
    assert_eq!(report.total, stats::Counts { files: 1, lines: 2, tests: 1, proptests: 0 });
    assert_eq!(stats::render(&report), run_output("stats", temp.path(), &[]));
    std::fs::write(temp.path().join("lib.rs"), "fn changed() {}\n").unwrap();
    let changed = stats::compute(&config, &stats::Args::default()).unwrap();
    assert_ne!(report, changed);
    assert_ne!(report.rows, changed.rows);
    let exact = stats::compute(&config, &stats::Args { exact: true }).unwrap();
    assert_eq!(stats::render(&exact), run_output("stats", temp.path(), &["--exact"]));
}

#[test]
fn icon_reports_preserve_output_and_packed_bytes_without_writing() {
    let temp = fixture("ico");
    let mut config = configs(temp.path()).ico;
    for task in ["ico", "icns"] {
        config.icns = task == "icns";
        let report = ico::compute(&config, &ico::Args::default()).unwrap();
        assert_data(&report);
        assert_eq!(report.sizes, [32]);
        assert!(!report.destination.exists());
        let expected = format!("icon.ico: 1 images, {} bytes\n", report.bytes.len());
        assert_eq!(ico::render(&report), expected);
        assert_eq!(run_output(task, temp.path(), &[]), expected);
        assert_eq!(std::fs::read(&report.destination).unwrap(), report.bytes);
        std::fs::remove_file(&report.destination).unwrap();
    }
}

#[test]
fn layers_report_retains_edges_and_legacy_error_diagnostics() {
    let temp = fixture("layers");
    let config = configs(temp.path()).layers;
    let args = layers::Args { metadata_path: Some("metadata.json".into()) };
    let report = layers::compute(&config, &args).unwrap();
    assert_data(&report);
    assert!(report.ok);
    assert_eq!(report.crates.len(), 2);
    assert_eq!(layers::render(&report), "layers: ok\n");
    assert_eq!(layers::render(&report), run_output("layers", temp.path(), &["--metadata", "metadata.json"]));
    let mut metadata: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(temp.path().join("metadata.json")).unwrap()).unwrap();
    metadata["packages"][0]["dependencies"] = json!([{"name":"top", "kind":null, "source":null}]);
    std::fs::write(temp.path().join("metadata.json"), metadata.to_string()).unwrap();
    let changed = layers::compute(&config, &args).unwrap();
    assert_ne!(report, changed);
    assert!(!changed.ok);
    assert_eq!(changed.violations[0].from, "base");
    assert_eq!(changed.violations[0].to, "top");
    assert_eq!(changed.violations[0].rule, "disallowed layer edge");
    assert_eq!(layers::render(&changed), "base -> top [Normal]: disallowed layer edge");
    assert_eq!(run_output("layers", temp.path(), &["--metadata", "metadata.json"]), format!("ERROR: {}", layers::render(&changed)));
}

#[test]
fn parity_report_preserves_checklist_and_exposes_weighted_score() {
    let temp = fixture("parity");
    let config = configs(temp.path()).parity;
    let original = std::fs::read_to_string(temp.path().join("docs/parity-checklist.md")).unwrap();
    let report = parity::compute(&config, &parity::Args::default()).unwrap();
    assert_data(&report);
    assert_eq!(report.weighted_score, 50.0);
    assert_eq!(report.rows, vec![("A".into(), "feature".into(), 0, 0.5)]);
    assert!(report.stale);
    assert_eq!(std::fs::read_to_string(&report.path).unwrap(), original);
    assert_eq!(parity::render(&report), parity::rewrite(&config, &original).unwrap());
    assert_eq!(run_output("parity", temp.path(), &["--check"]), "ERROR: parity checklist is stale");
    assert_eq!(run_output("parity", temp.path(), &[]), parity::render(&report));
    assert_eq!(run_output("parity", temp.path(), &["--check"]), parity::render(&report));
    let checked = parity::compute(&config, &parity::Args { check: true }).unwrap();
    assert!(!checked.stale);
    assert_eq!(checked.rows, report.rows);
}

#[test]
fn parity_scores_only_checklist_rows_and_handles_empty_checklists() {
    let temp = fixture("parity");
    let config = configs(temp.path()).parity;
    let path = temp.path().join("docs/parity-checklist.md");
    for (rows, expected_score, expected_count) in [("| A | feature | P0 | P |\n", 50.0, 1), ("", 0.0, 0)] {
        std::fs::write(&path, format!("| Example | not a feature | P0 | D |\n<!-- SUMMARY -->\nold\n## Rows\n{rows}")).unwrap();
        let report = parity::compute(&config, &parity::Args::default()).unwrap();
        assert_eq!(report.weighted_score, expected_score);
        assert_eq!(report.rows.len(), expected_count);
        assert_eq!(parity::render(&report), run_output("parity", temp.path(), &[]));
    }
}

#[test]
fn parity_command_reports_do_not_write_before_apply() {
    let temp = fixture("parity");
    let mut config = configs(temp.path()).parity;
    config.cargo.program = temp.path().join("cargo-fixture");
    config.command = Some(vec!["run".into()]);
    std::fs::write(&config.cargo.program, "#!/bin/sh\ncat generated.md\n").unwrap();
    let generated = "App-owned output\n| A | feature | P1 | D |\n";
    std::fs::write(temp.path().join("generated.md"), generated).unwrap();
    let path = temp.path().join("docs/parity-checklist.md");
    std::fs::remove_file(&path).unwrap();
    let report = parity::compute(&config, &parity::Args::default()).unwrap();
    assert_eq!(report.weighted_score, 100.0);
    assert_eq!(parity::render(&report), generated);
    assert!(!path.exists());
    parity::apply(&report).unwrap();
    assert_eq!(std::fs::read_to_string(path).unwrap(), generated);
    assert!(!parity::compute(&config, &parity::Args { check: true }).unwrap().stale);
}

#[test]
fn pdf_report_exposes_verified_pins_and_target_features() {
    let temp = fixture("pdf_deps");
    let mut config = configs(temp.path()).pdf_deps;
    let report = pdf_deps::compute(&config, &pdf_deps::Args).unwrap();
    assert_data(&report);
    assert_eq!(report.packages[0].features, ["read"]);
    assert_eq!(report.packages[0].source, "registry+fixture");
    assert_eq!(pdf_deps::render(&report), "PDF dependencies: pins, features and production graph verified\n");
    assert_eq!(run_output("pdf_deps", temp.path(), &[]), pdf_deps::render(&report));
    config.target_features.push(pdf_deps::TargetFeatures {
        root_package: "app".into(),
        target: Some("wasm32-unknown-unknown".into()),
        package: "pdf".into(),
        features: vec!["read".into()],
        depth: 20,
    });
    let targeted = pdf_deps::compute(&config, &pdf_deps::Args).unwrap();
    assert_eq!(targeted.target_features[0].target, "wasm32-unknown-unknown");
    assert_eq!(targeted.target_features[0].features, ["read"]);
    assert_ne!(targeted, report);
}

#[test]
fn typed_arguments_preserve_usage_errors() {
    let invalid = vec!["--invalid".into()];
    assert!(version::Args::parse(&invalid).is_err());
    assert!(stats::Args::parse(&invalid).is_err());
    assert!(layers::Args::parse(&invalid).is_err());
    assert!(parity::Args::parse(&invalid).is_err());
    assert!(pdf_deps::Args::parse(&invalid).is_err());
}
