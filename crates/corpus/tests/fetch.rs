//! Fetch/verify behaviour with an injected fetcher: no network.

use std::cell::RefCell;
use std::collections::HashMap;

use craft_corpus::{Error, Fetcher, Manifest, Result, STAMP_FILE, fetch_with, resolve_redirect, verify, verify_manifest};

struct Fake {
    files: HashMap<String, Vec<u8>>,
    calls: RefCell<Vec<String>>,
}

impl Fetcher for Fake {
    fn get(&self, url: &str, _max: u64) -> Result<Vec<u8>> {
        self.calls.borrow_mut().push(url.to_string());
        self.files.get(url).cloned().ok_or(Error::Fetch { url: url.into(), reason: "HTTP 404".into() })
    }
}

fn setup() -> (Manifest, Fake) {
    let a = b"alpha bytes".to_vec();
    let b = b"bravo bytes".to_vec();
    let text = format!(
        r#"
name = "demo"
[[file]]
path = "a.bin"
url = "https://example.org/a.bin"
sha256 = "{}"
licence = "CC0-1.0"
provenance = "test"
tags = ["tables"]

[[git]]
repo = "o/r"
commit = "ac0c3532476fa4876a83a279127840741cf216f2"
paths = ["d/*"]
strip_prefix = "d/"
licence = "MIT"
provenance = "test"
tags = ["rtl"]
[[git.file]]
path = "d/b.bin"
sha256 = "{}"
"#,
        craft_hash::sha256_hex(&a),
        craft_hash::sha256_hex(&b)
    );
    let m = Manifest::parse(&text).unwrap();
    let files = HashMap::from([
        ("https://example.org/a.bin".to_string(), a),
        ("https://raw.githubusercontent.com/o/r/ac0c3532476fa4876a83a279127840741cf216f2/d/b.bin".to_string(), b),
    ]);
    (m, Fake { files, calls: RefCell::new(Vec::new()) })
}

#[test]
fn fetch_writes_verified_tree_with_stamp_and_tags() {
    let (m, fake) = setup();
    let dir = tempfile::tempdir().unwrap();
    let r = fetch_with(&m, dir.path(), &fake).unwrap();
    assert_eq!((r.downloaded, r.reused, r.pruned), (2, 0, 0));
    assert_eq!(std::fs::read(dir.path().join("b.bin")).unwrap(), b"bravo bytes");
    assert!(dir.path().join(STAMP_FILE).is_file());
    let c = verify_manifest(dir.path(), &m).unwrap();
    assert_eq!(c.count(), 2);
    let tagged: Vec<String> = c.files_by_tag("rtl").into_iter().map(|f| f.rel).collect();
    assert_eq!(tagged, ["b.bin"]);
    assert_eq!(c.files_by_tag("tables")[0].path, dir.path().join("a.bin"));
    assert!(c.files_by_tag("charts").is_empty());
    // No temp files left behind.
    let names: Vec<_> = std::fs::read_dir(dir.path()).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    assert!(names.iter().all(|n| !n.ends_with(".tmp")), "{names:?}");
}

#[test]
fn second_fetch_reuses_files_without_network() {
    let (m, fake) = setup();
    let dir = tempfile::tempdir().unwrap();
    fetch_with(&m, dir.path(), &fake).unwrap();
    fake.calls.borrow_mut().clear();
    let r = fetch_with(&m, dir.path(), &fake).unwrap();
    assert_eq!((r.downloaded, r.reused), (0, 2));
    assert!(fake.calls.borrow().is_empty());
}

#[test]
fn hash_mismatch_writes_nothing_and_leaves_tree_unverified() {
    let (m, mut fake) = setup();
    fake.files.insert("https://example.org/a.bin".into(), b"evil".to_vec());
    let dir = tempfile::tempdir().unwrap();
    let err = fetch_with(&m, dir.path(), &fake).unwrap_err();
    assert!(matches!(err, Error::HashMismatch { ref path, .. } if path == "a.bin"), "{err}");
    assert!(!dir.path().join("a.bin").exists());
    assert!(matches!(verify(dir.path()).unwrap_err(), Error::Unverified { .. }));
}

#[test]
fn failed_refetch_invalidates_a_previously_good_tree() {
    let (m, mut fake) = setup();
    let dir = tempfile::tempdir().unwrap();
    fetch_with(&m, dir.path(), &fake).unwrap();
    std::fs::remove_file(dir.path().join("b.bin")).unwrap();
    fake.files.clear();
    assert!(fetch_with(&m, dir.path(), &fake).is_err());
    assert!(matches!(verify(dir.path()).unwrap_err(), Error::Unverified { .. }), "stale stamp must not survive");
}

