//! ```text
//! LEAN_ZIP_INFLATE=<scratch>/lzexe/.lake/build/bin/lzinflate ARTCRAFT_CORPUS=/Users/emm/ArtCraft/corpus \
//!   CARGO_TARGET_DIR=<worktree>/target-o cargo test --release --manifest-path tools/inflate-oracle/Cargo.toml \
//!   -- --ignored --nocapture
//! ```
//!
//! `INFLATE_ORACLE_STRIDE=n` keeps every n-th corpus stream (default 1 = all). `INFLATE_ORACLE_OUT`
//! receives `summary.json` only: counts and the names of non-agreeing inputs, never stream bytes.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use craft_oracle::{BytesJudge, Differential, Outcome, Registry, Side};
use inflate_oracle::sides::{CraftArchive, Flate2Direct, LeanZip, MinizDirect};
use inflate_oracle::{corpus, hostile};

fn registry() -> Registry {
    Registry::from_path(&Path::new(env!("CARGO_MANIFEST_DIR")).join("expected.toml")).expect("expected.toml")
}

fn out_dir() -> PathBuf {
    PathBuf::from(std::env::var_os("INFLATE_ORACLE_OUT").unwrap_or_else(|| "target/inflate-oracle".into()))
}

fn lean() -> LeanZip {
    LeanZip::from_env().expect("set LEAN_ZIP_INFLATE to the lzinflate executable (see docs/verification/inflate-oracle.md)")
}

#[derive(Default)]
struct Tally {
    counts: BTreeMap<String, usize>,
    bad: Vec<(String, String)>,
    lean_faults: usize,
    both_fail_classes: BTreeMap<String, usize>,
    both_fail_names: Vec<(String, String, String)>,
}

fn clean(o: &Outcome) -> bool {
    matches!(o, Outcome::Identical | Outcome::BothFail { .. } | Outcome::ExpectedDifference { .. })
}

/// Run `old` vs `new` over `files` on `threads` workers; merge counts and keep every unclean result.
fn differential<O, N>(old: O, new: N, files: Vec<(String, Vec<u8>)>, threads: usize) -> Tally
where
    O: Side<Output = Vec<u8>> + Send + Sync + 'static,
    N: Side<Output = Vec<u8>> + Send + Sync + 'static,
{
    let (old, new) = (Arc::new(old), Arc::new(new));
    let chunk = files.len().div_ceil(threads).max(1);
    let reg = registry();
    let mut tally = Tally::default();
    std::thread::scope(|s| {
        let handles: Vec<_> = files
            .chunks(chunk)
            .map(|part| {
                let (old, new, reg) = (old.clone(), new.clone(), reg.clone());
                s.spawn(move || {
                    let d = Differential::new(Shared(old), Shared(new), BytesJudge).expected(reg).timeout(Duration::from_secs(120));
                    d.run_files(part.iter().map(|(n, b)| (n.clone(), b.as_slice())))
                })
            })
            .collect();
        for h in handles {
            let r = h.join().expect("worker");
            for res in r.results {
                *tally.counts.entry(res.outcome.name().to_string()).or_default() += 1;
                if let Outcome::BothFail { old_err, new_err } = &res.outcome {
                    if old_err.contains("LEAN-FAULT") {
                        tally.lean_faults += 1;
                    }
                    if !res.name.starts_with("hostile/") && !res.name.starts_with("mutation/") {
                        tally.both_fail_names.push((res.name.clone(), old_err.clone(), new_err.clone()));
                    }
                    let digitless: String = old_err.chars().filter(|c| !c.is_ascii_digit()).collect();
                    let class = if res.name.starts_with("mutation/") { "mutation".to_string() } else { res.name.clone() };
                    if res.name.starts_with("hostile/") || res.name.starts_with("mutation/") {
                        *tally.both_fail_classes.entry(format!("{class}: lean=`{digitless}` craft=`{new_err}`")).or_default() += 1;
                    }
                }
                if !clean(&res.outcome) {
                    tally.bad.push((res.name, format!("{:?}", res.outcome)));
                }
            }
        }
    });
    tally
}

struct Shared<S>(Arc<S>);
impl<S: Side> Side for Shared<S> {
    type Output = S::Output;
    fn run(&self, input: &[u8]) -> Result<S::Output, String> {
        self.0.run(input)
    }
}

