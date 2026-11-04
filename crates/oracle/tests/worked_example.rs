//! Worked example: compare two revisions of a tiny function over a generated corpus.
//!
//! In a real consumer `rev_old` is the crate at the commit before the swap, linked as
//! `old_impl = { package = "<crate>", git = "...", rev = "..." }`; here two modules stand in.
//! The function maps grayscale bytes to half brightness and returns a 16x16 image.

use craft_compare::{Image, ImageTolerance};
use craft_oracle::{Differential, FnSide, ImageJudge, Outcome, Policy, Registry, SideId};

const SIDE: usize = 16;

fn to_image(v: &[u8]) -> Result<Image, String> {
    let plane = v.iter().map(|&x| f32::from(x) / 255.0).collect();
    Image::from_plane(SIDE, SIDE, plane).map_err(|e| format!("{e:?}"))
}

mod rev_old {
    pub fn halve(data: &[u8]) -> Result<Vec<u8>, String> {
        if data.len() != 256 {
            return Err(format!("expected 256 bytes, got {}", data.len()));
        }
        Ok(data.iter().map(|&x| x / 2).collect())
    }
}

mod rev_new {
    pub fn halve(data: &[u8]) -> Result<Vec<u8>, String> {
        if data.len() != 256 {
            return Err(format!("expected 256 bytes, got {}", data.len()));
        }
        Ok(data
            .iter()
            .map(|&x| match x {
                128..=131 => x / 2 + 1, // rounding change: one level, inside tolerance
                255 => 100,             // intended fix: saturated pixels no longer map to 127
                7 => 200,               // BUG introduced by the swap
                _ => x / 2,
            })
            .collect())
    }
}

/// Deterministic pseudo-random 256-byte image avoiding the special values, with `special` planted.
fn make(seed: u32, special: Option<u8>) -> Vec<u8> {
    let mut s = seed.wrapping_mul(2_654_435_761).wrapping_add(1);
    let mut v: Vec<u8> = (0..256)
        .map(|_| {
            s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let b = (s >> 24) as u8;
            if matches!(b, 7 | 128..=131 | 255) { 42 } else { b }
        })
        .collect();
    if let Some(x) = special {
        v[17] = x;
    }
    v
}

fn corpus() -> Vec<(String, Vec<u8>)> {
    let mut files = Vec::new();
    for i in 0..8 {
        files.push((format!("plain/{i}.raw"), make(i, None)));
    }
    for i in 0..4 {
        files.push((format!("round/{i}.raw"), make(100 + i, Some(128 + i as u8 % 4))));
    }
    for i in 0..3 {
        files.push((format!("sat/{i}.raw"), make(200 + i, Some(255))));
    }
    files.push(("bug/0.raw".into(), make(300, Some(7))));
    files.push(("bad/short.raw".into(), vec![1, 2, 3]));
    files
}

const EXPECTED: &str = r#"
[[expected]]
id = "sat-clamp"
reason = "saturated pixels were halved to 127; the fix maps them to 100"
evidence = "cloudbring/artcraft-algorithms#178 (worked example)"
glob = "sat/**"
metric = "max_abs"
min_value = 0.05
max_value = 0.2

[[expected]]
id = "never-used"
reason = "left over from an earlier change"
evidence = "cloudbring/artcraft-algorithms#178 (worked example)"
glob = "legacy/**"
"#;

fn differential(
    artifacts: &std::path::Path,
) -> Differential<
    impl craft_oracle::Side<Output = Image> + Send + Sync + 'static,
    impl craft_oracle::Side<Output = Image> + Send + Sync + 'static,
    ImageJudge,
> {
    let tol = ImageTolerance { max_abs: Some(0.01), ..Default::default() };
    Differential::new(
        FnSide(|b: &[u8]| rev_old::halve(b).and_then(|v| to_image(&v))),
        FnSide(|b: &[u8]| rev_new::halve(b).and_then(|v| to_image(&v))),
        ImageJudge::new(tol),
    )
    .expected(Registry::from_toml_str(EXPECTED).unwrap())
    .artifact_dir(artifacts)
}

#[test]
fn compares_two_revisions_over_generated_corpus() {
    let dir = tempfile::tempdir().unwrap();
    let report = differential(dir.path()).run_files(corpus());

    assert_eq!(report.total, 17);
    assert_eq!(report.count("Identical"), 8);
    assert_eq!(report.count("WithinTolerance"), 4);
    assert_eq!(report.count("ExpectedDifference"), 3);
    assert_eq!(report.count("Regression"), 1);
    assert_eq!(report.count("BothFail"), 1);
    assert_eq!(report.stale, vec!["never-used".to_string()]);

    let bug = report.results.iter().find(|r| r.name == "bug/0.raw").unwrap();
    assert!(matches!(&bug.outcome, Outcome::Regression { metric, .. } if metric == "max_abs"));

    // The regression fails the run; the expected differences and the clean BothFail do not.
    let policy = Policy::default();
    assert_eq!(report.exit_code(&policy), 1);
    assert_eq!(report.failures(&policy).len(), 1);
    assert_eq!(report.exit_code(&Policy { fail_on_both_fail: true, ..policy }), 1);

    // The failing input was saved with its outcome.
    let saved: Vec<String> = std::fs::read_dir(dir.path()).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    assert!(saved.iter().any(|n| n.ends_with("bug_0.raw.bin")), "{saved:?}");
    assert!(saved.iter().any(|n| n.ends_with("bug_0.raw.json")), "{saved:?}");
    assert_eq!(saved.len(), 2);

    // Reports serialise and summarise.
    let json: serde_json::Value = serde_json::from_str(&report.to_json()).unwrap();
    assert_eq!(json["counts"]["Regression"], 1);
    let md = report.to_markdown(&policy);
    assert!(md.contains("| Regression | 1 |") && md.contains("bug/0.raw") && md.contains("never-used"), "{md}");
    report.write(&dir.path().join("out"), &policy).unwrap();
    assert!(dir.path().join("out/report.json").exists() && dir.path().join("out/report.md").exists());
}

#[test]
fn without_the_regression_the_run_passes() {
    let dir = tempfile::tempdir().unwrap();
    let files: Vec<_> = corpus().into_iter().filter(|(n, _)| !n.starts_with("bug/")).collect();
    let report = differential(dir.path()).run_files(files);
    assert_eq!(report.exit_code(&Policy::default()), 0);
    // `fail_on_stale` turns the leftover registry entry into a failure.
    assert_eq!(report.exit_code(&Policy { fail_on_stale: true, ..Policy::default() }), 1);
}

#[test]
fn timeout_of_new_side_fails_the_run() {
    let d = Differential::new(
        FnSide(|b: &[u8]| Ok(b.to_vec())),
        FnSide(|b: &[u8]| {
            std::thread::sleep(std::time::Duration::from_millis(500));
            Ok(b.to_vec())
        }),
        craft_oracle::BytesJudge,
    )
    .timeout(std::time::Duration::from_millis(50));
    let r = d.run_files([("x", [1u8])]);
    assert_eq!(r.results[0].outcome, Outcome::Timeout { side: SideId::New });
    assert_eq!(r.exit_code(&Policy::default()), 1);
}
