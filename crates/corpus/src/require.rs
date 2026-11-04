//! Requirement gates: absence of a corpus is a skip locally and a failure in CI.

use std::path::Path;

use crate::error::{Error, Result};
use crate::fetch::{Corpus, verify, verify_manifest};
use crate::manifest::{Manifest, STAMP_FILE};

/// The global switch: `CRAFT_CORPUS_REQUIRE=1` makes every absent corpus an error.
pub const GLOBAL_VAR: &str = "CRAFT_CORPUS_REQUIRE";

/// The per-corpus variable: `pngsuite` -> `PNGSUITE_CORPUS_REQUIRED`, `psd-tools` ->
/// `PSD_TOOLS_CORPUS_REQUIRED`.
#[must_use]
pub fn per_corpus_var(name: &str) -> String {
    let up: String = name.chars().map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_uppercase() } else { '_' }).collect();
    format!("{up}_CORPUS_REQUIRED")
}

/// Which variable (if any) makes `name` required, reading the process environment.
#[must_use]
pub fn required_by(name: &str) -> Option<String> {
    required_by_with(name, |k| std::env::var(k).ok())
}

/// [`required_by`] with an injected environment lookup. Only the value `1` counts.
#[must_use]
pub fn required_by_with(name: &str, get: impl Fn(&str) -> Option<String>) -> Option<String> {
    let on = |k: &str| get(k).is_some_and(|v| v.trim() == "1");
    if on(GLOBAL_VAR) {
        return Some(GLOBAL_VAR.to_string());
    }
    let own = per_corpus_var(name);
    on(&own).then_some(own)
}

/// True when `name` must be present.
#[must_use]
pub fn is_required(name: &str) -> bool {
    required_by(name).is_some()
}

/// Loads the corpus at `root` for a test.
///
/// - Verified tree: `Ok(Some(corpus))`.
/// - Absent (no directory, or empty): `Ok(None)` after printing a `SKIP` line, unless required,
///   which is [`Error::Missing`].
/// - Present but unstamped, tampered or (with `manifest`) stale: always an error, required or not.
///
/// # Errors
/// As above.
pub fn load_or_skip(name: &str, root: &Path, manifest: Option<&Manifest>) -> Result<Option<Corpus>> {
    load_or_skip_with(name, root, manifest, |k| std::env::var(k).ok())
}

/// [`load_or_skip`] with an injected environment lookup.
///
/// # Errors
/// As [`load_or_skip`].
pub fn load_or_skip_with(name: &str, root: &Path, manifest: Option<&Manifest>, get: impl Fn(&str) -> Option<String>) -> Result<Option<Corpus>> {
    if is_absent(root) {
        return match required_by_with(name, get) {
            Some(var) => Err(Error::Missing { name: name.to_string(), root: root.to_path_buf(), hint: format!("{var}=1 is set") }),
            None => {
                println!(
                    "SKIP corpus `{name}`: not found at {} (set {GLOBAL_VAR}=1 or {}=1 to make this a failure)",
                    root.display(),
                    per_corpus_var(name)
                );
                Ok(None)
            }
        };
    }
    match manifest {
        Some(m) => verify_manifest(root, m).map(Some),
        None => verify(root).map(Some),
    }
}

fn is_absent(root: &Path) -> bool {
    match std::fs::read_dir(root) {
        Err(_) => true,
        Ok(mut it) => it.next().is_none() && !root.join(STAMP_FILE).exists(),
    }
}

/// In a `#[test]`: yields the verified [`Corpus`], or prints `SKIP ...` and returns from the test
/// when the corpus is absent and not required. Panics on a missing-but-required, unverified,
/// tampered or stale corpus.
///
/// ```no_run
/// #[test]
/// fn pngsuite_decodes() {
///     let corpus = craft_corpus::corpus_or_skip!("pngsuite", "corpus/pngsuite");
///     for f in corpus.files() {
///         let _ = std::fs::read(&f.path).unwrap();
///     }
/// }
/// ```
///
/// A third argument, a `&Manifest`, additionally checks the tree matches the manifest.
#[macro_export]
macro_rules! corpus_or_skip {
    ($name:expr, $root:expr) => {
        $crate::corpus_or_skip!($name, $root, ::core::option::Option::None)
    };
    ($name:expr, $root:expr, $manifest:expr) => {
        match $crate::load_or_skip($name, ::std::path::Path::new(&$root), $manifest) {
            ::core::result::Result::Ok(::core::option::Option::Some(c)) => c,
            ::core::result::Result::Ok(::core::option::Option::None) => return,
            ::core::result::Result::Err(e) => ::core::panic!("{e}"),
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let m: HashMap<String, String> = pairs.iter().map(|(k, v)| ((*k).to_string(), (*v).to_string())).collect();
        move |k| m.get(k).cloned()
    }

    #[test]
    fn var_names() {
        assert_eq!(per_corpus_var("pngsuite"), "PNGSUITE_CORPUS_REQUIRED");
        assert_eq!(per_corpus_var("psd-tools"), "PSD_TOOLS_CORPUS_REQUIRED");
    }

    #[test]
    fn only_one_enables() {
        assert_eq!(required_by_with("x", env(&[])), None);
        assert_eq!(required_by_with("x", env(&[("CRAFT_CORPUS_REQUIRE", "0")])), None);
        assert_eq!(required_by_with("x", env(&[("CRAFT_CORPUS_REQUIRE", "true")])), None);
        assert_eq!(required_by_with("x", env(&[("CRAFT_CORPUS_REQUIRE", "1")])).as_deref(), Some("CRAFT_CORPUS_REQUIRE"));
        assert_eq!(required_by_with("x", env(&[("X_CORPUS_REQUIRED", "1")])).as_deref(), Some("X_CORPUS_REQUIRED"));
        assert_eq!(required_by_with("x", env(&[("Y_CORPUS_REQUIRED", "1")])), None);
    }

    #[test]
    fn absent_skips_unless_required() {
        let dir = tempfile::tempdir().unwrap();
        let gone = dir.path().join("nope");
        assert!(load_or_skip_with("x", &gone, None, env(&[])).unwrap().is_none());
        assert!(load_or_skip_with("x", dir.path(), None, env(&[])).unwrap().is_none(), "empty dir counts as absent");
        let err = load_or_skip_with("x", &gone, None, env(&[("X_CORPUS_REQUIRED", "1")])).unwrap_err();
        assert!(matches!(err, Error::Missing { .. }), "{err}");
        assert!(load_or_skip_with("x", &gone, None, env(&[("CRAFT_CORPUS_REQUIRE", "1")])).is_err());
    }

    #[test]
    fn unstamped_tree_is_an_error_even_when_not_required() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.png"), b"x").unwrap();
        let err = load_or_skip_with("x", dir.path(), None, env(&[])).unwrap_err();
        assert!(matches!(err, Error::Unverified { .. }), "{err}");
    }
}
