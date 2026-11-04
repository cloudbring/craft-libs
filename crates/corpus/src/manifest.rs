//! The TOML manifest: what a corpus is, where each file comes from and what it must hash to.
//!
//! ```toml
//! name = "example"
//! description = "optional"
//!
//! # A file fetched from an HTTPS URL.
//! [[file]]
//! path = "dir/a.png"            # corpus-relative destination
//! url = "https://example.org/a.png"
//! sha256 = "<64 lowercase hex>"
//! licence = "CC0-1.0"
//! provenance = "who made it, where it came from"
//! tags = ["tables", "rtl"]      # optional feature tags
//! redistributable = true        # optional, default true
//!
//! # Files from a GitHub repository at a pinned commit. `paths` are globs every file must match.
//! [[git]]
//! repo = "owner/repo"
//! commit = "<40 hex>"
//! paths = ["tests/data/**"]
//! strip_prefix = "tests/data/"  # optional, removed from the repo path to get the destination
//! licence = "MIT"
//! provenance = "..."
//! tags = []                     # optional, added to every file of this source
//! redistributable = true        # optional
//!
//! [[git.file]]
//! path = "tests/data/b.bin"     # path inside the repository
//! sha256 = "<64 hex>"
//! dest = "b.bin"                # optional override
//! tags = ["charts"]             # optional, added to the source tags
//! licence = "..."               # optional override
//! provenance = "..."            # optional override
//! redistributable = false       # optional override
//! ```

use std::collections::BTreeSet;

use serde::Deserialize;

use crate::error::{Error, Result};
use crate::glob::glob_match;

/// Name of the stamp file written at the corpus root. Reserved: no corpus file may use it.
pub const STAMP_FILE: &str = ".craft-corpus.json";

fn yes() -> bool {
    true
}

/// A parsed manifest.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    /// Corpus name: `[a-z0-9_-]+`. Drives the `<NAME>_CORPUS_REQUIRED` variable.
    pub name: String,
    /// Free text, not part of the cache key.
    #[serde(default)]
    pub description: Option<String>,
    /// Files fetched from a URL.
    #[serde(default)]
    pub file: Vec<UrlFile>,
    /// Git repository sources.
    #[serde(default)]
    pub git: Vec<GitSource>,
}

/// One file fetched from an HTTPS URL.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct UrlFile {
    /// Corpus-relative destination.
    pub path: String,
    /// HTTPS URL.
    pub url: String,
    /// Lowercase hex SHA-256 of the file.
    pub sha256: String,
    /// SPDX id or licence text reference.
    pub licence: String,
    /// Where the file came from.
    pub provenance: String,
    /// Feature tags.
    #[serde(default)]
    pub tags: Vec<String>,
    /// False when the file may not be redistributed (kept out of published bundles).
    #[serde(default = "yes")]
    pub redistributable: bool,
}

/// A GitHub repository at a pinned commit.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct GitSource {
    /// `owner/repo` on github.com.
    pub repo: String,
    /// Full 40-hex commit id.
    pub commit: String,
    /// Globs (`*`, `?`, `**`) every listed file must match.
    pub paths: Vec<String>,
    /// Prefix removed from the repository path to form the destination.
    #[serde(default)]
    pub strip_prefix: Option<String>,
    /// Licence of the files.
    pub licence: String,
    /// Where the files came from.
    pub provenance: String,
    /// Tags added to every file.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Default for the files of this source.
    #[serde(default = "yes")]
    pub redistributable: bool,
    /// The files, each with its own hash.
    #[serde(default)]
    pub file: Vec<GitFile>,
}

/// One file of a [`GitSource`].
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct GitFile {
    /// Path inside the repository.
    pub path: String,
    /// Lowercase hex SHA-256.
    pub sha256: String,
    /// Destination override.
    #[serde(default)]
    pub dest: Option<String>,
    /// Extra tags.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Licence override.
    #[serde(default)]
    pub licence: Option<String>,
    /// Provenance override.
    #[serde(default)]
    pub provenance: Option<String>,
    /// Redistribution override.
    #[serde(default)]
    pub redistributable: Option<bool>,
}

/// A manifest file with everything resolved: one flat record per file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// Corpus-relative destination, `/`-separated.
    pub path: String,
    /// The HTTPS URL to GET.
    pub url: String,
    /// Pinned lowercase hex SHA-256.
    pub sha256: String,
    /// Licence.
    pub licence: String,
    /// Provenance.
    pub provenance: String,
    /// Sorted, deduplicated tags.
    pub tags: Vec<String>,
    /// Redistribution permitted.
    pub redistributable: bool,
}

