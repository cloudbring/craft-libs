# craft-scopes

Dependency-free, safe Rust CPU video scopes extracted for algorithm ticket #109.
No egui, GPU, rayon, serde, app dependencies, transfer curves, alpha policy or rendering.

## API and storage

- `Frame::interleaved(width, height, &[f32])`: packed R'G'B' triples.
- `Frame::planar(width, height, [&[f32]; 3])`: packed full-resolution R/G/B planes.
  Both borrow their input and validate exact lengths and dimension arithmetic.
  Values are **straight, display-encoded RGB**, nominally 0..1. Linear-light input must
  be encoded by the app. Y'CbCr input, subsampling, row padding and alpha are not implicit.
- `Accumulator::new(Config)` allocates a fallible, fixed-size `Vec<u64>` once.
  `clear`, `accumulate`, `accumulate_rows`, `merge`, and `scaled_into` allocate nothing.
  `accumulate` adds rather than replaces; clear before the next frame.
- `counts()` is plane-major, then row-major; `plane(index)` returns an optional borrowed
  slice. `ScopeKind::traces()` identifies the semantic planes. Waveform and parade widths
  are **per trace**, not the width of a combined coloured image.
- `FalseColour::new(thresholds, standard)` classifies unclamped luma into caller-defined
  exposure bands. `map_rows` fills a caller-owned `u16` output chunk. Band `n` counts
  thresholds less than or equal to luma; `u16::MAX` means invalid input. Apps choose colours.
  Neither original scopes source implements false colour, so this is new, palette-free code.

| Kind | Planes |
| --- | --- |
| WaveformRgb / ParadeRgb | R, G, B |
| WaveformLuma | Y |
| WaveformYcEnvelope | Y, Y ± chroma amplitude (FilmCraft YC) |
| WaveformChroma | Y, chroma amplitude (EffectCraft YC) |
| ParadeYuv | Y, Cb + 0.5, Cr + 0.5 |
| ParadeRgbWhite | R, G, B, Y |
| Histogram | R, G, B, Y; width bins, height **1** |
| Vectorscope | Cb horizontally, Cr upwards; one plane |

Rows default to lowest value first (`Origin::Bottom`); `Origin::Top` evaluates the
inverted fraction **before rounding**, rather than flipping rounded bin indices.
Value axes are linear and explicit (`Config::range`), independent of signal clamping.
Set `range = [-0.1, 1.1]` for FilmCraft's unclamped waveform or `[-0.25, 1.25]`
for EffectCraft's float waveform/histogram. FilmCraft histograms always use 0..1.
Vector half-extent defaults to 0.6; set 0.5 for EffectCraft's nominal chroma plane.
`Config::vector_cell(cb, cr)` maps a colour-bar target without allocating.

`ColorStandard::{Bt601, Bt709, Bt2020}` uses `(Kr, Kb)` of `(0.299, 0.114)`,
`(0.2126, 0.0722)`, `(0.2627, 0.0593)` respectively, with `Kg = 1 - Kr - Kb`.
The f32 evaluation order matches both original apps. BT.2020 is NCL, not constant
luminance. These coefficient tables are preserved from the source matrices, whose
definitions refer to ITU-R BT.601-7, BT.709-6 and BT.2020-2.

`Scale::{Linear, Logarithmic}` maps counts to `n/peak` or `ln(1+n)/ln(1+peak)`;
`scaled_into` uses the global peak across all planes and fills an existing f32 buffer.
These are **density** scales, not log exposure/PQ transfer functions. App-specific
phosphor floors, brightness gain, tinting, graticules and image layout stay in the apps.

## Caller-driven parallelism

Each worker owns one `Accumulator` with identical configuration. Pass the same borrowed
`Frame` and non-overlapping row ranges to `accumulate_rows`, then merge into a cleared
destination. Reuse both worker and destination buffers on subsequent frames. Scheduling
can use scoped threads, an app's rayon pool, or serial iteration; the crate starts no threads.
The input width is global, so row partitioning cannot change horizontal positions.
Workers must not submit overlapping rows unless double counting is intentional.

