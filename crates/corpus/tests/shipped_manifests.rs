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

fn load(name: &str) -> Manifest {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("manifests/{name}.toml"));
    Manifest::load(&path).unwrap_or_else(|err| panic!("{}: {err}", path.display()))
}

fn count_ext(m: &Manifest, exts: &[&str]) -> usize {
    m.entries().iter().filter(|e| exts.iter().any(|x| e.path.to_ascii_lowercase().ends_with(&format!(".{x}")))).count()
}

#[test]
fn office_and_cad_manifests_are_pinned_redistributable_and_tagged() {
    const OFFICE: &[&str] = &["docx", "xlsx", "pptx", "doc", "xls", "ppt", "docm", "xlsm", "pptm"];
    let cases = [
        ("office-poi", "apache/poi", "Apache-2.0"),
        ("office-libreoffice", "LibreOffice/core", "MPL-2.0"),
        ("office-openxml-sdk", "dotnet/Open-XML-SDK", "MIT"),
        ("cad-ezdxf", "mozman/ezdxf", "MIT"),
    ];
    let mut office = 0;
    let mut dxf = 0;
    for (name, repo, licence) in cases {
        let m = load(name);
        assert_eq!(m.name, name);
        assert_eq!(m.git.len(), 1, "{name}");
        assert_eq!(m.git[0].repo, repo);
        assert_eq!(m.git[0].commit.len(), 40, "{name}");
        assert!(m.git[0].commit.bytes().all(|b| b.is_ascii_hexdigit()));
        let entries = m.entries();
        assert!(!entries.is_empty(), "{name}");
        for e in &entries {
            assert_eq!(e.licence, licence, "{}", e.path);
            assert!(e.redistributable, "{}", e.path);
            assert!(e.url.starts_with(&format!("https://raw.githubusercontent.com/{repo}/{}/", m.git[0].commit)), "{}", e.url);
            assert!(!e.tags.is_empty(), "{} has no feature tags", e.path);
            let lower = e.path.to_ascii_lowercase();
            for bad in ["crash", "fuzz", "cve"] {
                assert!(!lower.contains(bad), "{} should have been excluded", e.path);
            }
        }
        if name == "cad-ezdxf" {
            dxf += count_ext(&m, &["dxf"]);
        } else {
            assert_eq!(count_ext(&m, OFFICE), entries.len(), "{name}: non-office file listed");
            office += entries.len();
        }
    }
    assert!(office >= 500, "office files: {office}");
    assert!(dxf >= 100, "dxf files: {dxf}");
}

#[test]
fn office_manifests_cover_the_feature_tags() {
    let mut all = Vec::new();
    for n in ["office-poi", "office-libreoffice", "office-openxml-sdk"] {
        all.extend(load(n).entries());
    }
    for tag in ["comments", "tracked-changes", "charts", "pivots", "tables", "rtl", "fields", "macros", "embedded-objects", "math", "large"] {
        assert!(all.iter().any(|e| e.tags.iter().any(|t| t == tag)), "no office file tagged {tag}");
    }
    for ext in ["docx", "xlsx", "pptx", "doc", "xls", "ppt"] {
        assert!(all.iter().any(|e| e.tags.iter().any(|t| t == ext)), "no {ext} file");
    }
}
