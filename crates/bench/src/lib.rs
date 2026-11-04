//! Paired old-vs-new benchmark harness (artcraft-algorithms#189).
//!
//! One binary holds both implementations (the consumer links its pre-change code under a
//! renamed git dependency, as the `craft-oracle` README describes). Runs alternate in seeded
//! ABBA blocks, each is timed on the wall clock and the process CPU clock, and the result is a
//! median, a MAD and a bootstrap 95% CI of the `new / old` ratio per input, plus a geometric
//! mean across inputs. A run on a loaded or throttled machine is marked `contended` and is not
//! publishable. See the README for the JSON schema and the evidence standard.
//!
//! ```
//! use craft_bench::{Config, Paired};
//! let report = Paired::new("sum")
//!     .old(|n: &u64| (0..*n).sum::<u64>())
//!     .new(|n: &u64| n * (n.saturating_sub(1)) / 2)
//!     .inputs([("n=1000", 1000u64)])
//!     .equal()
//!     .run(&Config { repetitions: 4, resamples: 100, ..Config::default() })
//!     .unwrap();
//! assert_eq!(report.inputs.len(), 1);
//! ```

pub mod compare;
pub mod paired;
pub mod probe;
pub mod report;
pub mod rng;
pub mod schedule;
pub mod stats;

pub use paired::{BenchError, Config, Paired, PairedBench};
pub use probe::{Fingerprint, Guard, SystemProbe};
pub use report::{Report, SCHEMA_VERSION};