fn hostile_inputs() -> Vec<(String, Vec<u8>)> {
    let mut v = hostile::cases();
    for (name, bytes) in hostile::mutations(3000, 0x00D3_F1A7) {
        // Pre-classify the one registered difference class so the registry glob is narrow.
        let name = if hostile::first_block_incomplete_litlen(&bytes) { name.replacen("mutation/", "mutation/incomplete-litlen/", 1) } else { name };
        v.push((name, bytes));
    }
    v
}

fn show(title: &str, t: &Tally) {
    println!("== {title}: {:?}; lean faults {}", t.counts, t.lean_faults);
    for (n, l, c) in t.both_fail_names.iter().take(40) {
        println!("   both-reject {n}: lean=`{l}` craft=`{c}`");
    }
    for (n, o) in t.bad.iter().take(40) {
        println!("   UNCLEAN {n}: {}", &o[..o.len().min(300)]);
    }
}

/// Rust-only sanity (no Lean needed): the three Rust decoders agree on every hostile stream.
#[test]
fn rust_sides_agree_on_hostile() {
    let t = differential(CraftArchive, Flate2Direct, hostile_inputs(), 4);
    show("craft-archive vs flate2", &t);
    assert!(t.bad.is_empty(), "{:?}", t.bad);
    let t = differential(CraftArchive, MinizDirect, hostile_inputs(), 4);
    show("craft-archive vs miniz_oxide", &t);
    assert!(t.bad.is_empty(), "{:?}", t.bad);
}

#[test]
#[ignore = "needs LEAN_ZIP_INFLATE"]
fn lean_vs_craft_archive_hostile() {
    let inputs = hostile_inputs();
    let n = inputs.len();
    let t = differential(lean(), CraftArchive, inputs, 8);
    show(&format!("hostile ({n} streams) lean vs craft-archive"), &t);
    for (k, v) in t.both_fail_classes.iter().filter(|(k, _)| !k.contains("truncated/short-")) {
        println!("   both reject [{v}x] {}", k.replace("hostile/truncated/frac-", "hostile/truncated/frac-NN"));
    }
    assert_eq!(t.lean_faults, 0);
    assert!(t.bad.is_empty(), "{:?}", t.bad);
}

#[test]
#[ignore = "needs LEAN_ZIP_INFLATE and ARTCRAFT_CORPUS"]
fn lean_vs_rust_sides_on_corpus() {
    let root = PathBuf::from(std::env::var_os("ARTCRAFT_CORPUS").expect("ARTCRAFT_CORPUS"));
    let t0 = Instant::now();
    let (all, tally) = corpus::extract(&root, &["office-poi", "office-openxml-sdk", "office-libreoffice"]);
    let stride: usize = std::env::var("INFLATE_ORACLE_STRIDE").ok().and_then(|s| s.parse().ok()).unwrap_or(1);
    let total_streams = all.len();
    let raw_bytes: usize = all.iter().map(|s| s.bytes.len()).sum();
    let picked: Vec<(String, Vec<u8>)> = all.into_iter().enumerate().filter(|(i, _)| i % stride == 0).map(|(_, s)| (s.name, s.bytes)).collect();
    println!(
        "extract: {tally:?}; deflate streams {total_streams} ({raw_bytes} compressed bytes) in {:?}; stride {stride} -> {} streams",
        t0.elapsed(),
        picked.len()
    );
    let n = picked.len();
    let compressed: usize = picked.iter().map(|(_, b)| b.len()).sum();

    let mut hostile_free = Vec::new();
    for (name, b) in &picked {
        hostile_free.push((name.clone(), b.clone()));
    }

    let t1 = Instant::now();
    let lean_vs_craft = differential(lean(), CraftArchive, picked, 8);
    let lean_secs = t1.elapsed();
    show("corpus lean vs craft-archive", &lean_vs_craft);

    let t2 = Instant::now();
    let craft_vs_flate2 = differential(CraftArchive, Flate2Direct, hostile_free.clone(), 8);
    show("corpus craft-archive vs flate2", &craft_vs_flate2);
    let craft_vs_miniz = differential(CraftArchive, MinizDirect, hostile_free, 8);
    show("corpus craft-archive vs miniz_oxide", &craft_vs_miniz);
    let rust_secs = t2.elapsed();

    let summary = serde_json::json!({
        "package_files_scanned": tally.files_seen, "packages": tally.packages, "unreadable_packages": tally.unreadable_packages,
        "deflate_entries": tally.deflate_entries, "stored_entries": tally.stored_entries, "other_method_entries": tally.other_method_entries,
        "stride": stride, "streams_compared": n, "compressed_bytes_compared": compressed,
        "lean_vs_craft_archive": { "counts": lean_vs_craft.counts, "wall_secs": lean_secs.as_secs_f64(), "unclean": lean_vs_craft.bad },
        "craft_archive_vs_flate2": { "counts": craft_vs_flate2.counts, "unclean": craft_vs_flate2.bad },
        "craft_archive_vs_miniz_oxide": { "counts": craft_vs_miniz.counts, "unclean": craft_vs_miniz.bad },
        "rust_sides_wall_secs": rust_secs.as_secs_f64(),
    });
    std::fs::create_dir_all(out_dir()).expect("out dir");
    std::fs::write(out_dir().join("summary.json"), serde_json::to_string_pretty(&summary).expect("json")).expect("write");
    assert_eq!(lean_vs_craft.lean_faults, 0);
    assert!(lean_vs_craft.bad.is_empty() && craft_vs_flate2.bad.is_empty() && craft_vs_miniz.bad.is_empty(), "see {}", out_dir().display());
}

