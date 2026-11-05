# craft-resample Lean development (ticket cloudbring/artcraft-algorithms#217): what is proved

Toolchain `leanprover/lean4:v4.24.0`, Mathlib `v4.24.0` (the same pin as `crates/blur/lean` and
`crates/fft/lean`). Run `./check.sh` (build, axiom audit, regenerate vectors and `cmp`).
`Audit.lean` prints the axioms of every theorem below; `check.sh` fails on anything outside
`propext`, `Classical.choice`, `Quot.sound`, or on any `sorry`. No `native_decide`, no `sorry`.

## Where the Rust lives

craft-libs had no resampler (`craft-audio-rt`, #41, is the realtime-callback crate). The audio
resamplers are in soundcraft (`crates/dsp/src/offline.rs` `Polyphase`; `crates/playback` sinc
resampler on `algo/46-sinc-resampler`) and filmcraft (`audio-dsp`). This ticket adds
`crates/resample` (`craft-resample`, the F8 crate proposed in #8) holding the polyphase kernel with
the same indexing as soundcraft's `Polyphase::run` (`pos = n * step`, whole part `pos / phases`,
phase `pos % phases`), and the Lean model next to it. The apps are NOT migrated by this change.

## Definitions (`CraftResample/Defs.lean`), over a commutative ring `R`

Sequences are `ℤ → R`; no finite-support hypothesis is needed (the FIR sum has `N` terms).
* `upsample L x m = if L ∣ m then x (m / L) else 0`; `fir h N u m = ∑_{k<N} h k * u (m - k)`;
  `downsample M v n = v (n * M)`; `direct = downsample M (fir h N (upsample L x))`.
* `tap h N k = h k` for `k < N`, else 0 (Rust pads the last phase with zeros).
* `polyOut h N L T x i p = ∑_{q<T} tap h N (p + q L) * x (i - q)` (the Rust inner loop,
  `coeffs[p*T + q] = h[p + qL]`).
* `step`/`accState`: the integer accumulator `p += M mod L; i += M div L; carry when p ≥ L`, started at
  `(first*M div L, first*M mod L)` as `Polyphase::process` does.
* `tapsPerPhase N L = ⌈N/L⌉`, `outLen len N L M = ⌈((len-1) L + N)/M⌉`.

## Proved (`CraftResample/Polyphase.lean`)

* `fir_eq_polyOut`: for `N ≤ L T`, `p < L`: `fir h N (↑L x) (i L + p) = polyOut h N L T x i p`.
* `accState_eq`: after `k` steps the accumulator is exactly `((first+k) M) divmod L` (no drift,
  built on `step_div_mod`).
* `streamOut_eq_direct`, `polyOut_eq_direct`, `rust_polyphase_eq_direct`: **polyphase output =
  `↓M (h * ↑L x)` for all inputs `x`**, all `L > 0`, all `M`, any output offset `first`, at the Rust
  table size `T = ⌈N/L⌉` (`le_mul_tapsPerPhase`).
* DC / normalisation: `polyOut_const` (constant window `c` gives `phaseSum * c`), `polyOut_dc`,
  `streamOut_dc`, `direct_dc` (every phase sums to 1 gives exactly `c`), `proto_sum_eq` and
  `proto_sum_of_phase_sum` (phases each summing to `s` make the prototype sum `L s`; `s = 1` is the
  zero-stuffing gain `L`).
* `direct_zero_after`: for input supported on `[0, len)` the output is 0 from `outLen` on, so
  Rust's `out_len` drops nothing.

## NOT proved / modelled

* Floating point: all statements are exact over a ring (`ℚ`, `ℝ` are instances). The Rust bound is
  empirical-but-derived (below), not a Lean theorem.
* The Kaiser design (`kaiser_prototype`: Bessel `I0`, `sin`) is transcendental and NOT modelled; the
  vectors use random rational taps injected through `Polyphase::from_prototype`. Its only checked
  property is the per-phase normalisation (Rust test; the Lean DC theorems then apply to it).
* The Rust loop's skipping of taps outside the input and `i64` casts: `x` is total over `ℤ` in Lean
  (zero outside the signal); Rust indexing equality is tested, not proved. `usize`/`i64` overflow of
  `first * M` is avoided by `u128` in Rust and untested beyond `first = 4e9`.
* soundcraft/filmcraft code itself: their weights are `K(p/L - k)` sampled from a continuous kernel
  and edges are clipped, which is the same `(i, p)` structure but is not imported here.
* Stopband / alias rejection (#202) is a measurement, not covered.

## Vectors (`../tests/vectors/lean-resample.csv`)

Columns `case,L,M,taps,len,out_len,x,h,y` (space-separated exact fractions). 22 rows:
`rand` (17): ratios 1/1, 2/1, 3/2, 2/3, 4/3, 3/4, 1/2, 4/2 and 6/4 (not coprime), 5/3, 160/147,
147/160, 80/49, short filters (2 to 8 taps per phase), some with `N` not a multiple of `L`;
inputs and taps are random `a/b` (`a ∈ [-16,16]`, `b ∈ {1,2,3,4,5,7,8}`, 64-bit LCG, seeds in
`Generate.lean`). `dc` (5): taps normalised to phase sum 1, constant input `3/7`.
`Generate.lean` throws if `direct`, `polyOut` at `(nM div L, nM mod L)`, and `streamOut` started at
0 and at 3 ever disagree, if the output past `outLen` is nonzero, if a `dc` interior output is not
exactly `3/7`, or if a `dc` prototype does not sum to `L`.

## Rust checks (`../tests/lean_vectors.rs`)

`f64` and `f32` outputs vs the exact rationals with
`|ŷ - y| <= (T + 4) * u * ∑_q |h[p+qL]| |x[i-q]|`, `u = 2^-53` / `2^-24`. Observed (22 rows):
f64 worst abs error `1.4e-14`, worst error/bound `0.285`; f32 worst abs error `5.8e-6`, worst
error/bound `0.274`. Also: literal direct form agrees and ends at `out_len`; streaming from an
offset is bit-identical to the whole run; `dc` rows are 1 in the interior; Kaiser phases sum to 1;
a 300k-output run at 160/147 matches a fresh closed-form start at every output (no drift), and at
offset 4e9. Mutation check: `p >= L` changed to `p > L` fails 7 of 8 tests.

## Regenerating

```sh
export PATH="$HOME/.elan/bin:$PATH"
cd crates/resample/lean && lake exe cache get && lake build
lake env lean Generate.lean > ../tests/vectors/lean-resample.csv   # after a reviewed change only
./check.sh
```
