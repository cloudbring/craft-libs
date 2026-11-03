# 24 MP Gaussian timings

Measured November 3, 2026 on Apple M6, macOS 27.0.1, aarch64, Rust 1.99.0,
Cargo release defaults. One serial 6000x4000 interleaved f32 plane, one channel,
Clamp edges, one timed render per cell; no rayon or other runtime dependency.
Input generation/copy and Gaussian construction are outside the timed interval.
Scratch grows during the first render and is reused for subsequent sigmas.
These are individual observations, not statistically stabilized benchmarks.

| Quality | sigma 1 | sigma 8 | sigma 64 |
| --- | ---: | ---: | ---: |
| Exact | 0.177 s | 0.554 s | 2.818 s |
| Recursive | 0.138 s | 0.196 s | 0.188 s |
| BoxApprox, 3 passes | 0.309 s | 0.261 s | 0.245 s |

Recursive at sigma 1 executes the documented sampled guard path. The two sampled
sigma-1 timings run the same code; their difference is run-order/cache noise, not
an algorithmic performance claim. Box initialization scans the input, never the
radius-sized extension. Recursive Clamp cost is independent of radius.

Reproduce all nine timings:

```sh
CARGO_TARGET_DIR="$PWD/target-2" rustup run stable cargo test -p craft-blur \
  --release --test bench -- --ignored --nocapture --test-threads=1
```

Validation at this revision: 15 ordinary unit/integration tests plus one doctest
passed; three ignored benchmark tests passed when explicitly enabled. Zero test
failures. Two proptests each cover 96 generated planes. The independent LightCraft
fixture is bit-identical for the raw recurrence at sigma 2.5/4/8/16/32/64, the public
Recursive path at 8/16/32/64, and the sampled fallback at 0.5/1/2. The public guard
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
