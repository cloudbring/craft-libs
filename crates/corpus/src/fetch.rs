//! Fetching, the stamp, and verification of a corpus tree.

use std::collections::BTreeSet;
use std::io::Read;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::manifest::{Entry, Manifest, STAMP_FILE};

/// Per-file download cap (bytes).
pub const MAX_FILE_BYTES: u64 = 512 * 1024 * 1024;
/// Redirect hops followed by [`UreqFetcher`].
pub const MAX_REDIRECTS: usize = 5;
/// Generic User-Agent: no personal details in requests.
pub const USER_AGENT: &str = "craft-corpus";

/// Where bytes come from. [`UreqFetcher`] is the network implementation; tests inject their own so
/// no unit test needs a network.
pub trait Fetcher {
    /// GET `url`, at most `max_bytes`. `url` has already been checked to be `https://`.
    ///
    /// # Errors
    /// [`Error::Fetch`] or [`Error::Insecure`].
    fn get(&self, url: &str, max_bytes: u64) -> Result<Vec<u8>>;
}

/// Blocking HTTPS GET through ureq + rustls. Redirects are followed by hand so each hop can be
/// checked: a redirect to anything but `https://` is refused.
pub struct UreqFetcher {
    agent: ureq::Agent,
}

impl Default for UreqFetcher {
    fn default() -> Self {
        Self::new()
    }
}

impl UreqFetcher {
    /// A fetcher with connect/response/overall timeouts and automatic redirects disabled.
    #[must_use]
    pub fn new() -> Self {
        let config = ureq::Agent::config_builder()
            .https_only(true)
            .user_agent(USER_AGENT)
            .timeout_connect(Some(std::time::Duration::from_secs(20)))
            .timeout_global(Some(std::time::Duration::from_secs(600)))
            .max_redirects(0)
            .http_status_as_error(false)
            .build();
        Self { agent: ureq::Agent::new_with_config(config) }
    }
}

impl Fetcher for UreqFetcher {
    fn get(&self, url: &str, max_bytes: u64) -> Result<Vec<u8>> {
        let fail = |url: &str, reason: String| Error::Fetch { url: url.to_string(), reason };
        let mut current = url.to_string();
        for _ in 0..=MAX_REDIRECTS {
            require_https(&current)?;
            let mut resp = self.agent.get(&current).call().map_err(|e| fail(&current, e.to_string()))?;
            let status = resp.status().as_u16();
            if (300..400).contains(&status) {
                let loc = resp
                    .headers()
                    .get("location")
                    .and_then(|v| v.to_str().ok())
                    .ok_or_else(|| fail(&current, format!("HTTP {status} without Location")))?;
                current = resolve_redirect(&current, loc)?;
                continue;
            }
            if status != 200 {
                return Err(fail(&current, format!("HTTP {status}")));
            }
            let mut buf = Vec::new();
            let mut reader = resp.body_mut().with_config().limit(max_bytes.saturating_add(1)).reader();
            reader.read_to_end(&mut buf).map_err(|e| fail(&current, format!("reading body: {e}")))?;
            if buf.len() as u64 > max_bytes {
                return Err(fail(&current, format!("larger than {max_bytes} bytes")));
            }
            return Ok(buf);
        }
        Err(fail(url, format!("more than {MAX_REDIRECTS} redirects")))
    }
}

fn require_https(url: &str) -> Result<()> {
    if url.get(..8).is_some_and(|p| p.eq_ignore_ascii_case("https://")) { Ok(()) } else { Err(Error::Insecure(url.to_string())) }
}

/// Resolves a `Location` header against the URL that returned it. Only results on `https://` are
/// accepted; a downgrade to `http://` (or any other scheme) is [`Error::Insecure`].
///
/// # Errors
/// [`Error::Insecure`] for a non-HTTPS target, [`Error::Fetch`] for a relative form not handled.
pub fn resolve_redirect(base: &str, location: &str) -> Result<String> {
    let target = if location.contains("://") {
        location.to_string()
    } else if let Some(rest) = location.strip_prefix("//") {
        format!("https://{rest}")
    } else if location.starts_with('/') {
        let after = base.split_once("://").map_or("", |(_, r)| r);
        let host = after.split('/').next().unwrap_or("");
        format!("https://{host}{location}")
    } else {
        return Err(Error::Fetch { url: base.to_string(), reason: format!("unsupported relative redirect {location:?}") });
    };
    require_https(&target)?;
    Ok(target)
}

/// One verified file as recorded in the stamp.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StampFile {
    /// Corpus-relative path.
    pub path: String,
    /// Lowercase hex SHA-256.
    pub sha256: String,
    /// Size in bytes.
    pub bytes: u64,
    /// Feature tags.
    pub tags: Vec<String>,
    /// Redistribution permitted.
    pub redistributable: bool,
    /// Licence.
    pub licence: String,
}

