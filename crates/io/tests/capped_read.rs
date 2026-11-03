use craft_io::{CappedReadError, CappedReader, read_capped, read_capped_from, read_capped_to_string};
use proptest::prelude::*;
use std::io::{self, Read};
use std::path::Path;

#[test]
fn reads_within_limit_and_rejects_oversize() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("f.bin");
    std::fs::write(&path, vec![7u8; 100])?;
    assert_eq!(read_capped(&path, 100)?.len(), 100);
    assert_eq!(read_capped(&path, 200)?.len(), 100);
    assert!(matches!(read_capped(&path, 99), Err(CappedReadError::TooLarge { limit: 99 })));
    assert_eq!(read_capped(&path, usize::MAX)?.len(), 100);
    assert_eq!(read_capped_from(std::fs::File::open(&path)?, 100)?.len(), 100);
    Ok(())
}

#[test]
fn exact_limit_and_one_extra_byte() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("bytes");
    std::fs::write(&path, b"abc")?;
    assert_eq!(read_capped(&path, 3)?, b"abc");
    assert!(matches!(read_capped(&path, 2), Err(CappedReadError::TooLarge { limit: 2 })));
    assert_eq!(CappedReader::new(io::Cursor::new(b""), 0).into_bytes()?, b"");
    assert!(matches!(CappedReader::new(io::Cursor::new(b"x"), 0).into_bytes(), Err(CappedReadError::TooLarge { limit: 0 })));
    Ok(())
}

#[test]
fn rejects_directory_and_reports_io_errors() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    assert!(matches!(read_capped(dir.path(), 10), Err(CappedReadError::NotRegular)));
    #[cfg(unix)]
    assert!(matches!(read_capped_from(std::fs::File::open(dir.path())?, 10), Err(CappedReadError::NotRegular)));
    let missing = read_capped(&dir.path().join("missing"), 10);
    assert!(matches!(missing, Err(CappedReadError::Io(error)) if error.kind() == io::ErrorKind::NotFound));
    Ok(())
}

#[test]
fn reads_utf8_and_reports_invalid_utf8() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("text");
    std::fs::write(&path, "hé")?;
    assert_eq!(read_capped_to_string(&path, 3)?, "hé");
    assert!(matches!(read_capped_to_string(&path, 2), Err(CappedReadError::TooLarge { limit: 2 })));
    std::fs::write(&path, [0xff])?;
    assert!(matches!(read_capped_to_string(&path, 10), Err(CappedReadError::Io(error)) if error.kind() == io::ErrorKind::InvalidData));
    Ok(())
}

#[test]
fn adapter_implements_read_without_truncating() -> Result<(), Box<dyn std::error::Error>> {
    let mut reader = CappedReader::new(io::Cursor::new(b"abcd"), 3);
    let mut buffer = [0; 2];
    assert_eq!(reader.read(&mut buffer)?, 2);
    assert_eq!(&buffer, b"ab");
    assert_eq!(reader.read(&mut buffer)?, 1);
    assert_eq!(buffer[0], b'c');
    assert!(reader.read(&mut buffer).is_err());
    assert!(reader.read(&mut buffer).is_err());
    Ok(())
}

#[cfg(unix)]
#[test]
fn rejects_non_regular_device() {
    if Path::new("/dev/zero").exists() {
        assert!(matches!(read_capped(Path::new("/dev/zero"), 8 << 20), Err(CappedReadError::NotRegular)));
    }
}

#[cfg(unix)]
#[test]
fn accepts_symlink_to_regular_file() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let target = dir.path().join("file");
    let link = dir.path().join("link");
    std::fs::write(&target, b"abc")?;
    std::os::unix::fs::symlink(&target, &link)?;
    assert_eq!(read_capped(&link, 3)?, b"abc");
    Ok(())
}

