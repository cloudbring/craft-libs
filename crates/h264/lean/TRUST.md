# craft-h264 inverse-transform Lean development (ticket cloudbring/artcraft-algorithms#220): what is proved

Toolchain `leanprover/lean4:v4.24.0`, Mathlib `v4.24.0` (rev `f897ebcf72cd16f89ab4577d0c826cd14afaafc7`),
the same pin as `crates/fft/lean`. Run `./check.sh` (build, axiom audit, regenerate vectors and `cmp`).
Related: #30 (craft-jpeg IDCT), #7 (epic F7).

## Subject

`crates/h264/src/transform.rs`: `idct4_core` / `idct4_add` (4x4, spec 8.5.12.2) and `idct8_core` /
`idct8_add` / `idct8_1d` (8x8, 8.5.13.2): the add/subtract/shift butterfly, rows then columns, then
`(v + 32) >> 6`, on `i32` with Rust's `+ - >>` (arithmetic shift). `idct4_add`/`idct8_add` first clamp every
coefficient to `[-32768, 32767]`.

## Coefficient bound used

`[-2^15, 2^15 - 1]` = `[-32768, 32767]`. Two reasons it is the right domain: (1) the Rust code clamps to
exactly this range before the transform, so it is the domain of the shipped function, whatever the
bitstream says; (2) H.264 8.5.12.2/8.5.13.2 state, as a bitstream-conformance requirement, that the
transform inputs and intermediates lie in `[-2^(7+bitDepth), 2^(7+bitDepth) - 1]`, which is this range for
8-bit video (cited from memory of the 2005+ editions; the PDF was not re-read while writing this, so
treat the clause numbers as unverified). The general theorems take any `|d| <= M` with `M <= 2^21`
(`bitDepth = 14`, the largest H.264 allows), of which the Rust range is the case `M = 2^15`. Because the
Rust clamp makes the claim independent of conformance, the proofs cover non-conformant streams too.

## Proved, over `Int` (omega, interval arithmetic) — `CraftIDCT/Bounds.lean`, `Transform.lean`

* `bf4` / `bf8`: exact `Int` models of the 4-/8-point butterflies (`>>` = floor division), `idct4` /
  `idct8` = rows, columns, `rnd v = (v + 32) >>> 6`.
* `bf4_bound`, `bf8_bound`: inputs in `[-M, M]` give outputs in `[-4M, 4M]` (4-point) and
  `[-(8M+1), 8M+1]` (8-point).
* `bf4_inter_i32`, `bf8_inter_i32`: for `M <= 2^27` (4-pt) / `2^25` (8-pt) *every* value computed inside the
  butterfly (`e f g h`, the shifted terms, `a0..a7`, `b0..b7`, outputs) is an `i32`.
* `idct4_no_overflow`, `idct8_no_overflow` (`M <= 2^21`): every intermediate of the row pass, of the column
  pass, and `column output + 32` is an `i32`.
* `idct4_rust_bounds`, `idct8_rust_bounds`: on the Rust range the row pass stays within `+-131072` (4x4) /
  `+-262145` (8x8) and the column pass within `+-524288` / `+-2097161`; `i32` allows `2^31 = 2147483648`,
  so the margin is a factor 4096 (4x4) and 1024 (8x8).

## Proved, `BitVec 32` = `Int` (`Exact.lean`, `Transform.lean`)

`bf4b`/`bf8b`/`idct4b`/`idct8b` are generic over the width; `*v` are the `BitVec 32` instances (Rust `i32`:
wrapping `+ - neg`, arithmetic `sshiftRight`).

* `add_ok`, `sub_ok`, `neg_ok`, `sshr_ok`: per-operation lifts, each with its no-overflow side condition.
* `bf4v_toInt`, `bf8v_toInt`: the `BitVec 32` butterfly's `toInt` equals the `Int` butterfly's value when the
  inputs are in `[-M, M]` (side conditions discharged by `omega` from the interval bounds).
* `idct4v_toInt`, `idct8v_toInt` (`M <= 2^21`), and the Rust-range corollaries `idct4_rust_i32_exact`,
  `idct8_rust_i32_exact`: for every coefficient matrix in range, every entry of the `BitVec 32` transform
  has `toInt` equal to the `Int` model. Hence Rust's wrapping `i32` arithmetic never wraps on this domain,
  and the Rust output is the mathematical (floor-shift) model. Method: `omega` + `Int.bmod_eq_of_le`; no SAT.

## Proved, SAT cross-check with `bv_decide` — `SAT.lean`

