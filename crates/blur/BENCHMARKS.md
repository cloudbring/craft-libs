# 24 MP Gaussian timings

## Wrap and sparse regions

November 3, 2026, same rustup release toolchain and host as below. The opt-in
`wrap_region::bench_wrap_padding_and_sparse_region` retains the original four
benchmarks and separately compares the new APIs. Input/filter construction and
input copies are outside the timers. Wrap/padding uses one warmup and four
measured renders with reused scratch; the reported value is the upper median.
The padding baseline uses `ceil(24 * sigma) + 32` pixels on every side, excludes
padding construction/cropping, and checks every retained output bit each run.

| Plane / request | Baseline | New API | Observation |
| --- | ---: | ---: | --- |
| 2048², Recursive sigma 64, periodic | Padded Clamp 86.52 ms | Wrap 32.38 ms | 2.67× faster; 0 differing bits over 4,194,304 samples per run |
| 4096², Recursive sigma 16, interior 64² tile | Full-plane Clamp 58.75 ms | Region 0.524 ms | 112.1× faster; maximum absolute error 2.3841858e-7 |

The region evaluates a 320² working window (64² output plus a 128-pixel halo on
each side), not the 4096² atlas. That timing is a single observation, not a speed
threshold. All measurements are serial; callers can retain their own worker
scheduling. Wrap's sigma <= 128 warmup lives only in per-worker line scratch,
not a padded plane. The larger-sigma steady-state path has no sigma-sized pad.

Numerical regressions additionally exercise 1–4 channels, all edges, tiny periods,
wide sampled/box kernels, border/seam/interior rectangles, and adversarial steps,
impulses and checkerboards just beyond the recursive halo. The 8-sigma truncation
is **not** bit-exact: the observed adversarial maximum absolute region/full-plane
error is 4.696846e-5 on unit-range planes; all cases satisfy the documented
pole-envelope bound plus the explicit 2e-6 roundoff allowance. Normal signed/HDR
fixtures have maximum error 6.0386956e-6. See README for the analytic bound and
the finite-padding bit-compatibility domain.

All checks use the rustup compiler, explicitly pinned because this host also has
a Homebrew compiler on PATH:

```sh
export PATH="$(dirname "$(rustup which cargo)"):$PATH"
export RUSTC="$(rustup which rustc)"
export RUSTDOC="$(rustup which rustdoc)"
export CARGO_TARGET_DIR="$PWD/target-2"
rustup run stable cargo test -p craft-blur --release --test wrap_region -- --ignored --nocapture --test-threads=1
```

## Original whole-plane measurements

Measured November 3, 2026 on Apple M6, macOS 27.0.1, aarch64, Rust 1.99.0,
Cargo release defaults and the rustup compiler. One serial 6000x4000 interleaved f32 plane, one channel,
Clamp edges, one timed render per cell; no rayon or other runtime dependency.
Input generation/copy and Gaussian construction are outside the timed interval.
Scratch grows during the first render and is reused for subsequent sigmas.
These are individual observations, not statistically stabilized benchmarks.

| Quality | sigma 1 | sigma 8 | sigma 64 |
| --- | ---: | ---: | ---: |
| Exact | 0.033 s | 0.203 s | 1.573 s |
| Recursive | 0.033 s | 0.068 s | 0.068 s |
| BoxApprox, 3 passes | 0.164 s | 0.152 s | 0.153 s |

Recursive at sigma 1 executes LightCraft's sampled fast path, also used by Exact.
The 2.5–8 shape-contract guard band remains sampled, as documented in README.
Box initialization scans the input, never the radius-sized extension. Recursive
Clamp cost is independent of radius. Box code is unchanged; differences from its
previous timings are measurement conditions, not a claimed box optimization.

## Same-binary LightCraft comparison

The independent fixture is the verbatim image kernel from LightCraft commit
`39aa4d256939c3703f1e22c0f2a715a4c98a189f`,
`crates/raster/src/blur/interleaved.rs`, with only the Image/Pixel import adapted.
It shares the verbatim independent coefficient constructor with the scalar
differential fixture, and never calls craft-blur's arithmetic. It runs serially:
there is no parallel feature or rayon dependency in craft-blur.

`bench_lightcraft_comparison` performs two warmups and seven measured renders per
sigma/channel count, alternating craft/upstream run order. Each reported time is
the median. Every iteration checks every output bit, not just a checksum. The
ignored benchmark asserts craft/LightCraft <=1.10 for all three one-channel cases;
this is an opt-in performance regression gate, not a normal CI timing threshold.