/// `.craft-corpus.json`: written only after every file verified.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stamp {
    /// Stamp format version (1).
    pub schema: u32,
    /// Corpus name.
    pub name: String,
    /// [`Manifest::cache_key`] at fetch time.
    pub cache_key: String,
    /// Every file, sorted by path.
    pub files: Vec<StampFile>,
}

/// What a fetch did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchReport {
    /// Files downloaded.
    pub downloaded: usize,
    /// Files already present with the right hash.
    pub reused: usize,
    /// Files not in the manifest that were removed.
    pub pruned: usize,
}

/// Fetches `manifest` into `root` over HTTPS.
///
/// # Errors
/// Any [`Error`]; on failure no stamp exists, so [`verify`] refuses the tree.
pub fn fetch(manifest: &Manifest, root: &Path) -> Result<FetchReport> {
    fetch_with(manifest, root, &UreqFetcher::new())
}

/// [`fetch`] with an injected [`Fetcher`].
///
/// The steps: drop the old stamp, then for each file reuse it when it already hashes right, else
/// download, check HTTPS and sha256, and write atomically (temp file in the same directory, fsync,
/// rename). Files not in the manifest are removed. The stamp is written last.
///
/// # Errors
/// Any [`Error`].
pub fn fetch_with(manifest: &Manifest, root: &Path, fetcher: &dyn Fetcher) -> Result<FetchReport> {
    manifest.validate()?;
    std::fs::create_dir_all(root).map_err(|e| Error::io(format!("create {}", root.display()), e))?;
    let stamp_path = root.join(STAMP_FILE);
    match std::fs::remove_file(&stamp_path) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(Error::io(format!("remove {}", stamp_path.display()), e)),
    }
    let entries = manifest.entries();
    let (mut downloaded, mut reused) = (0, 0);
    let mut files = Vec::with_capacity(entries.len());
    for e in &entries {
        let dest = root.join(&e.path);
        let existing = std::fs::read(&dest).ok().filter(|b| craft_hash::sha256_hex(b) == e.sha256);
        let bytes = if let Some(b) = existing {
            reused += 1;
            b
        } else {
            require_https(&e.url)?;
            let b = fetcher.get(&e.url, MAX_FILE_BYTES)?;
            let actual = craft_hash::sha256_hex(&b);
            if actual != e.sha256 {
                return Err(Error::HashMismatch { path: e.path.clone(), expected: e.sha256.clone(), actual });
            }
            atomic_write(&dest, &b)?;
            downloaded += 1;
            b
        };
        files.push(stamp_file(e, bytes.len() as u64));
    }
    let wanted: BTreeSet<&str> = entries.iter().map(|e| e.path.as_str()).collect();
    let pruned = prune(root, &wanted)?;
    files.sort_by(|a, b| a.path.cmp(&b.path));
    let stamp = Stamp { schema: 1, name: manifest.name.clone(), cache_key: manifest.cache_key(), files };
    let json = serde_json::to_vec_pretty(&stamp).map_err(|e| Error::Parse(e.to_string()))?;
    atomic_write(&stamp_path, &json)?;
    Ok(FetchReport { downloaded, reused, pruned })
}

fn stamp_file(e: &Entry, bytes: u64) -> StampFile {
    StampFile {
        path: e.path.clone(),
        sha256: e.sha256.clone(),
        bytes,
        tags: e.tags.clone(),
        redistributable: e.redistributable,
        licence: e.licence.clone(),
    }
}

/// Writes `bytes` to `path` so a reader never sees a partial file: temp file beside it, fsync,
/// rename over the target.
fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(dir).map_err(|e| Error::io(format!("create {}", dir.display()), e))?;
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let tmp = dir.join(format!(".{name}.{}.tmp", std::process::id()));
    let write = || -> std::io::Result<()> {
        use std::io::Write;
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        std::fs::rename(&tmp, path)
    };
    write().map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        Error::io(format!("write {}", path.display()), e)
    })
}

/// Every file path under `root` (relative, `/`-separated); `Err` on a symlink or unreadable entry.
fn walk(root: &Path) -> std::io::Result<Vec<String>> {
    let mut out = Vec::new();
    let mut stack: Vec<PathBuf> = vec![PathBuf::new()];
    while let Some(rel) = stack.pop() {
        for entry in std::fs::read_dir(root.join(&rel))? {
            let entry = entry?;
            let ft = entry.file_type()?;
            let child = rel.join(entry.file_name());
            if ft.is_symlink() {
                return Err(std::io::Error::other(format!("symlink {}", child.display())));
            }
            if ft.is_dir() {
                stack.push(child);
            } else {
                let parts: Vec<String> = child.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect();
                out.push(parts.join("/"));
            }
        }
    }
    out.sort();
    Ok(out)
}

