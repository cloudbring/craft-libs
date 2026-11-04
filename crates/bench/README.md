# craft-bench

Paired old-vs-new benchmark harness: the evidence behind every "N times faster" claim
(research/VERIFICATION.md section 1.2). Ticket: artcraft-algorithms#189.

One binary holds both implementations. The consumer links its pre-change code under a renamed
git dependency (`old = { package = "craft-x", git = "...", rev = "..." }`, as the
`craft-oracle` README describes) and passes both as closures.

```rust
use craft_bench::{Config, Paired, report::Revs};

let report = Paired::new("blur r=8")
    .old(|img: &Image| old_blur::blur(img, 8.0))
    .new(|img: &Image| craft_blur::blur(img, 8.0))
    .inputs(corpus.iter().map(|f| (f.name.clone(), f.load())))
    .check(|a, b| a == b)              // run once per input; must pass before timings count
    .run(&Config { revs: Revs { old: Some("1a2b3c4".into()), new: Some("5d6e7f8".into()) }, ..Config::default() })?;
std::fs::write("blur.json", report.to_json())?;
println!("{}", report.to_markdown());
report.require_publishable()?;          // errors on a contended or under-sampled run
```

`Paired` is a namespace: `Paired::new(name)` returns a `PairedBench`, whose `.old`, `.new`,
`.inputs`, `.check` / `.equal` (uses `PartialEq`) and `.run` build and execute the benchmark.
Closures take `&Input` and return the output both sides must agree on. Work shorter than about
a microsecond is below timer resolution: batch it inside the closure.

## Method

- **Schedule.** Per input, `ceil(repetitions / 2)` blocks of four runs, `ABBA` or `BAAB` (A =
  old), start chosen at random; blocks of all inputs are shuffled. Each side runs `repetitions`
  rounded up to even per input. Seeded (`Config::seed`), so a report is reproducible.
- **Warm-up.** `Config::warmup` (default 2) untimed runs of each side per input.
- **Timing.** Each run records wall time (`Instant`) and process CPU time (`cpu-time`: getrusage /
  `CLOCK_PROCESS_CPUTIME_ID` on unix, `GetProcessTimes` on Windows; all threads of the process).
  On other targets CPU time is `null` and `config.cpu_time` is `"unavailable"`. Outputs go through
  `black_box` and are dropped outside the timed region.
- **Statistics.** Per input and per clock: median, MAD (raw, unscaled), min, max of each side;
  ratio `median(new) / median(old)` with a bootstrap 95% CI (percentile method, `resamples`
  default 2000, both samples resampled with replacement). Below 1.0 means new is faster; a CI
  that includes 1.0 is `inconclusive`, not a speedup. The aggregate is the geometric mean of the
  per-input ratios; its CI takes the geometric mean of each bootstrap resample across inputs.
  Medians are floored at 1 ns before dividing.
- **Quiet-machine guard.** The run is marked `contended` (and not `publishable`) when the 1-minute
  load average, read before or after the run, exceeds `max_load_per_core` (0.5) times the logical
  cores, when macOS `pmset -g therm` reports `CPU_Speed_Limit` below 100, or when the Linux
  `thermal_throttle` counter rises during the run. Load is `/proc/loadavg` or `sysctl vm.loadavg`;
  where neither exists load is `null` and is not treated as contention. The after-run load
  includes the benchmark's own load, so a many-thread benchmark on a few cores can trip it.
- **Publishable** means: not contended, at least 10 runs per side, at least 2000 resamples, and an
  output check ran. Only a publishable report may back a speedup claim.

## CLI

```
craft-bench show REPORT.json
craft-bench compare BASE.json CAND.json [--fail-over PCT] [--cpu] [--absolute] [--allow-contended]
```

`compare` matches inputs by name and prints the change of the judged quantity. By default it is
the `new/old` ratio, which cancels machine speed; `--absolute` judges the new side's median
instead (same machine only); `--cpu` judges CPU time. Any input whose change exceeds
`--fail-over` percent is a `REGRESSION`. Exit codes: 0 ok, 1 regression, 2 usage error,
unreadable file, unsupported schema, or a contended report (override with `--allow-contended`).

## JSON schema (`schema_version` 1)

All times are nanoseconds. Readers must reject other versions. Any change to field names or
meaning bumps the version.

```
{
  "schema_version": 1,
  "name": str,
  "revs": { "old": str|null, "new": str|null },
  "fingerprint": { "os", "arch", "kernel"|null, "cpu_model"|null, "logical_cores", "physical_cores"|null,
                   "ram_bytes"|null, "rustc", "profile", "opt_level", "debug_assertions", "date" /* RFC 3339 UTC */ },
  "config": { "requested_repetitions", "runs_per_side", "warmup_runs", "resamples", "seed",
              "output_checked": bool, "cpu_time": "process"|"unavailable" },
  "guard": { "contended": bool, "reasons": [str], "load_before"|null, "load_after"|null, "logical_cores",
             "max_load_per_core", "load_threshold",
             "throttle_before": { "speed_limit_pct"|null, "throttle_count"|null }, "throttle_after": {...} },
  "contended": bool,        // = guard.contended
  "publishable": bool,
  "inputs": [ { "name": str,
                "wall": Metric,
                "cpu": Metric|null } ],
  "aggregate": { "inputs": n, "wall": Ratio, "cpu": Ratio|null },
  "runs": [ { "order": n, "input": str, "side": "old"|"new", "wall_ns": u64, "cpu_ns": u64|null } ]
}
Metric = { "old": Side, "new": Side, "ratio": Ratio }
Side   = { "n", "median_ns", "mad_ns", "min_ns", "max_ns" }
Ratio  = { "point", "ci_lo", "ci_hi" }          // new / old
```

`runs` is the raw per-run data in execution order; every other number can be recomputed from it
with the stored seed.

## Not covered

GPU timing (a GPU consumer times its own submission inside the closure and states the device in
`name`), cache-state control, CPU frequency pinning, and comparing reports across machines.
