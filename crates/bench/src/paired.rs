//! The paired runner.

use std::hint::black_box;
use std::sync::Arc;
use std::time::Instant;

use crate::probe::{Fingerprint, Guard, Readings, RealProbe, SystemProbe, evaluate};
use crate::report::{
    Aggregate, ConfigRecord, InputResult, MIN_REPETITIONS, MIN_RESAMPLES, MetricResult, Ratio, Report, Revs, RunRecord, SCHEMA_VERSION, SideStats,
};
use crate::rng::derive;
use crate::schedule::{self, Side};
use crate::stats;

/// Entry point: `Paired::new("name").old(..).new(..).inputs(..).run(&Config::default())`.
///
/// `Paired` is only a namespace so that both the constructor and the `.new(..)` builder method
/// can exist; [`Paired::new`] returns a [`PairedBench`].
pub struct Paired;

impl Paired {
    #[allow(clippy::new_ret_no_self)]
    pub fn new<'a, I, O>(name: impl Into<String>) -> PairedBench<'a, I, O> {
        PairedBench { name: name.into(), old: None, new: None, inputs: Vec::new(), check: None }
    }
}

type Func<'a, I, O> = Box<dyn Fn(&I) -> O + 'a>;
type Check<'a, O> = Box<dyn Fn(&O, &O) -> bool + 'a>;

/// A benchmark under construction. `I` is the input type, `O` the output both sides return.
pub struct PairedBench<'a, I, O> {
    name: String,
    old: Option<Func<'a, I, O>>,
    new: Option<Func<'a, I, O>>,
    inputs: Vec<(String, I)>,
    check: Option<Check<'a, O>>,
}

/// Run settings. `Config::default()` meets the evidence standard.
#[derive(Clone)]
pub struct Config {
    /// Timed runs per side per input, rounded up to even (ABBA blocks).
    pub repetitions: usize,
    /// Untimed runs of each side per input before timing starts.
    pub warmup: usize,
    pub resamples: usize,
    pub seed: u64,
    /// Load above this many per logical core marks the run contended.
    pub max_load_per_core: f64,
    pub revs: Revs,
    /// Source of load and throttle readings; replace in tests.
    pub probe: Arc<dyn SystemProbe>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            repetitions: MIN_REPETITIONS,
            warmup: 2,
            resamples: MIN_RESAMPLES,
            seed: 0x00C0_FFEE,
            max_load_per_core: 0.5,
            revs: Revs::default(),
            probe: Arc::new(RealProbe),
        }
    }
}

impl std::fmt::Debug for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Config")
            .field("repetitions", &self.repetitions)
            .field("warmup", &self.warmup)
            .field("resamples", &self.resamples)
            .field("seed", &self.seed)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BenchError {
    #[error("no old implementation set")]
    MissingOld,
    #[error("no new implementation set")]
    MissingNew,
    #[error("no inputs")]
    NoInputs,
    #[error("duplicate input name {0:?}")]
    DuplicateInput(String),
    #[error("repetitions must be at least 2 and resamples at least 1")]
    BadConfig,
    #[error("old and new outputs differ on input {0:?}; timings do not count")]
    OutputMismatch(String),
}

impl<'a, I, O> PairedBench<'a, I, O> {
    /// The pre-change implementation.
    #[must_use]
    pub fn old(mut self, f: impl Fn(&I) -> O + 'a) -> Self {
        self.old = Some(Box::new(f));
        self
    }

    /// The post-change implementation.
    #[must_use]
    #[allow(clippy::should_implement_trait)]
    pub fn new(mut self, f: impl Fn(&I) -> O + 'a) -> Self {
        self.new = Some(Box::new(f));
        self
    }

    /// Named inputs. May be called more than once.
    #[must_use]
    pub fn inputs<N: Into<String>>(mut self, it: impl IntoIterator<Item = (N, I)>) -> Self {
        self.inputs.extend(it.into_iter().map(|(n, i)| (n.into(), i)));
        self
    }

    /// Output-equality check, run once per input before any timing. `true` means equal.
    #[must_use]
    pub fn check(mut self, f: impl Fn(&O, &O) -> bool + 'a) -> Self {
        self.check = Some(Box::new(f));
        self
    }

    /// Output-equality check using `PartialEq`.
    #[must_use]
    pub fn equal(self) -> Self
    where
        O: PartialEq,
    {
        self.check(|a, b| a == b)
    }

    pub fn run(self, cfg: &Config) -> Result<Report, BenchError> {
        let old = self.old.as_ref().ok_or(BenchError::MissingOld)?;
        let new = self.new.as_ref().ok_or(BenchError::MissingNew)?;
        if self.inputs.is_empty() {
            return Err(BenchError::NoInputs);
        }
        if cfg.repetitions < 2 || cfg.resamples == 0 {
            return Err(BenchError::BadConfig);
        }
        for (i, (n, _)) in self.inputs.iter().enumerate() {
            if self.inputs[..i].iter().any(|(m, _)| m == n) {
                return Err(BenchError::DuplicateInput(n.clone()));
            }
        }
        let probe = cfg.probe.as_ref();
        let (load_before, throttle_before) = (probe.load1(), probe.throttle());

        if let Some(check) = &self.check {
            for (n, input) in &self.inputs {
                if !check(&black_box(old(input)), &black_box(new(input))) {
                    return Err(BenchError::OutputMismatch(n.clone()));
                }
            }
        }
        for (_, input) in &self.inputs {
            for _ in 0..cfg.warmup {
                black_box(old(input));
                black_box(new(input));
            }
        }

        let steps = schedule::build(self.inputs.len(), cfg.repetitions, cfg.seed);
        let mut runs = Vec::with_capacity(steps.len());
        for (order, step) in steps.iter().enumerate() {
            let (name, input) = &self.inputs[step.input];
            let f = if step.side == Side::Old { old } else { new };
            let (wall_ns, cpu_ns) = time_one(f.as_ref(), input);
            runs.push(RunRecord { order, input: name.clone(), side: step.side, wall_ns, cpu_ns });
        }

        let logical_cores = probe.logical_cores();
        let guard = evaluate(Readings {
            load_before,
            load_after: probe.load1(),
            logical_cores,
            max_load_per_core: cfg.max_load_per_core,
            throttle_before,
            throttle_after: probe.throttle(),
        });
        Ok(self.assemble(cfg, runs, guard, logical_cores))
    }

