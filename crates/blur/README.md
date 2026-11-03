# craft-blur

Dependency-free, safe Rust separable filtering of packed interleaved `f32` planes
with one through four channels. There is no app model, implicit quality selection,
global thread pool, SIMD dependency, or unsafe code. The workspace already uses
rayon, but the crate does not require it. `proptest` is an existing workspace
development dependency.

## API

```rust
use craft_blur::{Cancel, Edge, Gaussian, Quality, Scratch, Shape};

let shape = Shape::new(64, 32, 4)?;
let mut premultiplied_rgba = vec![0.0; shape.samples()];
let filter = Gaussian::new(8.0, Quality::Recursive)?;
let mut scratch = Scratch::default();
filter.apply(&mut premultiplied_rgba, shape, Edge::Transparent,
             &mut scratch, Cancel::Never)?;
# Ok::<(), craft_blur::Error>(())
```

`Gaussian::separable()` exposes `Separable::Kernel`, `Recursive` coefficients,
or three odd box widths as data for another device. `Separable::reach()` returns
finite support per axis for sampled/box passes and `None` for recursive passes:
an infinite recursive response must not be treated as a finite tile halo.
`gaussian_kernel(sigma)` returns the variance-calibrated sampled kernel.
`box_blur` runs one centered running-sum box per axis. `box_widths` exposes the
raw Kovesi widths independently of the Gaussian quality acceptance check.

Invalid layouts, channels, sigmas and pass counts return `Error`; checked size
arithmetic and fallible buffer reservations avoid overflow and allocation panics.
Sigma is a finite, nonnegative `f32` in **pixels**, not a diameter or a UI radius.
Zero is the identity. Empty planes are valid. Sampled kernels have no numerical
sigma cutoff; unrepresentable support or unavailable memory returns an error.
Inputs are expected to be finite; NaNs/infinities in samples propagate through
arithmetic, and are not sanitized or clamped. Negative and HDR samples are allowed.

## Explicit quality contract (#2, #52)

All variance measurements use normalized discrete second moments of a centered
one-dimensional impulse, far enough from the image boundary (32 sigma, at least
64 pixels). They are not claims about variance after cropping a finite image.
Shape error is the **maximum absolute difference between unit-mass impulse taps**
and `Exact`, not relative tap error, an L1/operator-norm bound, or a guarantee for
arbitrary input amplitudes. Each axis narrows to `f32`; recurrence states and
sampled/running-sum accumulation are `f64`.

| Quality | Algorithm | Enforced contract |
| --- | --- | --- |
| `Exact` | Symmetric sampled Gaussian, support `ceil(3 sigma)` on each side; calibrated sampling width by 48 bisections | Discrete variance error below 0.5% for the tested sigma range 0.5–64; no integer-width quantization. “Exact” means this finite calibrated sampled kernel, not an infinite continuous Gaussian or FFT. |
| `Recursive` | Third-order Young–van Vliet poles, five Newton refinements to match the analytic second moment, Triggs–Sdika initialization, eight independent interleaved `f64` chains per channel | Variance error below 0.5% on 0.5–64; maximum absolute impulse shape error **at most 0.01 for sigma >= 2.5** in that tested range. Uses sampled Exact below 8 and above 65,536. |
| `RecursiveRaw` | The same YvV coefficients, boundaries, precision and execution path, with the upstream kernel-selection threshold | Uses sampled Exact below 2.5 and above 65,536. Clamp output matches LightCraft V3 bit-for-bit through 65,536; no 0.01 impulse-shape guarantee in the 2.5–8 band. |
| `BoxApprox { passes: 3 }` | Kovesi neighboring odd widths; three running-sum boxes on each axis | Actual relative variance error **at most `1/(3 sigma)`**, checked at construction. Unsupported pass counts are errors, not ignored options. |

### Two necessary contract corrections

The supplied LightCraft kernel is not within 0.01 at every sigma >= 2.5.
Its `docs/recursive-gaussian.md` at commit `39aa4d25` records raw errors
0.023720975 at 2.5 and 0.011704820 at 4 (against a normalized sampled Gaussian).
Consequently the public `Recursive` quality has an explicit **sampled guard band
below 8**, extending the upstream below-2.5 fallback rather than weakening the
requested bound or modifying the borrowed recurrence. Above 65,536 it also uses
Exact, avoiding the numerically ill-conditioned high-sigma third-order recurrence.
No shape/variance certification is claimed for the raw recurrence beyond the
tested range; consumers requiring an unconditional sampled result should use Exact.

Use `Gaussian::new(sigma, Quality::RecursiveRaw)` when preserving LightCraft V3
output is the contract. The former public-API mismatch at 2.5 <= sigma < 8 was
entirely kernel selection: `Recursive` chose sampled Exact while V3 chose YvV.
The q(sigma) calibration, Newton refinements, Triggs–Sdika boundaries and f64
scratch arithmetic already match verbatim. `RecursiveRaw` exposes that existing
recurrence without changing `Recursive`'s shape guarantee or existing timings.
The independent verbatim test fixture also matches the compatibility kernel in
LightCraft commit `26940d64`, `crates/raster/src/blur.rs::RecursiveCompatibility`.
Random-plane differential tests cover the requested sigmas 2.5, 3, 4, 5, 6, 7
and 7.99, sampled/recursive boundaries, both q branches, and sigma 65,536, for
one through four channels using both whole-plane and split-axis APIs.

The proposed box **variance** bound is likewise not universally possible for
subpixel sigma: at 0.5, Kovesi chooses `[1, 1, 1]`, giving variance zero,
relative variance error 1, and requested bound 2/3. `Gaussian::new` returns
`Error::BoxTolerance { actual, bound }` for every such unattainable request.
It does not silently change sigma, add passes, relax the bound, or select Exact.
`box_widths` is available when an app explicitly accepts the raw quantization.
Tests assert the analytic and rendered bound, and sweep construction acceptance
over sigma 0.01–64 in 0.01 increments. At sigma zero the bound and error are
defined as zero and the result is the identity.

