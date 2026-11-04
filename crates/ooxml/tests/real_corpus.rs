//! Opens real Office files that the old per-app readers accepted. Needs
//! `CRAFT_OFFICE_CORPUS=/path/to/corpus`; run with `cargo test -- --ignored`.

use std::path::{Path, PathBuf};

use craft_ooxml::opc::{LazyLimits, LazyPackage, OpcLimits, Package, Tolerance};

fn find(dir: &Path, name: &str) -> Option<PathBuf> {
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Some(found) = find(&path, name) {
                return Some(found);
            }
        } else if path.file_name().is_some_and(|file| file == name) {
            return Some(path);
        }
    }
    None
}

#[test]
#[ignore = "needs CRAFT_OFFICE_CORPUS"]
fn tolerance_opens_previously_rejected_real_files() {
    let Ok(root) = std::env::var("CRAFT_OFFICE_CORPUS") else {
        eprintln!("CRAFT_OFFICE_CORPUS not set; skipping");
        return;
    };
    for name in ["49609.xlsx", "tdf131575.xlsx", "tdf76115.xlsx", "54764-2.xlsx"] {
        let path = find(Path::new(&root), name).unwrap_or_else(|| panic!("{name} not in corpus"));
        let bytes = std::fs::read(&path).expect("read");
        let package = Package::open_with(&bytes, &OpcLimits::default(), &Tolerance::lenient()).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(package.part("xl/workbook.xml").is_some(), "{name}: workbook part (eager)");
        assert!(
            !package.content_types.overrides.is_empty() || !package.content_types.defaults.is_empty(),
            "{name}: content types (eager): {:?}",
            package.warnings()
        );

        let lazy = LazyPackage::open_with(bytes, &OpcLimits::default(), LazyLimits::lenient()).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(lazy.find("xl/workbook.xml").is_some(), "{name}: workbook part (lazy)");
        assert!(!lazy.content_types.overrides.is_empty() || !lazy.content_types.defaults.is_empty(), "{name}: content types (lazy)");
        eprintln!("{name}: ok");
    }
}
