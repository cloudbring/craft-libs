//! The one error type.

use std::path::PathBuf;

/// Everything that can go wrong in this crate.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The manifest text is not valid TOML for the schema.
    #[error("manifest: {0}")]
    Parse(String),
    /// The manifest parsed but breaks a rule (bad hash, unsafe path, missing licence, ...).
    #[error("manifest invalid: {0}")]
    Invalid(String),
    /// A URL is not `https://`, or a redirect left HTTPS.
    #[error("not https: {0}")]
    Insecure(String),
    /// The network request failed.
    #[error("fetch {url}: {reason}")]
    Fetch {
        /// The URL requested.
        url: String,
        /// What went wrong.
        reason: String,
    },
    /// Downloaded bytes do not match the pinned sha256. Nothing was written.
    #[error("sha256 mismatch for {path}: pinned {expected}, got {actual}")]
    HashMismatch {
        /// Corpus-relative path.
        path: String,
        /// Pinned digest.
        expected: String,
        /// Digest of the bytes received.
        actual: String,
    },
    /// The tree has no stamp, or the stamp is unreadable: it was never verified.
    #[error("unverified corpus at {}: {reason}", .root.display())]
    Unverified {
        /// Corpus root.
        root: PathBuf,
        /// Why.
        reason: String,
    },
    /// A file differs from the stamp, is missing, or an unlisted file is present.
    #[error("tampered corpus at {}: {reason}", .root.display())]
    Tampered {
        /// Corpus root.
        root: PathBuf,
        /// Which file and how.
        reason: String,
    },
    /// The tree verifies against its own stamp but not against the manifest (a pin moved).
    #[error("stale corpus at {}: manifest cache key {expected}, stamp {actual}; run fetch", .root.display())]
    Stale {
        /// Corpus root.
        root: PathBuf,
        /// Cache key of the manifest.
        expected: String,
        /// Cache key recorded in the stamp.
        actual: String,
    },
    /// The corpus is absent and the environment says it is required.
    #[error("corpus `{name}` is required but absent at {}: {hint}", .root.display())]
    Missing {
        /// Corpus name.
        name: String,
        /// Where it was looked for.
        root: PathBuf,
        /// Which variable made it required.
        hint: String,
    },
    /// A floor was broken.
    #[error("floors: {0}")]
    Floors(String),
    /// Filesystem error.
    #[error("{context}: {source}")]
    Io {
        /// What was being done.
        context: String,
        /// The underlying error.
        #[source]
        source: std::io::Error,
    },
}

impl Error {
    pub(crate) fn io(context: impl Into<String>, source: std::io::Error) -> Self {
        Self::Io { context: context.into(), source }
    }
}

/// `Result` with this crate's [`Error`].
pub type Result<T> = std::result::Result<T, Error>;
