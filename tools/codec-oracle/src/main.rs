//! `codec-oracle gen`  : generate the ffmpeg fixture matrix and download the conformance vectors.
//! `codec-oracle run`  : run every old-vs-new scenario and write the report.
//!
//! Environment: CODEC_ORACLE_FIXTURES (fixture root), CODEC_ORACLE_OUT (report directory),
//! CODEC_ORACLE_ONLY (comma list of substrings of `<scenario>/<set>`), CODEC_ORACLE_TIMEOUT (s).

use codec_oracle::inputs::{self, Named};
use codec_oracle::judge;
use codec_oracle::matrix;
use codec_oracle::report::{Acc, Row, markdown};
use codec_oracle::trace::{End, Trace};
use craft_oracle::{Differential, FnJudge, FnSide, Outcome, Policy, Registry};
use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use codec_oracle::sides::Drv;

struct Scenario {
    name: String,
    app: &'static str,
    codec: &'static str,
    old: Drv,
    old_st: Drv,
    new: Drv,
    new_st: Drv,
    pre: Option<Drv>,
    pre_st: Option<Drv>,
    /// Which input family: the codec's ffmpeg dir plus the conformance/vector sets.
    driver: &'static str,
}

fn scenarios() -> Vec<Scenario> {
    use codec_oracle::sides::lookup;
    let mut v = Vec::new();
    // (app, driver): `driver` is also the name of the input family.
    let list: &[(&str, &str, &str)] = &[
        ("h264", "filmcraft", "h264"),
        ("h264", "deckcraft", "h264"),
        ("h264", "soundcraft", "h264"),
        ("h264", "filmcraft", "h264_avcc"),
        ("h264", "deckcraft", "h264_avcc"),
        ("h264", "soundcraft", "h264_avcc"),
        ("hevc", "filmcraft", "hevc"),
        ("hevc", "deckcraft", "hevc"),
        ("hevc", "filmcraft", "hevc_hvcc"),
        ("hevc", "deckcraft", "hevc_hvcc"),
        ("h264", "deckcraft", "h264_budget"),
        ("hevc", "deckcraft", "hevc_budget"),
        ("av1", "deckcraft", "av1_budget"),
        ("vp9", "deckcraft", "vp9_budget"),
        ("av1", "filmcraft", "av1"),
        ("av1", "deckcraft", "av1"),
        ("vp8", "filmcraft", "vp8"),
        ("vp9", "filmcraft", "vp9"),
        ("vp9", "deckcraft", "vp9"),
        ("opus", "filmcraft", "opus_ogg"),
        ("opus", "deckcraft", "opus_ogg"),
        ("opus", "filmcraft", "opus_bit_stereo"),
        ("opus", "filmcraft", "opus_bit_mono"),
        ("opus", "filmcraft", "opus_bit_stereo_noinv"),
        ("opus", "deckcraft", "opus_bit_stereo"),
        ("opus", "deckcraft", "opus_bit_mono"),
        ("opus", "deckcraft", "opus_bit_stereo_noinv"),
    ];
    for &(codec, app, driver) in list {
        let old = |st| match app {
            "filmcraft" => lookup::fc(driver, st),
            "deckcraft" => lookup::dc(driver, st),
            _ => lookup::sc(driver, st),
        };
        let (Some(old), Some(old_st)) = (old(false), old(true)) else { panic!("no {app} driver {driver}") };
        let get = |f: fn(&str, bool) -> Option<Drv>, st| f(driver, st).expect("driver");
        v.push(Scenario {
            name: format!("{}-{}", driver.replace('_', "-"), app),
            app: leak(app),
            codec: leak(codec),
            old,
            old_st,
            new: get(lookup::new, false),
            new_st: get(lookup::new, true),
            pre: lookup::pre(driver, false),
            pre_st: lookup::pre(driver, true),
            driver: leak(driver.replace('_', "-").as_str()),
        });
    }
    v
}

fn leak(s: &str) -> &'static str {
    Box::leak(s.to_string().into_boxed_str())
}

