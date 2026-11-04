# craft-oracle

Old-vs-new differential harness. A consumer links its **pre-change** code next to the current
crate, runs both over every corpus file, and gets one outcome per file. This is the evidence
for "app X now uses craft-Y, behaviour unchanged" (research/VERIFICATION.md section 1.1).
Ticket: artcraft-algorithms#178. Builds on `craft-compare` (metrics, verdicts) and `craft-corpus`
(pinned inputs, globs).

## API

- `trait Side { type Output; fn run(&self, input: &[u8]) -> Result<Self::Output, String>; }`; `FnSide(closure)` adapts a closure.
- `trait Judge<O> { fn judge(&self, old: &O, new: &O) -> Verdict; }`. Ready judges: `BytesJudge`, `XmlJudge`, `ImageJudge::new(ImageTolerance)`, `FnJudge(closure)`.
- `Differential::new(old, new, judge)` with `.timeout(d)`, `.expected(Registry)`, `.policy(Policy)`, `.artifact_dir(dir)`, then `.run_files(iter of (name, bytes))` or `.run_corpus(&Corpus, tag)`. Each side runs in its own worker thread, with `catch_unwind` and a timeout. A timed-out thread cannot be killed; it is abandoned.
- `Outcome`: `Identical`, `WithinTolerance{metric,value}`, `ExpectedDifference{id}`, `Regression{metric,value,limit}`, `BothFail{old_err,new_err}`, `OnlyOldFails`, `OnlyNewFails`, `Timeout{side}`, `Panic{side,msg}`.
- `Report`: `counts`, `results`, `stale`; `to_json()`, `to_markdown(&policy)`, `write(dir, &policy)`, `failures(&policy)`, `exit_code(&policy)`.
- Exit policy: always fails on `Regression`, `OnlyNewFails`, any `Panic`, `Timeout` of the new side. Opt-in: `fail_on_both_fail`, `fail_on_stale`.
- Failing inputs (per the policy) are written to the artefact dir as `NNNNN-<name>.bin` plus `.json` with the outcome. They are not minimised.

## Linking the old code

In the consumer's `Cargo.toml`, next to the current crate:

```toml
[dev-dependencies]
craft-oracle = { git = "https://github.com/storytold/craft-libs", rev = "<craft-libs commit>" }
craft-corpus = { git = "https://github.com/storytold/craft-libs", rev = "<craft-libs commit>" }
old_impl = { package = "<crate>", git = "https://github.com/<org>/<repo>", rev = "<commit before swap>" }
```

`package =` renames the old crate so both revisions live in one test binary. In a test, wrap each
side and compare:

```rust
let corpus = craft_corpus::corpus_or_skip!("mycorpus", root);
let report = Differential::new(
    FnSide(|b: &[u8]| old_impl::convert(b).map_err(|e| e.to_string())),
    FnSide(|b: &[u8]| new_crate::convert(b).map_err(|e| e.to_string())),
    BytesJudge,
)
.expected(Registry::from_path("oracle/expected.toml".as_ref()).unwrap())
.artifact_dir("target/oracle-failures")
.run_corpus(&corpus, "convert")
.unwrap();
report.write("target/oracle".as_ref(), &Policy::default()).unwrap();
assert_eq!(report.exit_code(&Policy::default()), 0, "{}", report.to_markdown(&Policy::default()));
```

## Expected differences: `oracle/expected.toml`

```toml
[[expected]]
id = "texturecraft-blend-fix"            # unique
reason = "premultiplied blend corrected"
evidence = "cloudbring/artcraft-algorithms#NNN"   # ticket, Lean theorem or spec clause
glob = "blend/**"                         # file name glob (craft-corpus glob_match)
metric = "max_abs"                        # optional: the Regression metric
min_value = 0.05                          # optional bounds on the metric value
max_value = 0.2
```

`id`, `reason` and `evidence` are required, plus `glob` and/or `metric`. Entries turn a
`Regression` (and, with `glob` alone, `OnlyOldFails`/`OnlyNewFails`) into `ExpectedDifference`.
An entry that matched no file is listed in `Report::stale`; remove it or set `fail_on_stale`.

## Worked example

`tests/worked_example.rs` compares two revisions of a tiny function (two modules) over a
generated 17-file corpus: identical, within tolerance, one registered expected difference
(3 files), one regression caught, one file both sides reject, one stale entry.
