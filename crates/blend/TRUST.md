# Verification pilot #31: trust chain and limits

## Source and transcription

The specification is W3C **Compositing and Blending Level 1**, Candidate
Recommendation Draft, 21 March 2024, §§6, 9.1.4, and 10.1.1–10.1.12:
https://www.w3.org/TR/2024/CRD-compositing-1-20240321/

The concrete regression is cloudbring/artcraft-algorithms issue #31. The
reference algebra is photocraft `crates/color/src/blend.rs`, `composite`,
reviewed at `327c4cc82e737a3e0af7c147a477126d77525495`.
The texturecraft correction is `179eefd1`; its parent is
`4edb8b86bc67f58922d6f16a6489059a78b61a3e`.

The old `crates/compose/src/eval.rs` expression, read directly with
`git show 179eefd1^:crates/compose/src/eval.rs`, was:

```rust,ignore
if a <= 0.0 {
    return (v, cov);
}
let b = if cov > 0.0 { blend_chan(c, mode, v, p) } else { p };
let nc = a + cov * (1.0 - a);
(mix3(v, b, a / nc.max(1e-5)), nc)
```

`crates/core/src/blend.rs` defines each channel of `mix3(a, blended, w)` as
`a + (blended - a) * w`. `oldTexturecraft` transcribes that scalar expression,
including both branches and the denominator floor, over the mathematical field.
The decimal floor is interpreted as the exact rational `1/100000`, not the binary
f32 encoding of that decimal. The distinction cannot affect the ticket witness,
whose alpha `3/4` is greater than either floor.

The witness is `Cb = 1/2, αb = 1/2, Cs = 4/5, αs = 1/2`, Multiply. Both output
alphas are `3/4`. The spec's premultiplied colour is `17/40`, straight colour
`17/30`; the old expression is `13/30`, a difference of `2/15`.
This establishes an algebraic discrepancy, not a renderer or colour-management test.

## What Lean proves

All names below are in namespace `CraftBlend`. The generic definitions range
over an ordered field; instantiation at `ℚ` is the exact compositing model.
`UnitInterval value` means `0 ≤ value ∧ value ≤ 1`.

| Theorem | Contract |
| --- | --- |
| `product_bounds`, `screen_bounds`, `hardLight_bounds`, `softD_bounds`, `blend_bounds` | Component bounds, by the actual piecewise blend definitions, not post-hoc clamping. The generic root parameter is constrained by `Cb ≤ root ≤ 1`. |
| `alpha_bounds` | `αo ∈ [0,1]` for input alphas in `[0,1]`. |
| `premul_bounds` | `0 ≤ Co ≤ αo` for all three input colours in `[0,1]`. |
| `straight_bounds`, `normalized_weights_eq` | `Co / αo ∈ [0,1]`, including canonical zero at zero alpha; exact equivalence of the weight-normalized expression used by the f32 path. |
| `all_modes_bounds` | Alpha and straight-colour bounds for every mode, given the root bound. |
| `sqrtQ_sound`, `root_bounds` | A successful rational square-root evaluation is nonnegative, squares to the input, and supplies the root bound. |
| `rational_blend_bounds`, `rational_all_modes_bounds` | Bounds for every successful exact rational mode evaluation and its composite. There is no assertion that every SoftLight input has a rational result. |
| `cast_blend`, `rational_blend_agrees_real` | Every successful rational mode evaluation, cast into ℝ, equals the full real-valued spec definition, including the exact square-root branch. |
| `real_all_modes_bounds` | Unconditional twelve-mode bounds for all `[0,1]` inputs over ℝ, using the actual `Real.sqrt`, not an uninterpreted bounded function. |
| `transparent_source`, `transparent_backdrop` | Both colour and alpha identities; nonzero surviving alpha or canonical surviving hidden colour is required. |
| `transparent_source_premul`, `transparent_backdrop_premul` | Unconditional identity laws in premultiplied colour and alpha, even with arbitrary transparent hidden colour. |
| `opaque_normal` | Normal with source alpha one yields the source colour and alpha one. |
| `ticket_witness`, `old_ticket_witness`, `old_differs_from_spec` | The values `17/30`, `13/30`, `3/4`, and their discrepancy, discharged using `norm_num`. |
| `no_rational_sqrt_half` | There is no rational square root of `1/2`; the total all-ℚ SoftLight requirement cannot be satisfied. |