`idct4_sat`, `idct8_sat` (and `*_flat`): for all 16 (64) `BitVec 32` coefficients in the signed range
`[-32768, 32767]`, the sign-extension to 64 bits of the 32-bit transform output equals the 64-bit transform
(same code at width 64, which cannot overflow on this box) — for every output position, so with all
intermediate wraps excluded in the output. It is an independent proof of the same no-wrap claim, with no
interval reasoning. Cost on an Apple-silicon laptop (single core): 4x4 about 20 s (16 SAT problems);
8x8 about 440 s (64 SAT problems of roughly 7 s each, 64 x 32-bit inputs); so `bv_decide` scales to the
8x8 transform, but linearly in the number of outputs and slow enough that the interval proof is the primary
one.

## Tightness (`Tight.lean`)

* `bf4_tight`: 4-point outputs on the Rust box lie in `[-114688, 114686]` (omega), attained
  (`bf4_tight_attained`, `decide +kernel`).  `bf8_tight`: `[-241664, 241660]`, attained
  (`bf8_tight_attained`; an exhaustive search of the 2^8 corners agrees).
* `idct4_attained`: the whole 4x4 transform reaches column-pass `401402` and `-401408`; proved upper
  bound `524288` is 1.31x that. For 8x8 the heuristic worst case (below) is `1782250` / `-1782272` against
  the proved `2097161` (1.18x). Those whole-transform extremes are *witnesses*, not proved maxima.
* `i16_would_overflow`: an `i16` implementation would overflow in the first pass (the row pass reaches
  `114686 > 32767`), so `i32` is not slack to be shaved; the spec's own 16-bit intermediate limit is a
  bitstream-conformance requirement, not a property of every clamped input.

## Axioms (`Audit.lean`, enforced by `check.sh`)

* Every theorem except the four `bv_decide` ones: only `propext`, `Classical.choice`, `Quot.sound`.
  (`decide +kernel` evaluates in the kernel and adds no axiom.) No `sorry`, no `native_decide` (grepped).
* `idct4_sat`, `idct4_sat_flat`, `idct8_sat`, `idct8_sat_flat`: additionally `Lean.ofReduceBool` and
  `Lean.trustCompiler`. `bv_decide` bit-blasts to CNF, runs the external CaDiCaL solver, and the UNSAT
  proof (LRAT) is checked by a verified checker whose evaluation is trusted via `Lean.ofReduceBool`
  (compiler/evaluator trusted for that one reflection step). `check.sh` allows these two axioms only for
  those four names; any other theorem that depends on them fails the audit. The primary no-overflow result
  (`idct4v_toInt`, `idct8v_toInt`, their corollaries) does not depend on them.

## NOT proved

* Correspondence of the Rust source to the Lean model is by hand transcription, guarded by tests (below),
  not machine-checked (no Aeneas/Verus/Kani here).
* Not modelled: `clip_u8` and the add to the prediction (covered only by the Rust test with bases
  0/77/128/255), the DC-only shortcut `idct_dc_add`, dequantisation (`scale4`, `scale8`, `luma_dc_dequant`,
  `chroma_dc_dequant_420`: these use `wrapping_mul`/shifts on unbounded levels and are *not* covered).
* The worst-case coefficient patterns for the 8x8 whole transform (and the 4x4 ones) are found by search, not
  proved extremal: 4x4 by exhaustive search over all 2^16 corners of the box; 8x8 by coordinate ascent from
  the sign pattern (the same ascent is checked equal to the exhaustive result for every 4x4 output).
  That corners dominate relies on each output being monotone in each input, which is clear from the
  formulas but not formalised. The proofs above do not depend on it.
* HEVC 4x4/8x8 (and larger) partial butterflies, craft-jpeg's IDCT and its error versus the real DCT, VP9/AV1
  transforms: not done in this change. The interval/bmod method here transfers directly (add/sub/shift).
* Spec clause numbers: see "Coefficient bound used".

## Vectors (`../tests/vectors/lean-idct.csv`) and the Rust check

`n,case,coeffs,pre,res`, 366 rows: 4x4 and 8x8; structural cases (zero, DC +-, all +-, checkerboard, odd
columns), per-output worst cases `maxpre_P`/`minpre_P` (the input pattern maximising / minimising the
column-pass output at position `P`), 48 LCG random full-range, 32 LCG random corners, 16 sparse.
`pre` is the pre-rounding column-pass output and `res = (pre + 32) >> 6` from the `Int` model.

`src/transform.rs` tests: `rust_transform_equals_lean_int_model` (every row through `idct4_core`/`idct8_core`
and through `idct4_add`/`idct8_add` with four prediction values, including clipping; asserts the CSV reaches
the Lean maxima 401408 / 1782272) and `corner_inputs_do_not_overflow_and_equal_i64_model` (all 2^16 4x4
corner matrices and 30000 random 8x8 corner matrices against an independent `i64` model, under the debug
profile where `i32` overflow panics).

## Regenerating

```sh
export PATH="$HOME/.elan/bin:$PATH"
cd crates/h264/lean && lake exe cache get && lake build
lake env lean Generate.lean > ../tests/vectors/lean-idct.csv   # after a reviewed change only (about 3 min)
./check.sh
```
