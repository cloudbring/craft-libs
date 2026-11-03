use craft_xtask::layers::{self, Config, Crate, Dep, DepKind};
use serde_json::json;

fn fixture(name: &str) -> (Config, Vec<Crate>) {
    let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/layers");
    let config = toml::from_str(&std::fs::read_to_string(base.join(format!("{name}.toml"))).unwrap()).unwrap();
    let metadata = serde_json::from_str(&std::fs::read_to_string(base.join(format!("{name}.json"))).unwrap()).unwrap();
    (config, layers::from_metadata(&metadata).unwrap())
}

macro_rules! app_graph {
    ($($name:ident),+ $(,)?) => { $(
        #[test]
        fn $name() {
            let (config, mut graph) = fixture(stringify!($name));
            assert_eq!(layers::check(&config, &graph).unwrap(), Vec::<String>::new(), "current graph");
            let report = layers::check_graph(&config, &graph).unwrap();
            assert_eq!(layers::render(&report), "layers: ok\n");
            let source = config.layers[0].1[0].clone();
            let target = config.layers.iter().rev().flat_map(|(_, names)| names).find(|name| !config.testkits.contains(name)).unwrap().clone();
            let krate = graph.iter_mut().find(|krate| krate.name == source).unwrap();
            krate.deps.push(Dep { name: target, kind: DepKind::Normal, workspace: true });
            assert!(!layers::check(&config, &graph).unwrap().is_empty(), "upward edge must fail");
            let changed = layers::check_graph(&config, &graph).unwrap();
            assert_ne!(report, changed);
            assert!(!changed.ok);
            assert_eq!(layers::render(&changed), layers::check(&config, &graph).unwrap().join("\n"));
        }
    )+ };
}

app_graph!(
    photocraft,
    lightcraft,
    texturecraft,
    vectorcraft,
    figcraft,
    designcraft,
    deckcraft,
    cadcraft,
    wordcraft,
    pdfcraft,
    gridcraft,
    materialcraft
);

fn edge(name: &str, dependency: &str, kind: DepKind, workspace: bool) -> Crate {
    Crate { name: name.into(), deps: vec![Dep { name: dependency.into(), kind, workspace }] }
}

#[test]
fn standalone_testkit_and_same_layer_policies() {
    let (config, _) = fixture("photocraft");
    assert!(layers::check(&config, &[edge("photocraft-codecs", "photocraft-heif", DepKind::Normal, true)]).unwrap().is_empty());
    for graph in [
        edge("photocraft-heif", "photocraft-codecs", DepKind::Normal, true),
        edge("photocraft-doc", "photocraft-testkit", DepKind::Build, true),
        edge("photocraft-color", "photocraft-raster", DepKind::Normal, true),
        edge("photocraft-doc", "bevy_render", DepKind::Dev, false),
        edge("photocraft-new", "serde", DepKind::Normal, false),
    ] {
        assert!(!layers::check(&config, &[graph]).unwrap().is_empty());
    }
}

#[test]
fn dev_exceptions_do_not_allow_production_edges() {
    let (config, _) = fixture("deckcraft");
    assert!(layers::check(&config, &[edge("deckcraft-pptx", "deckcraft-engine", DepKind::Dev, true)]).unwrap().is_empty());
    assert!(!layers::check(&config, &[edge("deckcraft-pptx", "deckcraft-engine", DepKind::Normal, true)]).unwrap().is_empty());
}

#[test]
fn gpu_network_and_format_allowlists() {
    for (app, graph) in [
        ("texturecraft", edge("texturecraft-engine", "wgpu-core", DepKind::Normal, false)),
        ("figcraft", edge("figcraft-doc", "tokio", DepKind::Build, false)),
        ("gridcraft", edge("gridcraft-xlsx", "gridcraft-numfmt", DepKind::Normal, true)),
    ] {
        let (config, _) = fixture(app);
        assert!(!layers::check(&config, &[graph]).unwrap().is_empty());
    }
}

#[test]
fn pdf_test_support_exemptions() {
    let (config, _) = fixture("pdfcraft");
    for graph in [
        edge("pdfcraft-cos", "pdfcraft-oracle", DepKind::Dev, true),
        edge("pdfcraft-oracle", "pdfcraft-testkit", DepKind::Normal, true),
        edge("pdfcraft-geom", "egui_kittest", DepKind::Dev, false),
    ] {
        assert!(layers::check(&config, &[graph]).unwrap().is_empty());
    }
    assert!(!layers::check(&config, &[edge("pdfcraft-cos", "pdfcraft-oracle", DepKind::Normal, true)]).unwrap().is_empty());
}

#[test]
fn metadata_uses_members_not_external_paths_or_aliases() {
    let metadata = json!({"workspace_members":["a","b"], "packages":[
        {"id":"a", "name":"a", "dependencies":[{"name":"b","rename":"alias","kind":"build","path":"/b"}, {"name":"shared","kind":null,"path":"/outside"}]},
        {"id":"b", "name":"b", "dependencies":[]},
        {"id":"shared", "name":"shared", "dependencies":[]}
    ]});
    let graph = layers::from_metadata(&metadata).unwrap();
    assert_eq!(graph.len(), 2);
    assert!(graph[0].deps[0].workspace);
    assert!(!graph[0].deps[1].workspace);
    assert_eq!(graph[0].deps[0].name, "b");
    assert!(layers::from_metadata(&json!({})).is_err());
}

#[test]
fn invalid_and_ambiguous_configuration_is_rejected() {
    let (mut config, graph) = fixture("photocraft");
    config.layers[0].1.push("[".into());
    assert!(layers::check(&config, &graph).is_err());
    config.layers[0].1.pop();
    config.layers[1].1.push("photocraft-geom".into());
    assert!(layers::check(&config, &graph).is_err());
}
