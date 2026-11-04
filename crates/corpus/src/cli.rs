//! The `craft-corpus` command line.
//!
//! ```text
//! craft-corpus fetch <manifest.toml> <root>
//! craft-corpus verify <root> [--manifest <manifest.toml>]
//! craft-corpus cache-key <manifest.toml>
//! craft-corpus sha256 <file>...
//! craft-corpus floors check  <floors.toml> (--counts <counts.json> | --count metric=N ...)
//! craft-corpus floors raise  <floors.toml> (--counts <counts.json> | --count metric=N ...)
//! craft-corpus floors ratchet <floors.toml> --base <base-floors.toml>
//! ```
//! Exit code 0 on success, 1 on a failed check or error, 2 on a usage error.

use std::io::Write;
use std::path::Path;

use crate::error::Error;
use crate::floors::{Counts, Floors};
use crate::{Manifest, fetch, sha256_file, verify, verify_manifest};

const USAGE: &str = "usage: craft-corpus fetch <manifest> <root> | verify <root> [--manifest <m>] | cache-key <manifest> | sha256 <file>... | floors check|raise <floors> (--counts <json> | --count k=N ...) | floors ratchet <floors> --base <floors>";

/// Runs a command, writing its output to `out`; returns the process exit code.
pub fn run(args: &[String], out: &mut dyn Write) -> i32 {
    let strs: Vec<&str> = args.iter().map(String::as_str).collect();
    match dispatch(&strs, out) {
        Ok(()) => 0,
        Err(CliError::Usage) => {
            let _ = writeln!(out, "{USAGE}");
            2
        }
        Err(CliError::Failed(e)) => {
            let _ = writeln!(out, "error: {e}");
            1
        }
    }
}

enum CliError {
    Usage,
    Failed(String),
}

impl From<Error> for CliError {
    fn from(e: Error) -> Self {
        Self::Failed(e.to_string())
    }
}

fn dispatch(args: &[&str], out: &mut dyn Write) -> std::result::Result<(), CliError> {
    match args {
        ["fetch", manifest, root] => {
            let m = Manifest::load(Path::new(manifest))?;
            let r = fetch(&m, Path::new(root))?;
            say(out, format!("{}: {} downloaded, {} reused, {} pruned; verified and stamped", m.name, r.downloaded, r.reused, r.pruned));
            Ok(())
        }
        ["verify", root] => verify_cmd(root, None, out),
        ["verify", root, "--manifest", m] => verify_cmd(root, Some(m), out),
        ["cache-key", manifest] => {
            say(out, Manifest::load(Path::new(manifest))?.cache_key());
            Ok(())
        }
        ["sha256", files @ ..] if !files.is_empty() => {
            for f in files {
                say(out, format!("{}  {f}", sha256_file(Path::new(f))?));
            }
            Ok(())
        }
        ["floors", "check", file, rest @ ..] => floors_check(file, rest, out),
        ["floors", "raise", file, rest @ ..] => floors_raise(file, rest, out),
        ["floors", "ratchet", file, "--base", base] => {
            let (now, was) = (Floors::load(Path::new(file))?, Floors::load(Path::new(base))?);
            ratchet(&now, &was, out)
        }
        _ => Err(CliError::Usage),
    }
}

fn say(out: &mut dyn Write, line: String) {
    let _ = writeln!(out, "{line}");
}

fn verify_cmd(root: &str, manifest: Option<&&str>, out: &mut dyn Write) -> std::result::Result<(), CliError> {
    let c = match manifest {
        Some(m) => verify_manifest(Path::new(root), &Manifest::load(Path::new(m))?)?,
        None => verify(Path::new(root))?,
    };
    say(out, format!("{}: {} files verified", c.stamp.name, c.count()));
    Ok(())
}

fn floors_check(file: &str, rest: &[&str], out: &mut dyn Write) -> std::result::Result<(), CliError> {
    let floors = Floors::load(Path::new(file))?;
    let counts = parse_counts(rest)?;
    match floors.check(&counts) {
        Ok(()) => {
            say(out, format!("floors ok ({} metrics)", floors.floors.len()));
            Ok(())
        }
        Err(v) => Err(CliError::Failed(v.iter().map(ToString::to_string).collect::<Vec<_>>().join("\n"))),
    }
}

