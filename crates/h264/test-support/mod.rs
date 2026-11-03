#![allow(dead_code, unused_imports)]

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Mutex, PoisonError, mpsc};
use std::time::Duration;

pub use craft_media_testkit::{ffmpeg, temp_path};

static PROGRESS: Mutex<String> = Mutex::new(String::new());

pub fn fixtures_dir(name: &str) -> PathBuf {
    craft_media_testkit::fixture_subdir(name)
}

pub fn ffmpeg_or_skip(context: &str) -> Option<PathBuf> {
    craft_media_testkit::ffmpeg().or_else(|| craft_media_testkit::skip(&format!("{context}: ffmpeg not found")))
}

pub fn ffmpeg_has_encoder(ffmpeg: &Path, encoder: &str) -> bool {
    let Ok(output) = Command::new(ffmpeg).args(["-hide_banner", "-encoders"]).stdin(Stdio::null()).output() else {
        return false;
    };
    String::from_utf8_lossy(&output.stdout).lines().any(|line| line.split_whitespace().nth(1) == Some(encoder))
}

pub fn skip(context: &str, reason: &str) {
    let _: Option<()> = craft_media_testkit::skip(&format!("{context}: {reason}"));
}

pub fn set_progress(label: &str) {
    *PROGRESS.lock().unwrap_or_else(PoisonError::into_inner) = label.to_owned();
}

pub fn with_deadline(seconds: u64, run: impl FnOnce() + Send + 'static) {
    let seconds =
        std::env::var("CRAFT_H264_HOSTILE_TIMEOUT_SECS").ok().and_then(|value| value.parse::<u64>().ok()).filter(|&value| value > 0).unwrap_or(seconds);
    let (sender, receiver) = mpsc::channel();
    let _ = std::thread::Builder::new().name("hostile deadline".into()).spawn(move || {
        let _ = sender.send(std::panic::catch_unwind(std::panic::AssertUnwindSafe(run)));
    });
    match receiver.recv_timeout(Duration::from_secs(seconds)) {
        Ok(Ok(())) => {}
        Ok(Err(payload)) => std::panic::resume_unwind(payload),
        Err(mpsc::RecvTimeoutError::Timeout) => {
            let last = PROGRESS.lock().unwrap_or_else(PoisonError::into_inner).clone();
            panic!("hostile-input test did not finish in {seconds}s (hang); last input: {last}");
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => panic!("hostile-input worker thread could not run"),
    }
}

pub mod fixtures {
    use std::path::{Path, PathBuf};

    pub use craft_media_testkit::generate_and_report;

    fn complete(path: &Path) -> bool {
        std::fs::metadata(path).is_ok_and(|meta| meta.is_file() && meta.len() > 0)
    }

    pub fn publish(temp: &Path, output: &Path) -> std::io::Result<()> {
        if !complete(temp) {
            let winner = complete(output);
            let _ = std::fs::remove_file(temp);
            return if winner { Ok(()) } else { Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "temporary fixture is not a non-empty file")) };
        }
        match std::fs::rename(temp, output) {
            Ok(()) => Ok(()),
            Err(error) => {
                let winner = complete(output);
                let _ = std::fs::remove_file(temp);
                if winner { Ok(()) } else { Err(error) }
            }
        }
    }

    pub fn generate(output: &Path, make: impl FnOnce(&Path) -> bool) -> Option<PathBuf> {
        if complete(output) {
            return Some(output.to_path_buf());
        }
        std::fs::create_dir_all(output.parent()?).ok()?;
        let temp = craft_media_testkit::temp_path(output);
        if !make(&temp) || !temp.exists() {
            let _ = std::fs::remove_file(&temp);
            return None;
        }
        publish(&temp, output).ok()?;
        Some(output.to_path_buf())
    }
}