`blendQ` returns `Option ℚ`. Only SoftLight's active square-root branch needs
`sqrtQ`; the latter checks the candidate's sign and square rather than treating
integer square roots as exact unconditionally. The real model covers irrational
results. Field division by zero is totalized to zero; Rust explicitly chooses
canonical transparent black. These are deliberate, documented domain choices,
not missing proof cases or additional axioms.

`Audit.lean` is a default Lake build target and prints the axioms of the main
theorems. Its output must contain only Lean's standard foundations
(`propext`, `Classical.choice`, `Quot.sound`), never `sorryAx` or a custom axiom.

## What the Rust tests link

`Generate.lean` imports the proved definitions and executes them with `#eval`.
Its committed CSV has 4,075 exact-rational vectors: the complete Cartesian
product of twelve modes, seven backdrop colours, seven source colours, and
seven alpha pairs, minus 42 irrational SoftLight rows, plus the ticket witness.
The colours exercise the quarter/half branch boundaries, endpoint precedence,
the SoftLight polynomial, and the square root `sqrt(9/16) = 3/4`.
Alpha pairs exercise both transparent identities, both-transparent black,
opaque source, asymmetric partial coverage, and the old denominator-floor scale.
Every mode has at least 200 rows.

Rust reads the CSV without a parser dependency. It checks blend, straight colour,
alpha, and re-associated premultiplied colour exactly against Lean using `Ratio`.
Separately, it rounds each input to f32 and checks the f32 blend, straight colour,
and alpha against the rounded Lean values with a maximum allowed distance of
**2 ULP**. Inputs `4/5` and `1/1000000` are not binary-exact: those rows include
input-conversion error in their comparison, not just output rounding.
Expected f32 values are formed by converting the small exact reference numerator
and denominator to f64, dividing, then rounding to f32; that conversion is itself
not a formally verified correctly-rounded oracle.

Additional tests cover the regression explicitly, irrational SoftLight rejection,
transparent canonicalization, invisible irrational branches, invalid/non-finite
inputs, ratio normalization and representability failures, and subnormal alpha.

## What remains unproven

- The human transcription of W3C prose and historical source into Lean.
- Rust-to-Lean refinement on all inputs: this is **not** Aeneas extraction or a
  Rust verification tool. Tests establish agreement only on the finite vectors.
- The bounded i64/i128 rational implementation, overflow/error completeness,
  integer square-root implementation, and compilation to native / wasm code.
- A global floating-point error theorem, f32 input rounding, intermediate
  rounding / square-root accuracy, final colour clamping, and platform/compiler effects.
  The 2-ULP figure is a test threshold, never a universal theorem.
- Vector generation uses Lean's evaluator/compiler, not a kernel proof of each
  CSV byte. Rust's parser, fixture integrity, and generation command are part of
  the test trust chain. Regeneration compares the entire CSV byte-for-byte.
- Colour spaces, gamma, HDR, vector colour conversion, renderer integration,
  nonseparable modes and GPU implementations.

The proof trust base is Lean's kernel and its standard logical foundations plus
the pinned Mathlib proof terms and dependency artifacts. No project axioms,
`sorry`, `admit`, `native_decide`, external oracle, or Verus are used. The local
proof build checks the development; cached Mathlib dependencies are trusted as
the pinned toolchain's published artifacts, not independently rebuilt proofs.

Cargo-only per-crate CI does **not** run Lean yet. A passing CI run alone cannot
establish that a modified CSV was actually generated from the Lean sources.
Run the regeneration/check procedure before accepting any proof or vector change.