Both filters are preconstructed and receive the same packed samples. Craft's
in-place input copy is outside its timer and its worker scratch is reused;
LightCraft's unchanged `filter_image` timer includes its output allocation/copy
and internal per-worker scratch allocation. Thus these measure each actual API,
not artificially allocation-equalized inner loops. No assembly-derived claims
are needed: the unchanged upstream path is timed inside the same executable.

| Channels | sigma | craft (ms) | LightCraft (ms) | craft/upstream | craft ms/channel |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 1 | 1 | 32.53 | 33.50 | 0.971 | 32.53 |
| 1 | 8 | 68.55 | 70.34 | 0.975 | 68.55 |
| 1 | 64 | 68.40 | 70.30 | 0.973 | 68.40 |
| 3 | 1 | 162.95 | 165.45 | 0.985 | 54.32 |
| 3 | 8 | 158.22 | 161.21 | 0.981 | 52.74 |
| 3 | 64 | 156.76 | 161.23 | 0.972 | 52.25 |
| 4 | 1 | 222.45 | 223.70 | 0.994 | 55.61 |
| 4 | 8 | 229.63 | 254.32 | 0.903 | 57.41 |
| 4 | 64 | 223.27 | 251.95 | 0.886 | 55.82 |

All one-channel results also meet the requested absolute ceilings
36.96/77.55/77.22 ms (110% of 33.6/70.5/70.2 ms). RGB/RGBA figures are measured
costs, not an assumption that one-channel SIMD throughput scales linearly.
Scheduling, cache state and memory pressure still affect wall-clock timings.

### Regression and cause

Before the fix, the same-binary benchmark failed at sigma 1: craft 116.28 ms,
LightCraft 37.54 ms, ratio 3.098. The previously reported Recursive timings were
138/196/188 ms. The old engine gathered each eight-line batch into padded
four-channel f64 input/output buffers, even for one channel, then scattered the
results; it also used a general position/tap/lane sampled loop with edge mapping.

The Clamp hot path now reads/writes packed image pixels directly, interleaves
eight recurrence chains, uses channel-sized forward scratch, and traverses
columns as adjacent row-major blocks. Sampled passes use a padded seven-tap
horizontal specialization and 64-row vertical working tiles retaining original
halos, with unchanged f64 summation order and per-eight-line cancellation.
No full-image transpose/intermediate, per-line allocations, unsafe code, new
dependencies, coefficient changes, or shape-bound relaxation were introduced.

Reproduce the quality timings and same-binary comparison:

```sh
export PATH="$(dirname "$(rustup which --toolchain stable rustc)"):$PATH"
export RUSTC="$(rustup which --toolchain stable rustc)"
export RUSTDOC="$(rustup which --toolchain stable rustdoc)"
CARGO_TARGET_DIR="$PWD/target-2" rustup run stable cargo test -p craft-blur \
  --release --test bench -- --ignored --nocapture --test-threads=1
```

Validation at this revision: 20 ordinary unit/integration tests plus one doctest
passed; all four ignored benchmark tests passed when explicitly enabled. Zero test
failures. Three proptests each cover 96 generated planes. The independent LightCraft
fixture is bit-identical for the raw recurrence at sigma 2.5/4/8/16/32/64, the public
Recursive path at 8/16/32/64, and the sampled fallback at 0.5/1/2/2.499. Image-path
tests additionally cover short rows, channel tails, 64-row tile crossings, halos
wider than a tile, scratch reuse for every channel count, and cancellation after
at most eight sampled output rows. The public guard
band is intentionally different from raw LightCraft below 8, as README explains.

The wasm32 check requires the **rustup compiler**, not only rustup's cargo, on this
machine (Homebrew's compiler is first on the ordinary PATH):

```sh
RUSTC="$(rustup which --toolchain stable rustc)" \
RUSTDOC="$(rustup which --toolchain stable rustdoc)" \
CARGO_TARGET_DIR="$PWD/target-2" \
rustup run stable cargo check -p craft-blur --target wasm32-unknown-unknown
```

Clippy (`--all-targets -- -D warnings`), formatting, rustdoc (`--no-deps`) and
wasm32 checks passed. Existing workspace warnings about two unused egui patches
are unrelated to this crate. `target-2` is temporary validation output and is not
part of the commit.
