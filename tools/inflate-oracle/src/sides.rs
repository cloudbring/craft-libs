use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use craft_archive::{Limits, Wrapper, inflate_bounded};
use craft_oracle::Side;

pub const MAX_OUT: u64 = 1 << 30;

/// The verified pure-Lean decoder as a subprocess. Exit 0 = accepted (stdout is the output),
/// exit 2 = rejected, anything else (crash, signal, timeout) is an error carrying a `LEAN-FAULT`
/// marker so the report cannot mistake it for a clean rejection.
pub struct LeanZip {
    pub exe: PathBuf,
    pub timeout: Duration,
}

impl LeanZip {
    pub fn from_env() -> Option<Self> {
        let exe = PathBuf::from(std::env::var_os("LEAN_ZIP_INFLATE")?);
        Some(Self { exe, timeout: Duration::from_secs(60) })
    }
}

impl Side for LeanZip {
    type Output = Vec<u8>;
    fn run(&self, input: &[u8]) -> Result<Vec<u8>, String> {
        let mut child = Command::new(&self.exe)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("LEAN-FAULT spawn: {e}"))?;
        let mut stdin = child.stdin.take().unwrap();
        let data = input.to_vec();
        let writer = std::thread::spawn(move || {
            let _ = stdin.write_all(&data);
        });
        let mut stdout = child.stdout.take().unwrap();
        let mut stderr = child.stderr.take().unwrap();
        let out = std::thread::spawn(move || {
            let mut v = Vec::new();
            let _ = stdout.read_to_end(&mut v);
            v
        });
        let err = std::thread::spawn(move || {
            let mut v = Vec::new();
            let _ = stderr.read_to_end(&mut v);
            v
        });
        let start = Instant::now();
        let status = loop {
            match child.try_wait() {
                Ok(Some(s)) => break s,
                Ok(None) if start.elapsed() > self.timeout => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err("LEAN-FAULT timeout".into());
                }
                Ok(None) => std::thread::sleep(Duration::from_micros(200)),
                Err(e) => return Err(format!("LEAN-FAULT wait: {e}")),
            }
        };
        let _ = writer.join();
        let out = out.join().unwrap_or_default();
        let err = String::from_utf8_lossy(&err.join().unwrap_or_default()).trim().to_string();
        match status.code() {
            Some(0) => Ok(out),
            Some(2) => Err(err),
            other => Err(format!("LEAN-FAULT exit {other:?}: {err}")),
        }
    }
}

/// craft-archive's bounded inflate on a raw DEFLATE stream (the path the zip reader uses).
pub struct CraftArchive;

impl Side for CraftArchive {
    type Output = Vec<u8>;
    fn run(&self, input: &[u8]) -> Result<Vec<u8>, String> {
        inflate_bounded(input, Limits { max_output: MAX_OUT, max_input: u64::MAX, max_ratio: u64::MAX, wrapper: Wrapper::Raw })
            .map_err(|e| e.to_string())
    }
}

/// flate2's `DeflateDecoder` used directly (miniz_oxide backend).
pub struct Flate2Direct;

impl Side for Flate2Direct {
    type Output = Vec<u8>;
    fn run(&self, input: &[u8]) -> Result<Vec<u8>, String> {
        let mut out = Vec::new();
        flate2::read::DeflateDecoder::new(input).take(MAX_OUT).read_to_end(&mut out).map_err(|e| e.to_string())?;
        Ok(out)
    }
}

/// miniz_oxide's one-shot API.
pub struct MinizDirect;

impl Side for MinizDirect {
    type Output = Vec<u8>;
    fn run(&self, input: &[u8]) -> Result<Vec<u8>, String> {
        miniz_oxide::inflate::decompress_to_vec_with_limit(input, MAX_OUT as usize).map_err(|e| format!("{:?}", e.status))
    }
}