fn floors_raise(file: &str, rest: &[&str], out: &mut dyn Write) -> std::result::Result<(), CliError> {
    let path = Path::new(file);
    let mut floors = if path.exists() { Floors::load(path)? } else { Floors::default() };
    let changes = floors.raise(&parse_counts(rest)?);
    floors.save(path)?;
    for c in &changes {
        let from = c.from.map_or_else(|| "new".to_string(), |f| f.to_string());
        say(out, format!("{}: {from} -> {}", c.metric, c.to));
    }
    say(out, format!("{} floors raised", changes.len()));
    Ok(())
}

fn ratchet(now: &Floors, base: &Floors, out: &mut dyn Write) -> std::result::Result<(), CliError> {
    match now.check_not_lowered(base) {
        Ok(()) => {
            say(out, "no floor lowered".to_string());
            Ok(())
        }
        Err(v) => Err(CliError::Failed(
            v.iter()
                .map(|x| match x.actual {
                    Some(a) => format!("{}: floor lowered from {} to {a}", x.metric, x.floor),
                    None => format!("{}: floor {} was removed", x.metric, x.floor),
                })
                .collect::<Vec<_>>()
                .join("\n"),
        )),
    }
}

fn parse_counts(args: &[&str]) -> std::result::Result<Counts, CliError> {
    let mut counts = Counts::new();
    let mut it = args.iter();
    while let Some(&flag) = it.next() {
        let value = it.next().ok_or(CliError::Usage)?;
        match flag {
            "--counts" => {
                let text = std::fs::read_to_string(value).map_err(|e| CliError::Failed(format!("read {value}: {e}")))?;
                let parsed: Counts = serde_json::from_str(&text).map_err(|e| CliError::Failed(format!("{value}: {e}")))?;
                counts.extend(parsed);
            }
            "--count" => {
                let (k, v) = value.split_once('=').ok_or(CliError::Usage)?;
                let n: u64 = v.parse().map_err(|_| CliError::Failed(format!("--count {value}: not a number")))?;
                counts.insert(k.to_string(), n);
            }
            _ => return Err(CliError::Usage),
        }
    }
    Ok(counts)
}

#[cfg(test)]
mod tests {
    use super::run;

    fn go(args: &[&str]) -> (i32, String) {
        let mut out = Vec::new();
        let code = run(&args.iter().map(|s| (*s).to_string()).collect::<Vec<_>>(), &mut out);
        (code, String::from_utf8(out).unwrap())
    }

    #[test]
    fn floors_check_raise_ratchet_round() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("corpus-floors.toml");
        let f = f.to_str().unwrap();
        let (code, text) = go(&["floors", "raise", f, "--count", "a.files=5", "--count", "b.files=2"]);
        assert_eq!(code, 0, "{text}");
        assert_eq!(go(&["floors", "check", f, "--count", "a.files=5", "--count", "b.files=3"]).0, 0);
        let (code, text) = go(&["floors", "check", f, "--count", "a.files=4", "--count", "b.files=3"]);
        assert_eq!((code, text.contains("a.files: 4 is below the floor 5")), (1, true), "{text}");
        let base = dir.path().join("base.toml");
        std::fs::copy(f, &base).unwrap();
        assert_eq!(go(&["floors", "raise", f, "--count", "a.files=1", "--count", "b.files=9"]).0, 0);
        assert_eq!(go(&["floors", "ratchet", f, "--base", base.to_str().unwrap()]).0, 0);
        assert_eq!(go(&["floors", "ratchet", base.to_str().unwrap(), "--base", f]).0, 1, "lowering b.files must fail");
    }

    #[test]
    fn usage_errors_exit_2() {
        assert_eq!(go(&[]).0, 2);
        assert_eq!(go(&["fetch"]).0, 2);
    }

    #[test]
    fn sha256_command_prints_digest() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("abc");
        std::fs::write(&p, b"abc").unwrap();
        let (code, text) = go(&["sha256", p.to_str().unwrap()]);
        assert_eq!(code, 0);
        assert!(text.starts_with("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"), "{text}");
    }
}