For false colour, split the output into non-overlapping mutable **row chunks** and call
`map_rows(frame, global_row_range, chunk)`; output length is the number of requested
rows times frame width, not necessarily the full-frame length. No reduction is needed.

```rust
use craft_scopes::{Accumulator, Config, Frame, ScopeKind};

let pixels = [0.5; 12];
let frame = Frame::interleaved(2, 2, &pixels)?;
let config = Config::new(ScopeKind::Histogram, 256, 1);
let mut top = Accumulator::new(config)?;
let mut bottom = Accumulator::new(config)?;
let mut result = Accumulator::new(config)?;
top.accumulate_rows(frame, 0..1)?;
bottom.accumulate_rows(frame, 1..2)?;
result.merge(&top)?;
result.merge(&bottom)?;
assert_eq!(result.counts()[128], 4);
# Ok::<(), craft_scopes::Error>(())
```

## Failure and overflow contracts

Zero/overflowing dimensions, invalid ranges/extents and mismatched buffers return `Error`.
Invalid row ranges, output sizes and merges fail before modifying the destination.
An empty row interval is valid. Construction checks count-storage arithmetic and uses
`try_reserve_exact`; no unchecked allocation is used for input-dependent storage.

Any non-finite RGB component skips the entire pixel, counted by `skipped()` (false colour
instead writes its invalid sentinel). This deliberately fixes inconsistent NaN/infinity
handling in the originals: EffectCraft's vector bounds comparisons can let NaN become
cell zero, while FilmCraft's histogram processes finite channels separately.
Out-of-range finite waveform/vector samples are dropped unless clamping applies.
Histograms report raw below/above counts independently of clamping; binning retains
FilmCraft's `1e-6` upper-end tolerance. YC envelope clamping includes the derived envelope.

Counts and diagnostics saturate at `u64::MAX` and set `saturated()` until `clear()`.
This makes repeated accumulation and merges deterministic instead of wrapping, and has
ample capacity for a DC 7680×4320 frame (33,177,600 samples in one histogram/vector cell).

## Provenance and differences

Base: `filmcraft/crates/scopes/src/lib.rs` at
`eb59ec09daaea519c9c8fffa404a594ae50d00e0`, particularly `build`/`waveform`/`parade`
(lines 457–511), `histogram` (531–563), `cell_of`/`vectorscope_yuv` (581–632), and
`filmcraft/crates/color/src/lib.rs` (`Matrix::kr_kb`, `rgb_to_ycbcr`). FilmCraft already
separates raw semantic traces from rendering; this is preferable to retaining EffectCraft's
render-colour planes and repeated frame/worker allocations in a low-level shared crate.

Comparison: `effectcraft/crates/raster/src/scopes.rs` at
`7870b5916ae0749d497d880989683bf2946d6c8b`, particularly `ScopeOpts::row`, `plot`,
`vec_cell`, and `ColorStandard::ycbcr`.

- FilmCraft draws YC as Y ± chroma; EffectCraft draws chroma magnitude itself. Both are
  exposed under distinct kinds rather than silently claiming they are equivalent.
- FilmCraft waveform row zero is low; EffectCraft row zero is high. FilmCraft vector
  extent is ±0.6; EffectCraft is ±0.5. Extended signal ranges also differ as noted above.
- FilmCraft stores separate parade traces; EffectCraft packs thirds into one canvas,
  duplicates Y/vector counts into display-colour planes and tints YUV traces in that canvas.
- FilmCraft includes Y in histograms and raw clipping diagnostics; EffectCraft only RGB.
- FilmCraft clamps histogram Y **after** converting raw RGB; other scopes convert clamped
  RGB. This distinction is retained. EffectCraft's histogram has no luma channel.