/// Lean decoder throughput: the largest streams, one at a time, sequential, median of 3.
#[test]
#[ignore = "needs LEAN_ZIP_INFLATE and ARTCRAFT_CORPUS"]
fn lean_throughput() {
    let root = PathBuf::from(std::env::var_os("ARTCRAFT_CORPUS").expect("ARTCRAFT_CORPUS"));
    let (mut all, _) = corpus::extract(&root, &["office-poi", "office-openxml-sdk", "office-libreoffice"]);
    all.sort_by_key(|s| std::cmp::Reverse(s.bytes.len()));
    let lean = lean();
    let tiny = hostile::cases().into_iter().find(|(n, _)| n == "hostile/valid/empty-fixed").expect("case").1;
    let time = |side: &dyn Fn(&[u8]) -> usize, input: &[u8]| {
        let mut t: Vec<f64> = (0..3)
            .map(|_| {
                let s = Instant::now();
                let n = side(input);
                std::hint::black_box(n);
                s.elapsed().as_secs_f64()
            })
            .collect();
        t.sort_by(|a, b| a.partial_cmp(b).expect("finite"));
        t[1]
    };
    let lean_f = |b: &[u8]| lean.run(b).map(|o| o.len()).unwrap_or(0);
    let craft_f = |b: &[u8]| CraftArchive.run(b).map(|o| o.len()).unwrap_or(0);
    let startup = time(&lean_f, &tiny);
    println!("lean process startup (empty stream): {:.1} ms", startup * 1e3);
    let (mut out_total, mut lean_total, mut craft_total, mut in_total) = (0usize, 0.0, 0.0, 0usize);
    for s in all.iter().take(25) {
        let out = CraftArchive.run(&s.bytes).expect("corpus stream decodes").len();
        let (tl, tc) = (time(&lean_f, &s.bytes), time(&craft_f, &s.bytes));
        println!(
            "{:>10} B in -> {:>10} B out: lean {:.1} ms (net {:.1} MB/s), craft-archive {:.1} ms ({:.1} MB/s)  {}",
            s.bytes.len(),
            out,
            tl * 1e3,
            out as f64 / 1e6 / (tl - startup).max(1e-6),
            tc * 1e3,
            out as f64 / 1e6 / tc.max(1e-6),
            s.name
        );
        out_total += out;
        in_total += s.bytes.len();
        lean_total += tl - startup;
        craft_total += tc;
    }
    println!(
        "25 largest: {in_total} B in, {out_total} B out; lean net {:.1} MB/s out, craft-archive {:.1} MB/s out",
        out_total as f64 / 1e6 / lean_total,
        out_total as f64 / 1e6 / craft_total
    );
}