impl Manifest {
    /// Parses and validates manifest text.
    ///
    /// # Errors
    /// [`Error::Parse`] for bad TOML or unknown keys, [`Error::Invalid`] for rule violations.
    pub fn parse(text: &str) -> Result<Self> {
        let m: Self = toml::from_str(text).map_err(|e| Error::Parse(e.to_string()))?;
        m.validate()?;
        Ok(m)
    }

    /// Reads and parses a manifest file.
    ///
    /// # Errors
    /// I/O, parse or validation errors.
    pub fn load(path: &std::path::Path) -> Result<Self> {
        let text = std::fs::read_to_string(path).map_err(|e| Error::io(format!("read {}", path.display()), e))?;
        Self::parse(&text)
    }

    /// Checks every rule; called by [`Manifest::parse`].
    ///
    /// # Errors
    /// [`Error::Invalid`] naming the first violation.
    pub fn validate(&self) -> Result<()> {
        let bad = |m: String| Err(Error::Invalid(m));
        if !valid_name(&self.name) {
            return bad(format!("name {:?} must match [a-z0-9_-]+", self.name));
        }
        for g in &self.git {
            validate_git(g)?;
        }
        let mut seen = BTreeSet::new();
        for e in self.entries() {
            validate_entry(&e)?;
            if !seen.insert(e.path.clone()) {
                return bad(format!("duplicate destination {}", e.path));
            }
        }
        Ok(())
    }

    /// Every file, resolved, in manifest order (URL files first, then git sources).
    #[must_use]
    pub fn entries(&self) -> Vec<Entry> {
        let mut out: Vec<Entry> = self.file.iter().map(url_entry).collect();
        for g in &self.git {
            out.extend(g.file.iter().map(|f| git_entry(g, f)));
        }
        out
    }

    /// Entries carrying `tag`.
    #[must_use]
    pub fn entries_with_tag(&self, tag: &str) -> Vec<Entry> {
        self.entries().into_iter().filter(|e| e.tags.iter().any(|t| t == tag)).collect()
    }

    /// Hash of the manifest's content for CI caches: changes when any file, pin, hash, licence,
    /// provenance, tag or flag changes; does not change for comments, whitespace, key order or
    /// `description`. Use it as the cache key suffix, e.g. `corpus-pngsuite-<key>`.
    #[must_use]
    pub fn cache_key(&self) -> String {
        let mut entries = self.entries();
        entries.sort_by(|a, b| a.path.cmp(&b.path));
        let mut text = format!("craft-corpus-v1\nname={}\n", self.name);
        for e in &entries {
            text.push_str(&format!(
                "{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
                e.path,
                e.url,
                e.sha256,
                e.licence,
                e.provenance,
                e.tags.join(","),
                e.redistributable
            ));
        }
        craft_hash::sha256_hex(text.as_bytes())
    }
}

fn url_entry(f: &UrlFile) -> Entry {
    Entry {
        path: f.path.clone(),
        url: f.url.clone(),
        sha256: f.sha256.clone(),
        licence: f.licence.clone(),
        provenance: f.provenance.clone(),
        tags: norm_tags(f.tags.iter()),
        redistributable: f.redistributable,
    }
}

fn git_entry(g: &GitSource, f: &GitFile) -> Entry {
    let dest = f.dest.clone().unwrap_or_else(|| {
        let stripped = g.strip_prefix.as_deref().and_then(|p| f.path.strip_prefix(p));
        stripped.unwrap_or(&f.path).to_string()
    });
    Entry {
        path: dest,
        url: format!("https://raw.githubusercontent.com/{}/{}/{}", g.repo, g.commit, encode_url_path(&f.path)),
        sha256: f.sha256.clone(),
        licence: f.licence.clone().unwrap_or_else(|| g.licence.clone()),
        provenance: f.provenance.clone().unwrap_or_else(|| g.provenance.clone()),
        tags: norm_tags(g.tags.iter().chain(f.tags.iter())),
        redistributable: f.redistributable.unwrap_or(g.redistributable),
    }
}

/// Percent-encode a repository path for a URL (RFC 3986): every byte except the unreserved set
/// `A-Za-z0-9-._~` is written as `%XX` (UTF-8 bytes for non-ASCII); `/` separators are kept.
pub fn encode_url_path(path: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = String::with_capacity(path.len());
    for b in path.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~' | b'/') {
            out.push(char::from(b));
        } else {
            out.push('%');
            out.push(char::from(HEX[usize::from(b >> 4)]));
            out.push(char::from(HEX[usize::from(b & 15)]));
        }
    }
    out
}

