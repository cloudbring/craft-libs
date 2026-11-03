# craft-blend

Dependency-free W3C Compositing and Blending Level 1 separable blending and
straight-alpha source-over, with a colocated Lean 4 development. No unsafe code.

```rust
use craft_blend::{Channel, Mode, composite_f32};

let backdrop = Channel { color: 0.5, alpha: 0.5 };
let source = Channel { color: 0.8, alpha: 0.5 };
let result = composite_f32(Mode::Multiply, backdrop, source)?;
assert!((result.color - 17.0 / 30.0).abs() < 1e-7);
assert_eq!(result.alpha, 0.75);
# Ok::<(), craft_blend::Error>(())
```

For each straight colour channel, with `B` the selected blend mode:

```text
Co = (1 − αb) αs Cs + αb αs B(Cb, Cs) + (1 − αs) αb Cb
αo = αs + αb (1 − αs)
colour = Co / αo
```

Normal, Multiply, Screen, Overlay, Darken, Lighten, ColorDodge, ColorBurn,
HardLight, SoftLight, Difference and Exclusion use the W3C definitions. Dodge
checks `Cb = 0` before `Cs = 1`; Burn checks `Cb = 1` before `Cs = 0`.
SoftLight uses the cubic `D(Cb)` for `Cb ≤ 1/4`, and `sqrt(Cb)` otherwise.
Colour-space conversion, nonseparable modes, HDR, and opacity multiplication
are outside this crate. Apply the desired colour space and opacity before calling.

## Two necessary domain qualifications

1. **W3C SoftLight is not closed over ℚ.** For `Cb = 1/2, Cs = 1`, its result
   is `sqrt(1/2)`, which is irrational. An all-rational, total implementation of
   all twelve exact W3C modes is mathematically impossible. `blend_rational`
   returns `Error::Irrational` on an active nonsquare root, never an approximation.
   Lean proves the rational algebra and bounds of successful rational evaluations;
   it also proves the complete twelve-mode bounds over ℝ using `Real.sqrt`.
2. **A fully transparent straight colour is undefined.** The crate uses black
   when both input alphas are zero, matching Lean's totalized field division
   `0 / 0 = 0`. Both straight-colour identity laws require the surviving pixel
   to have nonzero alpha or canonical zero hidden colour. Without that condition,
   two differently coloured transparent pixels would require the same result
   to equal both colours. Positive-alpha identities are unconditional.

## APIs and failure behavior

- `blend_f32` / `composite_f32` use f32 arithmetic throughout. Compositing divides
  the alpha weights before multiplying colour, avoiding premultiplied underflow
  at tiny alpha. `normalized_weights_eq` proves this rearrangement in the exact
  algebra. Final colour is clamped to `[0,1]` to contain rounding overshoot.
  No epsilon denominator floor is used, including at subnormal alpha.
- `blend_rational` / `composite_rational` use reduced `Ratio` values, with i64
  numerator / positive i64 denominator and checked i128 intermediate arithmetic.
  Unrepresentable reduced intermediates return `Error::Overflow`; denominator
  zero returns `Error::ZeroDenominator`. This bounded representation is not ℚ
  for arbitrary-size computations. It does not silently round or saturate.
- Channels and alphas must be finite and in `[0, 1]`; other inputs return
  `Error::OutOfRange`. Inputs are never clamped. Hidden colours are validated
  too. Transparent branches skip unused blend evaluations.
- `Channel<T>` is one colour component plus alpha; repeat for RGB with shared alpha.

## Proof and test provenance

See [TRUST.md](TRUST.md) for theorem names, source transcription, exact domain,
the Rust/Lean correspondence, and remaining trust boundaries. See
[tests/vectors/README.md](tests/vectors/README.md) for deterministic regeneration.

Lean and Mathlib are pinned to **v4.24.0**, including the resolved Lake manifest.
The per-crate CI runs Cargo format, Clippy, tests, docs and wasm checks only.
**Lean is not run in CI yet**: CI checks the committed vectors, not their provenance.