#[cfg(unix)]
#[test]
fn fifo_without_writer_finishes_before_timeout() -> Result<(), Box<dyn std::error::Error>> {
    use std::sync::mpsc;
    use std::time::Duration;

    let dir = tempfile::tempdir()?;
    let fifo = dir.path().join("pipe");
    assert!(std::process::Command::new("mkfifo").arg(&fifo).status()?.success());
    let (sender, receiver) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        let _ = sender.send(read_capped(&fifo, 1024));
    });
    let outcome = receiver.recv_timeout(Duration::from_secs(2))?;
    assert!(matches!(outcome, Err(CappedReadError::NotRegular)));
    worker.join().map_err(|_| io::Error::other("FIFO worker panicked"))?;
    Ok(())
}

proptest! {
    #[test]
    fn random_file_sizes_obey_limit(bytes in proptest::collection::vec(any::<u8>(), 0..1024), limit in 0usize..1025) {
        let dir = tempfile::tempdir().map_err(|error| TestCaseError::fail(error.to_string()))?;
        let path = dir.path().join("random");
        std::fs::write(&path, &bytes).map_err(|error| TestCaseError::fail(error.to_string()))?;
        let outcome = read_capped(&path, limit);
        if bytes.len() <= limit {
            prop_assert_eq!(outcome.map_err(|error| TestCaseError::fail(error.to_string()))?, bytes);
        } else {
            let is_too_large = matches!(outcome, Err(CappedReadError::TooLarge { limit: actual }) if actual == limit);
            prop_assert!(is_too_large);
        }
    }
}

// Reference: /Users/emm/ArtCraft/cadcraft/crates/engine/src/capped_read.rs (read_capped).
#[cfg(unix)]
mod cadcraft {
    use std::fs::OpenOptions;
    use std::io::{self, Read};
    use std::path::Path;

    /// Read at most `max` bytes from the regular file at `path`.
    ///
    /// Returns an error if the path is not a regular file, cannot be opened, or holds more than `max`
    /// bytes. At most `max + 1` bytes are ever buffered, so an endless device cannot exhaust memory.
    pub fn read_capped(path: &Path, max: usize) -> io::Result<Vec<u8>> {
        let mut options = OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(libc::O_NONBLOCK);
        }
        let file = options.open(path)?;
        let meta = file.metadata()?;
        if !meta.is_file() {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "not a regular file"));
        }
        // Read one byte past the limit so we can tell "exactly max" from "too big" without trusting
        // the metadata length (which a device may misreport).
        let mut buf = Vec::new();
        let cap = (max as u64).saturating_add(1);
        let read = file.take(cap).read_to_end(&mut buf)?;
        if read as u64 > max as u64 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, format!("file is larger than the {max}-byte limit")));
        }
        Ok(buf)
    }
}

#[cfg(unix)]
fn normalized(outcome: Result<Vec<u8>, CappedReadError>) -> Result<Vec<u8>, io::ErrorKind> {
    outcome.map_err(|error| match error {
        CappedReadError::TooLarge { .. } => io::ErrorKind::InvalidData,
        CappedReadError::NotRegular => io::ErrorKind::InvalidInput,
        CappedReadError::Io(error) => error.kind(),
    })
}

#[cfg(unix)]
#[test]
fn matches_cadcraft_on_same_fixtures() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("file");
    let link = dir.path().join("link");
    let fifo = dir.path().join("pipe");
    std::fs::write(&path, b"abcd")?;
    std::os::unix::fs::symlink(&path, &link)?;
    assert!(std::process::Command::new("mkfifo").arg(&fifo).status()?.success());
    for (fixture, limits) in [
        (path.as_path(), &[0, 3, 4, 5, usize::MAX][..]),
        (link.as_path(), &[4][..]),
        (fifo.as_path(), &[4][..]),
        (dir.path(), &[4][..]),
        (Path::new("/dev/zero"), &[4][..]),
        (Path::new("/missing-craft-io-differential-fixture"), &[4][..]),
    ] {
        for &limit in limits {
            assert_eq!(normalized(read_capped(fixture, limit)), cadcraft::read_capped(fixture, limit).map_err(|error| error.kind()));
        }
    }
    Ok(())
}