- FilmCraft decimates in its input adapter; EffectCraft samples every pixel and composites
  premultiplied input over black. Neither policy is imposed by this crate.
- EffectCraft's floating horizontal position can lose a column (13/22×22 becomes just
  below 13). This crate uses exact integer mapping, matching FilmCraft at native width.
- EffectCraft rounds vector coordinates before checking bounds, accepting slightly
  off-scale points. This crate keeps FilmCraft's strict pre-round bounds. Differential
  vector comparisons use nominal/clamped RGB where both bounds policies agree.
- HLS scope hue rotation/quantisation also differs (FilmCraft rotates red to its YUV angle
  and uses a 0.1° direction table; EffectCraft puts red on +X). HLS and UI/summary helpers
  are outside this crate step; the requested chroma vectorscope is supported.

## Verification

`tests/reference/{filmcraft,effectcraft}.inc` contains **verbatim accumulator function
bodies** copied from those paths. FilmCraft's serde derives alone are removed; minimal
local input/settings shims replace app dependencies. EffectCraft's unchanged `plot`,
`bump`, `vec_cell`, `hls` and coefficient/axis code run in a serial RGB harness instead
of its rayon/Image adapter. The frozen functions do not call the new crate's maths.
The `.inc` suffix keeps rustfmt from rewriting the original oracle text. Original helper
names/comments are deliberately preserved; lint allowances are confined to oracle modules.

Deterministic random frames use 16 independent seeds, all three matrices, clamp modes,
and nominal/extended ranges. FilmCraft waveform, YC envelope, all three parades, RGB/Y
histograms with diagnostics and vectorscope agree **exactly**. EffectCraft's shared
scopes agree **exactly** after explicitly selecting axes/extents, extracting parade
thirds and removing duplicated display planes. Horizontal rounding comparisons at
non-power-of-two widths permit **at most one count per bin** on single-row random frames;
the concentrated multi-row case would compound the original column error, not indicate
a different luma formula. No tolerance is used in the main differential suites.

Known answers cover red vector targets, a grey diagonal waveform, flat ramp histogram,
false-colour band edges, 8K DC counts, repeated-merge overflow, planar/interleaved identity,
actual scoped-thread row reduction, buffer address reuse, non-finite inputs and errors.
Benchmarks are ignored tests: full 3840×2160 deterministic noise, one warm-up and three
measured iterations, single-thread release build; allocation/input generation is excluded.

```sh
export CARGO_TARGET_DIR="$PWD/target-109"
export RUSTC="$(rustup which --toolchain stable rustc)"
export RUSTDOC="$(rustup which --toolchain stable rustdoc)"
rustup run stable cargo test -p craft-scopes
rustup run stable cargo clippy -p craft-scopes --all-targets -- -D warnings
rustup run stable cargo fmt -p craft-scopes --check
rustup run stable cargo doc -p craft-scopes --no-deps
rustup run stable cargo check -p craft-scopes --target wasm32-unknown-unknown
rustup run stable cargo test -p craft-scopes --release --test bench -- --ignored --nocapture --test-threads=1
```

Explicit compiler paths matter when Homebrew's `rustc` precedes rustup on PATH;
otherwise Cargo can miss rustup's installed wasm standard library even under `rustup run`.

Measured locally on 2026-11-03, Apple M6, rustup stable 1.99.0, release profile:

| Scope | ms / 4K frame |
| --- | ---: |
| Waveform RGB | 49.017 |
| Waveform luma | 33.862 |
| Waveform YC envelope | 58.959 |
| Waveform chroma | 49.735 |
| Parade RGB | 48.746 |
| Parade YUV | 48.982 |
| Parade RGB-white | 61.715 |
| Histogram | 42.375 |
| Vectorscope | 36.901 |
| False colour | 13.808 |

These are local microbenchmarks, not a speedup claim against either app. Trace grids
are 256×256 (histograms: 256 bins); all 8,294,400 source pixels are processed.