/// The input sets of a scenario: (set name, inputs).
fn sets(sc: &Scenario, root: &Path) -> Vec<(&'static str, Box<dyn Iterator<Item = Named>>)> {
    let mut v: Vec<(&'static str, Box<dyn Iterator<Item = Named>>)> = Vec::new();
    match sc.driver {
        "h264-budget" | "hevc-budget" | "av1-budget" | "vp9-budget" => {
            v.push(("budget", Box::new(budget_inputs(sc, root).into_iter())));
        }
        "hevc-hvcc" => {
            v.push(("hvcc", Box::new(codec_oracle::synth_hevc::hvcc_corpus().into_iter())));
        }
        "h264-avcc" => {
            v.push(("avcc", Box::new(codec_oracle::synth_h264::avcc_corpus().into_iter().map(|(n, b)| (n, b)))));
        }
        "h264" | "hevc" | "av1" | "vp8" | "vp9" => {
            let dir = root.join(sc.codec);
            let ext = matrix::ext(sc.codec);
            v.push(("matrix", Box::new(inputs::read_all(inputs::list(&dir, ext)))));
            match sc.codec {
                "vp8" => v.push(("vectors", Box::new(inputs::read_all(inputs::list(&root.join("vp8/conformance"), "ivf"))))),
                "vp9" => v.push(("vectors", Box::new(inputs::read_all(inputs::list(&root.join("vp9/conformance"), "ivf"))))),
                "av1" => v.push(("vectors", Box::new(inputs::read_all(inputs::list(&root.join("av1/aom-test-data"), "ivf"))))),
                _ => {}
            }
            let bases = inputs::mutant_bases(&dir, ext, 8, 400_000);
            v.push(("mutants", Box::new(inputs::mutant_stream(bases))));
            // av1 decodes are slow (large allocations on hostile dimensions): a smaller sweep
            let (nb, width, offs) = if sc.codec == "av1" { (4, 10, 5) } else { (6, 12, 14) };
            let small = inputs::mutant_bases(&dir, ext, nb, 40_000);
            v.push(("sweep", Box::new(inputs::sweep_stream(sc.codec, small.clone(), width, offs))));
            if sc.codec == "h264" {
                v.push(("synth", Box::new(codec_oracle::synth_h264::corpus().into_iter())));
            }
            if sc.codec == "hevc" {
                v.push(("synth", Box::new(codec_oracle::synth_hevc::corpus().into_iter())));
            }
            let (hn, hs) = if sc.codec == "av1" { (4, 10) } else { (14, 12) };
            v.push(("havoc", Box::new(inputs::havoc_stream(sc.codec, small, hn, hs))));
        }
        "opus-ogg" => {
            let dir = root.join("opus");
            v.push(("matrix", Box::new(inputs::read_all(inputs::list(&dir, "opus")))));
            let bases = inputs::mutant_bases(&dir, "opus", 8, 400_000);
            v.push(("mutants", Box::new(inputs::mutant_stream(bases))));
        }
        _ => {
            // opus-bit-*: the twelve official vectors, plus damaged copies of two of them
            let dir = root.join("opus/vectors");
            v.push(("vectors", Box::new(inputs::read_all(inputs::list(&dir, "bit")))));
            let mut bases = inputs::list(&dir, "bit");
            bases.truncate(0);
            let pick: Vec<PathBuf> =
                ["testvector01.bit", "testvector05.bit", "testvector11.bit"].iter().map(|n| dir.join(n)).filter(|p| p.is_file()).collect();
            v.push(("mutants", Box::new(inputs::mutant_stream(pick).take(150))));
        }
    }
    v
}

fn count_end(t: &Trace, ok: &mut usize, err: &mut usize, panic: &mut usize) {
    match t.end {
        End::Complete => *ok += 1,
        End::Error { .. } => *err += 1,
        End::Panic { .. } => *panic += 1,
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let root = matrix::fixture_root();
    match args.first().map(String::as_str) {
        Some("gen") => generate(&root, args.get(1).map(String::as_str)),
        Some("run") => run(&root),
        Some("opus-quality") => opus_quality(&root),
        _ => eprintln!("usage: codec-oracle gen [codec] | run"),
    }
}

fn generate(root: &Path, only: Option<&str>) {
    let (ok, skipped) = matrix::generate(root, only);
    println!("generated or cached: {}; skipped: {} {:?}", ok.len(), skipped.len(), skipped);
}

fn run(root: &Path) {
    let out = PathBuf::from(std::env::var_os("CODEC_ORACLE_OUT").unwrap_or_else(|| "target/codec-oracle".into()));
    std::fs::create_dir_all(&out).expect("out dir");
    let reg_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("expected.toml");
    let registry = Registry::from_path(&reg_path).expect("expected.toml");
    let only: Vec<String> = std::env::var("CODEC_ORACLE_ONLY").unwrap_or_default().split(',').filter(|s| !s.is_empty()).map(String::from).collect();
    let timeout = Duration::from_secs(std::env::var("CODEC_ORACLE_TIMEOUT").ok().and_then(|s| s.parse().ok()).unwrap_or(120));
    let policy = Policy::default();
    let mut md = String::new();
    let mut details = String::new();
    let mut rows: Vec<Row> = Vec::new();
    let mut stale_all: Option<Vec<String>> = None;
    let mut failures = 0usize;
    for sc in scenarios() {
        for (set, files) in sets(&sc, root) {
            let tag = format!("{}/{}", sc.name, set);
            if !only.is_empty() && !only.iter().any(|o| tag.contains(o)) {
                continue;
            }
            let acc = Arc::new(Mutex::new(Acc::default()));
            let det: Arc<Mutex<HashMap<u64, String>>> = Arc::default();
            let names: Arc<Mutex<HashMap<u64, String>>> = Arc::default();
            let (acc2, det2) = (acc.clone(), det.clone());
            let j = FnJudge(move |o: &Trace, n: &Trace| {
                let mut guard = acc2.lock().unwrap();
                let a = &mut *guard;
                a.streams += 1;
                a.units += n.units;
                a.frames_old += o.frames.len();
                a.frames_new += n.frames.len();
                a.pcm_samples += n.pcm.len();
                count_end(o, &mut a.old_ok, &mut a.old_err, &mut a.old_panic);
                count_end(n, &mut a.new_ok, &mut a.new_err, &mut a.new_panic);
                let d = judge::diff(o, n);
                if !d.aspects.is_empty() {
                    det2.lock().unwrap().insert(n.input, d.detail);
                }
                judge::judge(o, n)
            });
            let (old, new, pre) = if set != "matrix" && set != "vectors" { (sc.old_st, sc.new_st, sc.pre_st) } else { (sc.old, sc.new, sc.pre) };
            let wrap_old = move |b: &[u8]| {
                old(b).map(|mut t| {
                    t.input = inputs::fnv(b);
                    t
                })
            };
            let wrap_new = move |b: &[u8]| {
                let mut t = new(b)?;
                t.input = inputs::fnv(b);
                t.pre = pre.and_then(|f| f(b).ok()).map(Arc::new);
                Ok(t)
            };
            let diff = Differential::new(FnSide(wrap_old), FnSide(wrap_new), j).timeout(timeout).expected(registry.clone()).policy(policy);
            let names2 = names.clone();
            let prefix = tag.clone();
            let files = files.map(move |(n, b)| {
                names2.lock().unwrap().insert(inputs::fnv(&b), n.clone());
                (format!("{prefix}/{n}"), b)
            });
            let t0 = std::time::Instant::now();
            let report = diff.run_files(files);
            let acc = acc.lock().unwrap().clone();
            let counts: BTreeMap<String, usize> = report.counts.iter().filter(|(_, v)| **v > 0).map(|(k, v)| (k.clone(), *v)).collect();
            eprintln!("{tag}: {} streams, {} frames, {:?} ({:.0}s)", report.total, acc.frames_new, counts, t0.elapsed().as_secs_f64());
            let n_fail = report.failures(&policy).len();
            failures += n_fail;
            let det = det.lock().unwrap();
            let nm = names.lock().unwrap();
            for r in &report.results {
                if r.outcome == Outcome::Identical {
                    continue;
                }
                // details are keyed by input hash; recover it from the name
                let key = nm.iter().find(|(_, n)| r.name.ends_with(&format!("/{}", n))).map(|(h, _)| *h);
                let d = key.and_then(|k| det.get(&k)).cloned().unwrap_or_default();
                details.push_str(&format!("{}\t{:?}\t{}\n", r.name, r.outcome, d));
            }
            stale_all = Some(match stale_all {
                None => report.stale.clone(),
                Some(s) => s.into_iter().filter(|id| report.stale.contains(id)).collect(),
            });
            rows.push(Row {
                scenario: sc.name.clone(),
                app: sc.app.into(),
                codec: sc.codec.into(),
                set: set.into(),
                total: report.total,
                counts,
                acc,
            });
        }
    }
    md.push_str(&markdown(&rows));
    if let Some(s) = &stale_all {
        md.push_str(&format!("\nStale expected entries (matched nothing in any run): {s:?}\n"));
    }
    std::fs::write(out.join("report.md"), &md).expect("write report.md");
    std::fs::write(out.join("details.tsv"), &details).expect("write details");
    println!("{md}");
    println!("failures (unexpected differences, panics, timeouts of the new side): {failures}");
    std::process::exit(i32::from(failures > 0));
}

/// RFC 8251 vectors: opus_compare-style quality of the old and new decoders' PCM against the
/// reference `.dec` files (the crate's own pass criterion is quality >= 0), plus the largest sample
/// difference between old and new.
fn opus_quality(root: &Path) {
    use codec_oracle::opus_quality::{opus_quality, snr_db};
    use codec_oracle::sides::lookup;
    let out = PathBuf::from(std::env::var_os("CODEC_ORACLE_OUT").unwrap_or_else(|| "target/codec-oracle".into()));
    std::fs::create_dir_all(&out).expect("out dir");
    let dir = root.join("opus/vectors");
    let pcm = |p: PathBuf| -> Vec<f32> {
        std::fs::read(p).unwrap_or_default().chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]]) as f32 / 32768.0).collect()
    };
    let mut md = String::from(
        "| vector | config | quality fc | quality dc | quality new | SNR new vs ref (dB) | max abs fc-new | max abs dc-new | range states equal (fc/dc vs new) |\n|---|---|---:|---:|---:|---:|---:|---:|---|\n",
    );
    let mut worst = f64::MAX;
    let mut n = 0;
    for v in 1..=12 {
        let name = format!("testvector{v:02}");
        let Ok(bit) = std::fs::read(dir.join(format!("{name}.bit"))) else { continue };
        for (cfg, driver, channels, suffix) in
            [("stereo", "opus_bit_stereo", 2usize, ""), ("stereo-noinv", "opus_bit_stereo_noinv", 2, "m"), ("mono", "opus_bit_mono", 1, "m")]
        {
            let mut reference = pcm(dir.join(format!("{name}{suffix}.dec")));
            if channels == 1 {
                reference = reference.chunks_exact(2).map(|c| 0.5 * (c[0] + c[1])).collect();
            }
            let run = |f: Option<codec_oracle::sides::Drv>| f.map(|f| f(&bit).expect("driver"));
            let (fc, dc, new) = (run(lookup::fc(driver, false)), run(lookup::dc(driver, false)), run(lookup::new(driver, false)));
            let (fc, dc, new) = (fc.unwrap(), dc.unwrap(), new.unwrap());
            let q = |t: &Trace| opus_quality(&reference, &t.pcm, channels, 48000);
            let maxd = |a: &Trace, b: &Trace| a.pcm.iter().zip(&b.pcm).map(|(x, y)| f64::from((x - y).abs())).fold(0f64, f64::max);
            let qn = q(&new);
            worst = worst.min(qn);
            n += 1;
            let _ = writeln!(
                md,
                "| {name} | {cfg} | {:.1} | {:.1} | {:.1} | {:.1} | {:.2e} | {:.2e} | {}/{} |",
                q(&fc),
                q(&dc),
                qn,
                snr_db(&reference, &new.pcm),
                maxd(&fc, &new),
                maxd(&dc, &new),
                fc.ranges == new.ranges,
                dc.ranges == new.ranges
            );
        }
    }
    let _ = writeln!(md, "\n{n} comparisons; lowest new-decoder quality {worst:.1} (pass criterion: >= 0).");
    std::fs::write(out.join("opus-quality.md"), &md).expect("write");
    println!("{md}");
}

