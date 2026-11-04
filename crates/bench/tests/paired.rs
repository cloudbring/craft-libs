//! End-to-end: a worked example (two implementations of a small function), the equality gate,
//! the contended marker with injected load, JSON round trip and the compare gate.

use std::hint::black_box;
use std::sync::Arc;

use craft_bench::compare::{Basis, Clock, compare};
use craft_bench::probe::Throttle;
use craft_bench::report::{ReportError, Verdict};
use craft_bench::{BenchError, Config, Paired, Report, SCHEMA_VERSION, SystemProbe};

struct Fake(Option<f64>);
impl SystemProbe for Fake {
    fn load1(&self) -> Option<f64> {
        self.0
    }
    fn logical_cores(&self) -> usize {
        4
    }
    fn throttle(&self) -> Throttle {
        Throttle::default()
    }
}

fn cfg(load: Option<f64>) -> Config {
    Config { warmup: 1, probe: Arc::new(Fake(load)), ..Config::default() }
}

/// Sum of squares below n: a loop (old) against the closed form (new).
fn sum_sq_loop(n: &u64) -> u64 {
    (0..black_box(*n)).fold(0u64, |a, i| a.wrapping_add(black_box(i).wrapping_mul(i)))
}
fn sum_sq_closed(n: &u64) -> u64 {
    if *n == 0 { 0 } else { (n - 1) * n * (2 * n - 1) / 6 }
}

fn worked_example(cfg: &Config) -> Report {
    Paired::new("sum of squares")
        .old(sum_sq_loop)
        .new(sum_sq_closed)
        .inputs([("n=1e4", 10_000u64), ("n=1e5", 100_000), ("n=1e6", 1_000_000)])
        .equal()
        .run(cfg)
        .unwrap()
}

#[test]
fn worked_example_loop_vs_closed_form() {
    let r = worked_example(&cfg(Some(0.1)));
    assert_eq!(r.schema_version, SCHEMA_VERSION);
    assert!(r.publishable, "{:?}", r.publish_problems());
    assert_eq!(r.inputs.len(), 3);
    // 10 timed runs per side per input, alternating, every run recorded.
    assert_eq!(r.runs.len(), 3 * 2 * 10);
    for i in &r.inputs {
        assert_eq!(i.wall.old.n, 10);
        assert_eq!(i.wall.new.n, 10);
        assert!(i.cpu.is_some());
    }
    // O(n) against O(1): new is faster on the largest input with the CI entirely below 1.
    let big = &r.inputs[2].wall.ratio;
    assert_eq!(big.verdict(), Verdict::Faster, "{big:?}");
    assert!(r.aggregate.wall.ci_hi < 1.0);
    // Runs alternate: no three in a row of one side.
    assert!(r.runs.windows(3).all(|w| !(w[0].side == w[1].side && w[1].side == w[2].side)));
    assert!(r.to_markdown().contains("| n=1e5 |"));
}

#[test]
fn same_seed_gives_the_same_schedule_and_bootstrap_inputs() {
    let a = worked_example(&cfg(Some(0.1)));
    let b = worked_example(&cfg(Some(0.1)));
    let order = |r: &Report| r.runs.iter().map(|x| (x.input.clone(), x.side)).collect::<Vec<_>>();
    assert_eq!(order(&a), order(&b));
}

#[test]
fn mismatched_outputs_fail_before_timing() {
    let err = Paired::new("wrong").old(|n: &u64| *n).new(|n: &u64| *n + 1).inputs([("a", 1u64)]).equal().run(&cfg(Some(0.0))).unwrap_err();
    assert_eq!(err, BenchError::OutputMismatch("a".into()));
}

#[test]
fn custom_check_allows_a_tolerance() {
    let r = Paired::new("approx")
        .old(|x: &f64| *x)
        .new(|x: &f64| *x + 1e-12)
        .inputs([("x", 1.0)])
        .check(|a, b| (a - b).abs() < 1e-9)
        .run(&Config { repetitions: 4, resamples: 50, ..cfg(Some(0.0)) })
        .unwrap();
    assert!(r.config.output_checked);
}

#[test]
fn builder_errors() {
    let c = cfg(Some(0.0));
    assert_eq!(Paired::new("x").new(|n: &u64| *n).inputs([("a", 1u64)]).run(&c).unwrap_err(), BenchError::MissingOld);
    assert_eq!(Paired::new("x").old(|n: &u64| *n).inputs([("a", 1u64)]).run(&c).unwrap_err(), BenchError::MissingNew);
    assert_eq!(Paired::new("x").old(|n: &u64| *n).new(|n: &u64| *n).run(&c).unwrap_err(), BenchError::NoInputs);
    assert_eq!(
        Paired::new("x").old(|n: &u64| *n).new(|n: &u64| *n).inputs([("a", 1u64), ("a", 2)]).run(&c).unwrap_err(),
        BenchError::DuplicateInput("a".into())
    );
}

#[test]
fn high_injected_load_marks_the_run_contended_and_unpublishable() {
    let r = worked_example(&cfg(Some(3.0))); // 4 cores: limit 2.0
    assert!(r.contended && !r.publishable);
    assert_eq!(r.guard.load_before, Some(3.0));
    assert!(r.require_publishable().is_err());
    assert!(r.to_markdown().contains("NOT PUBLISHABLE"));
}

#[test]
fn short_or_unchecked_runs_are_not_publishable() {
    let r = Paired::new("quick").old(|n: &u64| *n).new(|n: &u64| *n).inputs([("a", 1u64)]).run(&Config { repetitions: 4, ..cfg(Some(0.0)) }).unwrap();
    let why = r.publish_problems().join(";");
    assert!(!r.publishable && why.contains("repetitions") && why.contains("equality"), "{why}");
}

#[test]
fn json_round_trips_and_rejects_other_schema_versions() {
    let r = worked_example(&cfg(Some(0.1)));
    let json = r.to_json();
    assert_eq!(Report::from_json(&json).unwrap(), r);
    let bumped = json.replacen("\"schema_version\": 1", "\"schema_version\": 2", 1);
    assert_eq!(Report::from_json(&bumped).unwrap_err(), ReportError::Schema { found: 2 });
    assert!(matches!(Report::from_json("{"), Err(ReportError::Json(_))));
}

#[test]
fn compare_gate_flags_a_regression() {
    let base = worked_example(&cfg(Some(0.1)));
    let mut cand = base.clone();
    for i in &mut cand.inputs {
        i.wall.ratio.point *= 1.5;
        i.wall.new.median_ns *= 1.5;
    }
    let same = compare(&base, &base, Basis::Ratio, Clock::Wall, Some(5.0));
    assert!(!same.failed());
    assert!(same.geomean_change_pct.abs() < 1e-9);

    let worse = compare(&base, &cand, Basis::Ratio, Clock::Wall, Some(10.0));
    assert!(worse.failed());
    assert!(worse.rows.iter().all(|r| (r.change_pct - 50.0).abs() < 1e-6));
    assert!(worse.to_markdown().contains("REGRESSION"));
    assert!(!compare(&base, &cand, Basis::NewMedian, Clock::Wall, Some(60.0)).failed());
    assert!(!compare(&base, &cand, Basis::Ratio, Clock::Wall, None).failed());
}
