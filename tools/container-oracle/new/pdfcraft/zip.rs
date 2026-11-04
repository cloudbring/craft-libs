// Vendored verbatim from https://github.com/cloudbring/pdfcraft branch algo/120-use-craft-archive-zip at eefdea880bcbf2c6e762e6e1c850bd9a3b22becc
// path: crates/export/src/zip.rs
// Only change: the trailing '#[cfg(test)]' items are dropped (they are the app's own unit tests and need its dev-dependencies); nothing else is edited.

//! In-memory Office ZIP archives, backed by craft-archive.

use std::io::{self, Cursor};

use craft_archive::ZipWriter;

/// Builds a zip archive in memory.
pub struct Zip {
    writer: ZipWriter<Cursor<Vec<u8>>>,
    error: Option<io::Error>,
}

impl Default for Zip {
    fn default() -> Self {
        Self { writer: ZipWriter::in_memory(), error: None }
    }
}

impl Zip {
    /// Add a file. Already-compressed data (images) is stored; the rest is deflated.
    pub fn add(&mut self, name: &str, data: &[u8], compress: bool) {
        if self.error.is_some() {
            return;
        }
        // The old writer set UTF-8 for every name; craft-archive sets it only for non-ASCII names.
        let result = if compress { self.writer.add_deflated(name, data, 6) } else { self.writer.add_stored(name, data) };
        if let Err(error) = result {
            self.error = Some(error);
        }
    }

    /// Finish with a recoverable error if any entry could not be written.
    pub fn try_finish(self) -> io::Result<Vec<u8>> {
        if let Some(error) = self.error {
            return Err(error);
        }
        Ok(self.writer.finish()?.into_inner())
    }

    /// Finish an archive using the legacy infallible export API.
    pub fn finish(self) -> Vec<u8> {
        self.try_finish().unwrap_or_else(|error| {
            eprintln!("Office ZIP export failed: {error}");
            Vec::new()
        })
    }
}

