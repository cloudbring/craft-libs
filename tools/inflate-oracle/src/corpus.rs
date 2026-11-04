//! Raw DEFLATE payloads of every deflate-compressed zip entry under a corpus directory.

use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};

pub struct Stream {
    /// `<corpus-relative zip path>!<entry name>`
    pub name: String,
    pub bytes: Vec<u8>,
}

#[derive(Default, Debug)]
pub struct Tally {
    pub files_seen: usize,
    pub packages: usize,
    pub unreadable_packages: usize,
    pub deflate_entries: usize,
    pub stored_entries: usize,
    pub other_method_entries: usize,
}

pub fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    let mut items: Vec<_> = rd.flatten().map(|e| e.path()).collect();
    items.sort();
    for p in items {
        if p.is_dir() {
            walk(&p, out);
        } else {
            out.push(p);
        }
    }
}

/// Every file whose first four bytes are a zip local-header signature is a package.
pub fn extract(root: &Path, dirs: &[&str]) -> (Vec<Stream>, Tally) {
    let mut files = Vec::new();
    for d in dirs {
        walk(&root.join(d), &mut files);
    }
    let mut tally = Tally::default();
    let mut streams = Vec::new();
    for path in files {
        tally.files_seen += 1;
        let Ok(bytes) = std::fs::read(&path) else { continue };
        if !bytes.starts_with(b"PK\x03\x04") {
            continue;
        }
        tally.packages += 1;
        let rel = path.strip_prefix(root).unwrap_or(&path).display().to_string();
        let Ok(mut zip) = zip::ZipArchive::new(Cursor::new(&bytes)) else {
            tally.unreadable_packages += 1;
            continue;
        };
        for i in 0..zip.len() {
            let Ok(mut entry) = zip.by_index_raw(i) else { continue };
            match entry.compression() {
                zip::CompressionMethod::Deflated => {
                    let mut raw = Vec::new();
                    if entry.read_to_end(&mut raw).is_ok() {
                        tally.deflate_entries += 1;
                        streams.push(Stream {
                            name: format!("{rel}!{}", entry.name().map(|n| n.into_owned()).unwrap_or_else(|_| format!("#{i}"))),
                            bytes: raw,
                        });
                    }
                }
                zip::CompressionMethod::Stored => tally.stored_entries += 1,
                _ => tally.other_method_entries += 1,
            }
        }
    }
    (streams, tally)
}