## Edges and alpha

- `Clamp`: repeat the first/last original sample. Recursive initialization models
  the causal continuation of that original signal, not a clamped causal buffer.
- `Mirror`: half-sample reflection, including the edge pixel. Sampled and box
  extension is exact. Recursive filtering uses 32 sigma of periodic reflected
  extension on each side before Triggs–Sdika initialization; the residual agrees
  with brute-force impulse convolution within 2e-6 on the small-image fixtures.
- `Zero`: zero extension without border renormalization; constants fade at edges.
- `Transparent`: zero extension of **already-premultiplied** gray-alpha/RGBA
  (2/4 channels). Output remains premultiplied. No unpremultiplication, RGB gamma
  conversion, color management, or hidden-color repair is implied.

## Cancellation, workers and strips

`Cancel::Flag(&AtomicBool)` and `Cancel::Hook(&closure)` are checked before each
batch of at most `LINE_BATCH = 8` independent rows/columns. A cancellation that
arrives during a batch stops before the next batch; this is a line-count bound,
not a wall-clock guarantee. `Error::Cancelled` leaves completed batches modified;
the app discards that render. Reuse one `Scratch` per worker between calls.
Scratch buffers grow to a high-water mark; no allocation occurs per line after
capacity is available. The whole-plane convenience call allocates row references,
but no full-image intermediate or transpose.

Clamp recurrence passes read/write the image directly, interleaving eight rows
or eight adjacent columns with `[[f64; 8]; CHANNELS]` forward scratch. One-channel
planes do not pay for unused RGBA lanes or the generic edge engine's gathered
input/output buffers. The column pass traverses row-major eight-pixel blocks.
`Exact` and the `Recursive` sampled fallback use a padded horizontal window
(unrolled for the seven-tap sigma-1 kernel), then a 64-row vertical working tile
plus its kernel halos. Both halos are retained from the horizontal result before
in-place vertical writes; only an eight-row output batch is advanced between
cancellation checks. The split line APIs produce identical results, but only
`apply` uses the cache-local sampled vertical tiles. Other edges and boxes retain
the general edge-policy engine; all accumulation orders are unchanged.

Apps may use their own rayon `par_chunks_mut(row_samples * LINE_BATCH)` with
`for_each_init(Scratch::default, ...)` and `filter_rows`, passing the batch's
`Shape`. For columns, split every row into equal-width disjoint stripes, collect
each stripe's mutable row slices, and distribute stripes to `filter_columns`.
Each stripe owns all rows for its columns; this is safe, transpose-free and
produces the same bits as the serial call. Never split a recursive line into
independently initialized tiles. Sampled/box output tiles require the full
`reach()` halo and the **original** extent's edge policy, not the tile's edges.

Box initialization uses a bounded prefix scan and closed-form edge sums, then
running additions/subtractions. Thus even boxes much wider than the image cost
O(image samples), not O(radius). Recursive work is independent of radius for
Clamp/Zero/Transparent; reflected padding increases Mirror's work with sigma.
Exact work is O(image samples * sampled support).

## Verification and provenance

The recurrence and coefficients come from `cloudbring/lightcraft` commit
`39aa4d256939c3703f1e22c0f2a715a4c98a189f`,
`crates/raster/src/blur.rs::{Gaussian::new, Gaussian::filter}` and
`crates/raster/src/blur/interleaved.rs::Gaussian::recursive`.
`src/lines/tests/lightcraft.rs` retains verbatim independent constructor/filter
bodies and sampled helper (only constructor visibility changed). Differential
image tests and the same-binary performance benchmark also use the verbatim
`src/lines/tests/lightcraft/interleaved.rs` image kernel, with only its Image/Pixel
import redirected to local test adapters. This is the actual eight-chain/tiled
upstream path, not the much slower scalar line reference. Differential tests
compare every output bit for 1–4 channels, short lines, row/column tails,
negative/HDR values, sigma 2.5–64 on the raw port, and public Recursive at >=8
and below 2.5, including 64-row tile boundaries. The public guard-band range
intentionally differs from raw YvV.
No app source is linked as a build/test dependency.

PhotoCraft's `crates/algo/src/blur.rs` and `separable.rs` informed the running-sum,
cancel/strip, and device-neutral pass APIs; no app migration is included.
Algorithm references: Young, van Vliet and van Ginkel, *Recursive Gabor
Filtering*, IEEE TSP 50(11), 2798–2805 (2002); Triggs and Sdika, *Boundary
Conditions for Young–van Vliet Recursive Filtering*, IEEE TSP 54(6), 2365–2367
(2006); Kovesi, *Fast Almost-Gaussian Filtering* (2010).

Run `cargo test -p craft-blur`, `cargo clippy -p craft-blur --all-targets -- -D warnings`,
`cargo fmt --check -p craft-blur`, and `cargo doc -p craft-blur --no-deps`.
The three quality benchmarks each run a 6000x4000 single-channel plane at sigma
1/8/64 serially, excluding allocation of the input/filter from timing. A fourth
ignored benchmark compares `Recursive` with the verbatim LightCraft image kernel
for 1/3/4 channels using seven-run medians after two warmups, alternating run
order. It checks every output bit on every iteration, and asserts a single-channel
craft/LightCraft ratio <=1.10:

```sh
cargo test -p craft-blur --release --test bench -- --ignored --nocapture --test-threads=1
```

Timing results are recorded in `BENCHMARKS.md`; they are not CI speed thresholds.
