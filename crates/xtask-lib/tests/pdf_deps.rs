use craft_xtask::pdf_deps::{Config, PackagePin, TargetFeatures, git_source, validate, validate_target_features};
use serde_json::{Value, json};

fn fixture() -> (Config, Value) {
    let source = git_source("https://example.invalid/pdf", "abc123");
    let config = Config {
        packages: vec![PackagePin {
            name: "craft-pdf".into(),
            version: "1.0.0".into(),
            source: source.clone(),
            required_features: vec!["read".into()],
            exact_features: None,
        }],
        production_roots: vec!["app".into()],
        forbidden_production: vec!["hayro*".into()],
        ..Default::default()
    };
    let metadata = json!({"packages":[
        {"id":"app", "name":"app"},
        {"id":"pdf", "name":"craft-pdf", "version":"1.0.0", "source":source},
        {"id":"reader", "name":"hayro"}
    ], "resolve":{"nodes":[
        {"id":"app", "features":[], "deps":[{"pkg":"pdf", "dep_kinds":[{"kind":null}]}]},
        {"id":"pdf", "features":["read", "write"], "deps":[{"pkg":"reader", "dep_kinds":[{"kind":"dev"}]}]},
        {"id":"reader", "features":[], "deps":[]}
    ]}});
    (config, metadata)
}

#[test]
fn pins_reject_missing_duplicate_version_and_source_drift() {
    let (config, metadata) = fixture();
    validate(&config, &metadata).unwrap();
    for field in ["version", "source", "name"] {
        let mut changed = metadata.clone();
        changed["packages"][1][field] = json!("wrong");
        assert!(validate(&config, &changed).is_err());
    }
    let mut duplicate = metadata.clone();
    duplicate["packages"].as_array_mut().unwrap().push(metadata["packages"][1].clone());
    assert!(validate(&config, &duplicate).is_err());
    assert!(validate(&Config::default(), &metadata).is_err());
}

#[test]
fn features_support_required_and_exact_sets() {
    let (mut config, mut metadata) = fixture();
    config.packages[0].exact_features = Some(vec!["read".into()]);
    assert!(validate(&config, &metadata).is_err());
    metadata["resolve"]["nodes"][1]["features"] = json!(["read"]);
    validate(&config, &metadata).unwrap();
    metadata["resolve"]["nodes"][1]["features"] = json!([]);
    assert!(validate(&config, &metadata).is_err());
    metadata["resolve"]["nodes"][1]["features"] = json!([42]);
    assert!(validate(&config, &metadata).is_err());
}

#[test]
fn production_traversal_distinguishes_dev_normal_and_build() {
    let (config, metadata) = fixture();
    validate(&config, &metadata).unwrap();
    for kind in [json!(null), json!("build")] {
        let mut changed = metadata.clone();
        changed["resolve"]["nodes"][1]["deps"][0]["dep_kinds"][0]["kind"] = kind;
        assert!(validate(&config, &changed).unwrap_err().contains("production"));
    }
    let mut cycle = metadata.clone();
    cycle["resolve"]["nodes"][1]["deps"] = json!([{"pkg":"app", "dep_kinds":[{"kind":null}]}]);
    validate(&config, &cycle).unwrap();
}

#[test]
fn forbidden_active_checks_resolved_nodes_not_unused_packages() {
    let (mut config, mut metadata) = fixture();
    config.forbidden_active = vec!["hayro*".into()];
    assert!(validate(&config, &metadata).is_err());
    metadata["resolve"]["nodes"].as_array_mut().unwrap().pop();
    validate(&config, &metadata).unwrap();
}

#[test]
fn malformed_graphs_fail_closed() {
    let (config, metadata) = fixture();
    for pointer in ["/packages", "/resolve/nodes", "/resolve/nodes/0/deps", "/resolve/nodes/0/deps/0/dep_kinds", "/resolve/nodes/0/deps/0/pkg"] {
        let mut changed = metadata.clone();
        *changed.pointer_mut(pointer).unwrap() = Value::Null;
        assert!(validate(&config, &changed).is_err(), "{pointer}");
    }
    let mut missing = metadata;
    missing["resolve"]["nodes"][0]["deps"][0]["pkg"] = json!("missing");
    assert!(validate(&config, &missing).is_err());
}

#[test]
fn native_and_wasm_feature_rules_are_exact() {
    let mut rule = TargetFeatures { root_package: "app".into(), target: None, package: "hayro".into(), features: vec!["rayon".into()], depth: 20 };
    validate_target_features(&rule, "app v1|\nhayro v1|rayon\n").unwrap();
    assert!(validate_target_features(&rule, "hayro v1|rayon,extra").is_err());
    assert!(validate_target_features(&rule, "other v1|rayon").is_err());
    rule.target = Some("wasm32-unknown-unknown".into());
    rule.features.clear();
    validate_target_features(&rule, "hayro v1|\n").unwrap();
    assert!(validate_target_features(&rule, "hayro v1|rayon").is_err());
}