    fn assemble(&self, cfg: &Config, runs: Vec<RunRecord>, guard: Guard, logical_cores: usize) -> Report {
        let cpu_ok = runs.iter().all(|r| r.cpu_ns.is_some());
        let mut inputs = Vec::new();
        let (mut wall_dists, mut cpu_dists) = (Vec::new(), Vec::new());
        let (mut wall_points, mut cpu_points) = (Vec::new(), Vec::new());
        for (idx, (name, _)) in self.inputs.iter().enumerate() {
            let pick = |side: Side, cpu: bool| -> Vec<f64> {
                runs.iter()
                    .filter(|r| r.input == *name && r.side == side)
                    .filter_map(|r| if cpu { r.cpu_ns } else { Some(r.wall_ns) })
                    .map(|v| v as f64)
                    .collect()
            };
            let (wall, wd) = metric(&pick(Side::Old, false), &pick(Side::New, false), cfg, derive(cfg.seed, idx, 1));
            wall_points.push(wall.ratio.point);
            wall_dists.push(wd);
            let cpu = cpu_ok.then(|| metric(&pick(Side::Old, true), &pick(Side::New, true), cfg, derive(cfg.seed, idx, 2)));
            let cpu = cpu.map(|(m, d)| {
                cpu_points.push(m.ratio.point);
                cpu_dists.push(d);
                m
            });
            inputs.push(InputResult { name: name.clone(), wall, cpu });
        }
        let agg = |points: &[f64], dists: &[Vec<f64>]| {
            let (ci_lo, ci_hi) = stats::geomean_ci(dists);
            Ratio { point: stats::geomean(points), ci_lo, ci_hi }
        };
        let aggregate = Aggregate { inputs: inputs.len(), wall: agg(&wall_points, &wall_dists), cpu: cpu_ok.then(|| agg(&cpu_points, &cpu_dists)) };
        let config = ConfigRecord {
            requested_repetitions: cfg.repetitions,
            runs_per_side: schedule::runs_per_side(cfg.repetitions),
            warmup_runs: cfg.warmup,
            resamples: cfg.resamples,
            seed: cfg.seed,
            output_checked: self.check.is_some(),
            cpu_time: if cpu_ok { "process" } else { "unavailable" }.to_string(),
        };
        let mut report = Report {
            schema_version: SCHEMA_VERSION,
            name: self.name.clone(),
            revs: cfg.revs.clone(),
            fingerprint: Fingerprint::collect(logical_cores),
            config,
            contended: guard.contended,
            guard,
            publishable: false,
            inputs,
            aggregate,
            runs,
        };
        report.publishable = report.publish_problems().is_empty();
        report
    }
}

fn metric(old: &[f64], new: &[f64], cfg: &Config, seed: u64) -> (MetricResult, Vec<f64>) {
    let side = |v: &[f64]| SideStats {
        n: v.len(),
        median_ns: stats::median(v).unwrap_or(0.0),
        mad_ns: stats::mad(v).unwrap_or(0.0),
        min_ns: v.iter().copied().fold(f64::INFINITY, f64::min),
        max_ns: v.iter().copied().fold(0.0, f64::max),
    };
    let (o, n) = (side(old), side(new));
    let dist = stats::bootstrap_ratios(old, new, cfg.resamples, seed);
    let (ci_lo, ci_hi) = stats::ci95(&dist);
    let ratio = Ratio { point: stats::ratio_of_medians(n.median_ns, o.median_ns), ci_lo, ci_hi };
    (MetricResult { old: o, new: n, ratio }, dist)
}

#[cfg(any(unix, windows))]
fn cpu_now() -> Option<std::time::Duration> {
    cpu_time::ProcessTime::try_now().ok().map(|t| t.as_duration())
}

/// No process CPU clock on this target: CPU columns are null.
#[cfg(not(any(unix, windows)))]
fn cpu_now() -> Option<std::time::Duration> {
    None
}

fn time_one<I, O>(f: &dyn Fn(&I) -> O, input: &I) -> (u64, Option<u64>) {
    let c0 = cpu_now();
    let t0 = Instant::now();
    let out = f(black_box(input));
    let wall = t0.elapsed();
    let c1 = cpu_now();
    black_box(&out);
    drop(out);
    let cpu = c0.zip(c1).map(|(a, b)| u64::try_from(b.saturating_sub(a).as_nanos()).unwrap_or(u64::MAX));
    (u64::try_from(wall.as_nanos()).unwrap_or(u64::MAX), cpu)
}