#[test]
fn http_url_is_refused_before_any_request() {
    let (mut m, fake) = setup();
    m.file[0].url = "http://example.org/a.bin".into();
    let dir = tempfile::tempdir().unwrap();
    let err = fetch_with(&m, dir.path(), &fake).unwrap_err();
    assert!(matches!(err, Error::Insecure(_)), "{err}");
    assert!(fake.calls.borrow().is_empty());
}

#[test]
fn redirects_must_stay_on_https() {
    assert_eq!(resolve_redirect("https://a.org/x", "https://b.org/y").unwrap(), "https://b.org/y");
    assert_eq!(resolve_redirect("https://a.org/x/z", "/y").unwrap(), "https://a.org/y");
    assert_eq!(resolve_redirect("https://a.org/x", "//c.org/y").unwrap(), "https://c.org/y");
    for bad in ["http://b.org/y", "ftp://b.org/y", "file:///etc/passwd", "HTTP://b.org/y"] {
        assert!(matches!(resolve_redirect("https://a.org/x", bad).unwrap_err(), Error::Insecure(_)), "{bad}");
    }
    assert!(resolve_redirect("https://a.org/x", "relative/path").is_err());
}

fn fetched() -> (Manifest, tempfile::TempDir) {
    let (m, fake) = setup();
    let dir = tempfile::tempdir().unwrap();
    fetch_with(&m, dir.path(), &fake).unwrap();
    (m, dir)
}

#[test]
fn verify_refuses_edited_file() {
    let (_, dir) = fetched();
    std::fs::write(dir.path().join("a.bin"), b"alpha bytez").unwrap();
    assert!(matches!(verify(dir.path()).unwrap_err(), Error::Tampered { .. }));
}

#[test]
fn verify_refuses_missing_and_extra_files() {
    let (_, dir) = fetched();
    std::fs::write(dir.path().join("extra.bin"), b"x").unwrap();
    let err = verify(dir.path()).unwrap_err();
    assert!(matches!(err, Error::Tampered { .. }) && err.to_string().contains("extra.bin"), "{err}");
    std::fs::remove_file(dir.path().join("extra.bin")).unwrap();
    std::fs::remove_file(dir.path().join("a.bin")).unwrap();
    assert!(matches!(verify(dir.path()).unwrap_err(), Error::Tampered { .. }));
}

#[test]
fn verify_refuses_missing_or_garbled_stamp() {
    let (_, dir) = fetched();
    std::fs::write(dir.path().join(STAMP_FILE), b"{not json").unwrap();
    assert!(matches!(verify(dir.path()).unwrap_err(), Error::Unverified { .. }));
    std::fs::remove_file(dir.path().join(STAMP_FILE)).unwrap();
    assert!(matches!(verify(dir.path()).unwrap_err(), Error::Unverified { .. }));
}

#[cfg(unix)]
#[test]
fn verify_refuses_symlinks() {
    let (_, dir) = fetched();
    std::os::unix::fs::symlink("/etc/hosts", dir.path().join("link")).unwrap();
    assert!(matches!(verify(dir.path()).unwrap_err(), Error::Tampered { .. }));
}

#[test]
fn verify_manifest_detects_moved_pin() {
    let (m, dir) = fetched();
    let mut moved = m.clone();
    moved.git[0].commit = "bc0c3532476fa4876a83a279127840741cf216f2".into();
    assert!(verify_manifest(dir.path(), &m).is_ok());
    assert!(matches!(verify_manifest(dir.path(), &moved).unwrap_err(), Error::Stale { .. }));
}

#[test]
fn refetch_prunes_files_no_longer_in_manifest() {
    let (m, fake) = setup();
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("old")).unwrap();
    std::fs::write(dir.path().join("old/gone.bin"), b"x").unwrap();
    let r = fetch_with(&m, dir.path(), &fake).unwrap();
    assert_eq!(r.pruned, 1);
    assert!(verify(dir.path()).is_ok());
}

#[test]
fn macro_skips_when_absent() {
    fn body(reached: &mut bool) {
        let _c = craft_corpus::corpus_or_skip!("craft-corpus-test-definitely-absent", "/nonexistent/craft-corpus-path");
        *reached = true;
    }
    // Moot if the environment sets CRAFT_CORPUS_REQUIRE=1 (the macro would then panic).
    if !craft_corpus::is_required("craft-corpus-test-definitely-absent") {
        let mut reached = false;
        body(&mut reached);
        assert!(!reached, "macro should return early");
    }
}
