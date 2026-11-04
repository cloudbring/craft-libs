//! Every manifest under `manifests/` parses, is pinned, and is HTTPS-only.

use craft_corpus::Manifest;

#[test]
fn shipped_manifests_are_valid() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("manifests");
    let mut n = 0;
    for e in std::fs::read_dir(dir).unwrap() {
        let path = e.unwrap().path();
        if path.extension().is_some_and(|x| x == "toml") {
            let m = Manifest::load(&path).unwrap_or_else(|err| panic!("{}: {err}", path.display()));
            assert!(!m.entries().is_empty(), "{}", path.display());
            n += 1;
        }
    }
    assert!(n >= 1);
}

#[test]
fn pngsuite_manifest_is_pinned_to_a_commit() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("manifests/pngsuite.toml");
    let m = Manifest::load(&path).unwrap();
    assert_eq!(m.name, "pngsuite");
    assert_eq!(m.git[0].commit.len(), 40);
    assert_eq!(m.entries().len(), 62);
    assert!(m.entries().iter().all(|e| e.url.starts_with("https://raw.githubusercontent.com/pnggroup/libpng/ac0c3532")));
    assert!(!m.entries_with_tag("interlaced").is_empty());
}
