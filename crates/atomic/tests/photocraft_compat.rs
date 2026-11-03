//! Compatibility excerpts from `photocraft/crates/format/src/atomic.rs` and
//! `photocraft/crates/format/src/atomic/tests.rs`.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

const WRITE_CHUNK: usize = 8 << 20;

fn atomic_write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    craft_atomic::WriteOptions::default().with_create_parents(true).with_preserve_metadata(true).atomic_write(path, bytes)
}

fn atomic_write_progress(path: &Path, bytes: &[u8], progress: &dyn Fn(f32)) -> io::Result<()> {
    let mut previous = 0;
    let mut sink = |written: u64, _| {
        if written > previous {
            previous = written;
            progress(written as f32 / bytes.len() as f32);
        }
        Ok(())
    };
    craft_atomic::WriteOptions::default().with_create_parents(true).with_preserve_metadata(true).atomic_write_with_progress(
        path,
        Some(bytes.len() as u64),
        &mut sink,
        |writer| {
            for chunk in bytes.chunks(WRITE_CHUNK) {
                writer.write_all(chunk)?;
            }
            Ok(())
        },
    )
}

struct TempDir(PathBuf);
impl TempDir {
    fn new(tag: &str) -> Self {
        static N: AtomicU64 = AtomicU64::new(0);
        let d = std::env::temp_dir().join(format!("pc-atomic-{tag}-{}-{}", std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        Self(d)
    }
}
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn assert_no_temp(dir: &Path) {
    let leftovers: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".craft_tmp"))
        .collect();
    assert!(leftovers.is_empty(), "temporary files left behind: {leftovers:?}");
}

#[test]
fn replaces_and_creates() {
    let d = TempDir::new("ok");
    let p = d.0.join("doc.psd");
    atomic_write(&p, b"first").unwrap();
    assert_eq!(std::fs::read(&p).unwrap(), b"first");
    atomic_write(&p, b"second, longer").unwrap();
    assert_eq!(std::fs::read(&p).unwrap(), b"second, longer");
    assert_no_temp(&d.0);
}

#[test]
fn reports_write_progress() {
    let d = TempDir::new("progress");
    let p = d.0.join("big.psd");
    let bytes: Vec<u8> = (0..WRITE_CHUNK * 2 + 123).map(|i| (i % 251) as u8).collect();
    let seen = std::sync::Mutex::new(Vec::new());
    atomic_write_progress(&p, &bytes, &|f| seen.lock().unwrap().push(f)).unwrap();
    let seen = seen.into_inner().unwrap();
    assert_eq!(seen.len(), 3, "one report per chunk: {seen:?}");
    assert!(seen.windows(2).all(|w| w[0] < w[1]), "monotonic: {seen:?}");
    assert!(seen[0] > 0.0 && seen[0] < 0.6, "{seen:?}");
    assert_eq!(seen.last().copied(), Some(1.0));
    assert!(std::fs::read(&p).unwrap() == bytes, "the file holds the bytes");
    assert_no_temp(&d.0);
}

#[test]
fn creates_missing_parent_folders() {
    let d = TempDir::new("parent");
    let p = d.0.join("a/b/out.png");
    atomic_write(&p, b"x").unwrap();
    assert_eq!(std::fs::read(&p).unwrap(), b"x");
}