/// `[u64 BE budget][stream]` inputs: for two small streams of the codec, the budget at which the
/// shared crate starts accepting (binary search) and a dense neighbourhood of it, plus extremes.
fn budget_inputs(sc: &Scenario, root: &Path) -> Vec<Named> {
    use codec_oracle::sides::lookup;
    let driver = sc.driver.replace('-', "_");
    let new = lookup::new(&driver, false).expect("new budget driver");
    let ext = matrix::ext(sc.codec);
    let dir = root.join(sc.codec);
    let mut files = inputs::list(&dir, ext);
    files.retain(|p| std::fs::metadata(p).is_ok_and(|m| m.len() < 60_000));
    let pick: Vec<PathBuf> = [0usize, files.len() / 3, 2 * files.len() / 3].iter().filter_map(|&i| files.get(i).cloned()).collect();
    let mut out = Vec::new();
    for p in pick {
        let Ok(data) = std::fs::read(&p) else { continue };
        let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let with = |b: u64| {
            let mut v = b.to_be_bytes().to_vec();
            v.extend_from_slice(&data);
            v
        };
        let accepts = |b: u64| new(&with(b)).map(|t| t.end == End::Complete).unwrap_or(false);
        let (mut lo, mut hi) = (0u64, 1u64 << 36);
        if !accepts(hi) {
            continue;
        }
        while lo + 1 < hi {
            let mid = lo + (hi - lo) / 2;
            if accepts(mid) { hi = mid } else { lo = mid }
        }
        let t = hi;
        let mut budgets = vec![0u64, 1, t / 2, t.saturating_mul(2), t.saturating_mul(1000), u64::MAX];
        for d in [1u64, 2, 4, 16, 64, 256, 1024, 4096, 65536] {
            budgets.push(t.saturating_sub(d));
            budgets.push(t.saturating_add(d));
        }
        budgets.push(t);
        budgets.sort_unstable();
        budgets.dedup();
        for b in budgets {
            out.push((format!("{name}#budget{b}"), with(b)));
        }
    }
    out
}