fn prune(root: &Path, wanted: &BTreeSet<&str>) -> Result<usize> {
    let mut n = 0;
    for rel in walk(root).map_err(|e| Error::io(format!("scan {}", root.display()), e))? {
        if rel != STAMP_FILE && !wanted.contains(rel.as_str()) {
            std::fs::remove_file(root.join(&rel)).map_err(|e| Error::io(format!("remove {rel}"), e))?;
            n += 1;
        }
    }
    Ok(n)
}

/// A verified corpus tree.
#[derive(Debug, Clone)]
pub struct Corpus {
    /// Root directory.
    pub root: PathBuf,
    /// The stamp that was checked.
    pub stamp: Stamp,
}

/// A file of a verified [`Corpus`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CorpusFile {
    /// Absolute (root-joined) path.
    pub path: PathBuf,
    /// Corpus-relative path.
    pub rel: String,
    /// Tags.
    pub tags: Vec<String>,
    /// Redistribution permitted.
    pub redistributable: bool,
}

impl Corpus {
    /// Every file, sorted by relative path.
    #[must_use]
    pub fn files(&self) -> Vec<CorpusFile> {
        self.stamp.files.iter().map(|f| self.file(f)).collect()
    }

    /// Files carrying `tag`, sorted.
    #[must_use]
    pub fn files_by_tag(&self, tag: &str) -> Vec<CorpusFile> {
        self.stamp.files.iter().filter(|f| f.tags.iter().any(|t| t == tag)).map(|f| self.file(f)).collect()
    }

    /// Files that may be redistributed.
    #[must_use]
    pub fn redistributable(&self) -> Vec<CorpusFile> {
        self.stamp.files.iter().filter(|f| f.redistributable).map(|f| self.file(f)).collect()
    }

    /// Number of files, for floors.
    #[must_use]
    pub fn count(&self) -> u64 {
        self.stamp.files.len() as u64
    }

    fn file(&self, f: &StampFile) -> CorpusFile {
        CorpusFile { path: self.root.join(&f.path), rel: f.path.clone(), tags: f.tags.clone(), redistributable: f.redistributable }
    }
}

/// Verifies the tree at `root` against its own stamp: the stamp must exist, every listed file must
/// exist with the stamped sha256 and size, and no other file or symlink may be present.
///
/// The stamp is self-asserted, so this proves the tree is the one a fetch produced and has not
/// been edited since. Use [`verify_manifest`] to also prove it is the one the manifest pins.
///
/// # Errors
/// [`Error::Unverified`] (no or unreadable stamp) or [`Error::Tampered`].
pub fn verify(root: &Path) -> Result<Corpus> {
    let unverified = |reason: String| Error::Unverified { root: root.to_path_buf(), reason };
    let tampered = |reason: String| Error::Tampered { root: root.to_path_buf(), reason };
    let raw = std::fs::read(root.join(STAMP_FILE)).map_err(|e| unverified(format!("no readable {STAMP_FILE}: {e}")))?;
    let stamp: Stamp = serde_json::from_slice(&raw).map_err(|e| unverified(format!("bad {STAMP_FILE}: {e}")))?;
    if stamp.schema != 1 {
        return Err(unverified(format!("unknown stamp schema {}", stamp.schema)));
    }
    let mut listed = BTreeSet::new();
    for f in &stamp.files {
        if !crate::manifest::safe_rel(&f.path) || f.path == STAMP_FILE || !listed.insert(f.path.as_str()) {
            return Err(tampered(format!("stamp lists unsafe or duplicate path {:?}", f.path)));
        }
        let bytes = std::fs::read(root.join(&f.path)).map_err(|e| tampered(format!("{}: {e}", f.path)))?;
        if bytes.len() as u64 != f.bytes || craft_hash::sha256_hex(&bytes) != f.sha256 {
            return Err(tampered(format!("{} does not match the stamped sha256", f.path)));
        }
    }
    for rel in walk(root).map_err(|e| tampered(format!("scan: {e}")))? {
        if rel != STAMP_FILE && !listed.contains(rel.as_str()) {
            return Err(tampered(format!("unlisted file {rel}")));
        }
    }
    Ok(Corpus { root: root.to_path_buf(), stamp })
}

/// [`verify`], and the stamp must also match `manifest` (same name, same [`Manifest::cache_key`]).
///
/// # Errors
/// [`verify`]'s errors, or [`Error::Stale`] when a pin moved since the fetch.
pub fn verify_manifest(root: &Path, manifest: &Manifest) -> Result<Corpus> {
    let corpus = verify(root)?;
    let expected = manifest.cache_key();
    if corpus.stamp.cache_key != expected || corpus.stamp.name != manifest.name {
        return Err(Error::Stale { root: root.to_path_buf(), expected, actual: corpus.stamp.cache_key });
    }
    Ok(corpus)
}