fn norm_tags<'a>(tags: impl Iterator<Item = &'a String>) -> Vec<String> {
    tags.cloned().collect::<BTreeSet<_>>().into_iter().collect()
}

fn valid_name(n: &str) -> bool {
    !n.is_empty() && n.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-')
}

fn is_hex(s: &str, len: usize) -> bool {
    s.len() == len && s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn validate_git(g: &GitSource) -> Result<()> {
    let bad = |m: String| Err(Error::Invalid(m));
    let parts: Vec<&str> = g.repo.split('/').collect();
    let part_ok =
        |p: &&str| !p.is_empty() && p.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.')) && *p != "." && *p != "..";
    if parts.len() != 2 || !parts.iter().all(part_ok) {
        return bad(format!("git repo {:?} must be owner/repo", g.repo));
    }
    if !is_hex(&g.commit, 40) {
        return bad(format!("git commit {:?} must be 40 lowercase hex digits (a branch or tag is not a pin)", g.commit));
    }
    if g.paths.is_empty() {
        return bad(format!("git source {} needs at least one path glob", g.repo));
    }
    for f in &g.file {
        if !safe_rel(&f.path) {
            return bad(format!("unsafe repository path {:?}", f.path));
        }
        if !g.paths.iter().any(|p| glob_match(p, &f.path)) {
            return bad(format!("{} in {} matches none of the path globs {:?}", f.path, g.repo, g.paths));
        }
    }
    Ok(())
}

fn validate_entry(e: &Entry) -> Result<()> {
    let bad = |m: String| Err(Error::Invalid(m));
    if !safe_rel(&e.path) || e.path == STAMP_FILE {
        return bad(format!("unsafe destination {:?}", e.path));
    }
    if !is_hex(&e.sha256, 64) {
        return bad(format!("{}: sha256 must be 64 lowercase hex digits", e.path));
    }
    if !e.url.starts_with("https://") {
        return Err(Error::Insecure(e.url.clone()));
    }
    if e.licence.trim().is_empty() || e.provenance.trim().is_empty() {
        return bad(format!("{}: licence and provenance are required", e.path));
    }
    if e.tags.iter().any(|t| t.is_empty() || t.contains(',') || t.contains(char::is_whitespace)) {
        return bad(format!("{}: tags must be non-empty words without commas or spaces", e.path));
    }
    Ok(())
}

/// A relative path of plain `/`-separated names: no root, no `.`/`..`, no backslash, no empty or
/// control segments.
#[must_use]
pub fn safe_rel(p: &str) -> bool {
    !p.is_empty()
        && !p.contains('\\')
        && !p.contains(|c: char| c.is_control() || c == ':')
        && p.split('/').all(|s| !s.is_empty() && s != "." && s != "..")
}

#[cfg(test)]
mod tests {
    use super::*;

    const H: &str = "0000000000000000000000000000000000000000000000000000000000000001";
    const C: &str = "ac0c3532476fa4876a83a279127840741cf216f2";

    fn sample() -> String {
        format!(
            r#"
name = "demo"
[[file]]
path = "a/one.bin"
url = "https://example.org/one.bin"
sha256 = "{H}"
licence = "CC0-1.0"
provenance = "example.org"
tags = ["tables", "rtl"]

[[git]]
repo = "owner/repo"
commit = "{C}"
paths = ["data/**/*.png"]
strip_prefix = "data/"
licence = "MIT"
provenance = "owner/repo"
tags = ["charts"]
redistributable = false

[[git.file]]
path = "data/x/two.png"
sha256 = "{H}"
tags = ["rtl"]
"#
        )
    }

    #[test]
    fn parses_and_resolves_entries() {
        let m = Manifest::parse(&sample()).unwrap();
        let e = m.entries();
        assert_eq!(e.len(), 2);
        assert_eq!(e[0].path, "a/one.bin");
        assert!(e[0].redistributable);
        assert_eq!(e[0].tags, ["rtl", "tables"]);
        assert_eq!(e[1].path, "x/two.png");
        assert_eq!(e[1].url, format!("https://raw.githubusercontent.com/owner/repo/{C}/data/x/two.png"));
        assert_eq!(e[1].licence, "MIT");
        assert!(!e[1].redistributable);
        assert_eq!(e[1].tags, ["charts", "rtl"]);
    }

    #[test]
    fn tag_iteration() {
        let m = Manifest::parse(&sample()).unwrap();
        let paths = |t: &str| m.entries_with_tag(t).into_iter().map(|e| e.path).collect::<Vec<_>>();
        assert_eq!(paths("rtl"), ["a/one.bin", "x/two.png"]);
        assert_eq!(paths("tables"), ["a/one.bin"]);
        assert!(paths("none").is_empty());
    }

    fn rejects(text: String, needle: &str) {
        let err = Manifest::parse(&text).unwrap_err().to_string();
        assert!(err.contains(needle), "{err:?} lacks {needle:?}");
    }

    #[test]
    fn rejects_http_urls() {
        rejects(sample().replace("https://example.org", "http://example.org"), "not https");
    }

    #[test]
    fn rejects_bad_hash_unpinned_commit_and_traversal() {
        rejects(sample().replacen(H, "abc", 1), "sha256");
        rejects(sample().replace(C, "main"), "40 lowercase hex");
        rejects(sample().replace("a/one.bin", "../one.bin"), "unsafe destination");
        rejects(sample().replace("a/one.bin", ".craft-corpus.json"), "unsafe destination");
        rejects(sample().replace("data/x/two.png", "data/x/two.txt"), "matches none of the path globs");
    }

    #[test]
    fn rejects_missing_licence_unknown_keys_and_duplicates() {
        rejects(sample().replace("licence = \"CC0-1.0\"\n", ""), "licence");
        rejects(sample().replace("name = \"demo\"", "name = \"demo\"\nbogus = 1"), "bogus");
        rejects(sample().replace("name = \"demo\"", "name = \"Demo\""), "name");
        let dup = format!(
            "{}\n[[file]]\npath = \"a/one.bin\"\nurl = \"https://example.org/2\"\nsha256 = \"{H}\"\nlicence = \"MIT\"\nprovenance = \"p\"\n",
            sample()
        );
        rejects(dup, "duplicate");
    }

    #[test]
    fn cache_key_tracks_content_not_formatting() {
        let a = Manifest::parse(&sample()).unwrap().cache_key();
        assert_eq!(a.len(), 64);
        let reformatted = format!("# comment\n{}", sample().replace("[[file]]", "\n\n[[file]]")).replace("description", "description");
        assert_eq!(a, Manifest::parse(&reformatted).unwrap().cache_key());
        let described = sample().replace("name = \"demo\"", "name = \"demo\"\ndescription = \"words\"");
        assert_eq!(a, Manifest::parse(&described).unwrap().cache_key());
        let moved = sample().replace(C, "bc0c3532476fa4876a83a279127840741cf216f2");
        assert_ne!(a, Manifest::parse(&moved).unwrap().cache_key());
        let relicensed = sample().replace("CC0-1.0", "MIT");
        assert_ne!(a, Manifest::parse(&relicensed).unwrap().cache_key());
    }

    #[test]
    fn url_path_encoding_classes() {
        assert_eq!(encode_url_path("a/b-c_d.e~f/G9"), "a/b-c_d.e~f/G9", "unreserved and separators kept");
        assert_eq!(encode_url_path("my file.doc"), "my%20file.doc", "space");
        assert_eq!(encode_url_path("100%.doc"), "100%25.doc", "percent");
        assert_eq!(encode_url_path("a&b=c+d?e#f.doc"), "a%26b%3Dc%2Bd%3Fe%23f.doc", "reserved");
        assert_eq!(encode_url_path("caf\u{e9}/\u{65e5}.docx"), "caf%C3%A9/%E6%97%A5.docx", "non-ASCII as UTF-8");
        assert_eq!(encode_url_path("a(1)[2]'x'!*@:;,$.xls"), "a%281%29%5B2%5D%27x%27%21%2A%40%3A%3B%2C%24.xls", "sub-delims and gen-delims");
    }

    #[test]
    fn git_entry_urls_are_encoded() {
        let text = sample().replace("data/x/two.png", "data/x/t w&o%.png");
        let m = Manifest::parse(&text).unwrap();
        assert_eq!(m.entries()[1].url, format!("https://raw.githubusercontent.com/owner/repo/{C}/data/x/t%20w%26o%25.png"));
        assert_eq!(m.entries()[1].path, "x/t w&o%.png");
    }

    #[test]
    fn safe_rel_rules() {
        for ok in ["a", "a/b.png", "a b/c"] {
            assert!(safe_rel(ok), "{ok}");
        }
        for bad in ["", "/a", "a//b", "a/../b", "./a", "a\\b", "C:/x", "a/"] {
            assert!(!safe_rel(bad), "{bad}");
        }
    }
}
