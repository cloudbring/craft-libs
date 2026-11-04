# craft-blur Lean development (tickets cloudbring/artcraft-algorithms#215 and #203): what is proved

Toolchain `leanprover/lean4:v4.24.0`, Mathlib `v4.24.0` (the same pin as `crates/blend/lean` and
`crates/fft/lean`). Run `./check.sh` (build, axiom audit, regenerate vectors and `cmp`).
`Audit.lean` prints the axioms of every theorem below; `check.sh` fails on anything outside
`propext`, `Classical.choice`, `Quot.sound`, or on any `sorry`. No `native_decide`, no `sorry`.

## Proved

`CraftBlur/Dist.lean` (exact, over `ℚ`; a kernel is a finite list of `(offset, weight)`):
* `mom0_conv`, `mom1_conv`, `mom2_conv`: moments of a convolution; `var_conv`: variance is additive
  for unit-mass zero-mean kernels.
* `var_conv_reflect`: a kernel convolved with its mirror image has twice its variance (the
  forward + backward recursive filter).
* `box_mom0`, `box_mom1`, `box_mom2`, `box_var`: the box of odd width `w = 2r+1` has mass 1, mean 0
  and variance `(w² - 1)/12`.
* `cascade_moments`, `cascadeW_var`: a cascade of odd boxes has mass 1, mean 0 and variance
  `∑ (w² - 1)/12`.

`CraftBlur/Widths.lean` (the rule of `craft_blur::box_widths`: `l = ⌊√(4σ²+1)⌋` made odd, `m` boxes of
width `l` and `3 - m` of `l + 2` with `m = round(m*)`, `m* = (12σ² - 3l² - 12l - 9)/(-4l - 4)`):
* `lower_sq_le`, `lt_upper_sq`: `l² ≤ 4σ²+1 < (l+2)²`.
* `mStar_bounds`, `count_le_three`: `0 < m* ≤ 3`, so the count of lower-width boxes is in `0..3`.
* `widths_odd`, `widthVar_widths`: every width is odd and the variance is
  `(3(l²-1) + (3-m)(4l+4))/12`.
* `variance_error_le`: `|V - σ²| ≤ (l+1)/6` (rounding `m*` moves the variance by at most half a
  box-step). `relative_error_le`: for `σ > 0`, `|V/σ² - 1| ≤ (σ+1)/(3σ²) = (1/(3σ))(1 + 1/σ)`.
* `integer_sigma`: for every integer `σ = n ≥ 1` the widths are exactly `2n-1, 2n-1, 2n+1`, the
  variance is exactly `n² - n/3`, and the relative deficit is exactly `-1/(3n)`. This is the
  `~1/(3σ)` under-blur of the integer-box Gaussian, as a theorem.

`CraftBlur/Gaussian.lean` (reals; the sampled kernel of `craft_blur::gaussian_kernel`):
* `weights_sum_one`, `mean_zero`: the normalised weights `exp(-i²/(2s²))/Z` on `-h..h` sum to 1 and
  have mean 0. `varianceSampled_eq`: variance `= ∑ i² e^{-i²/2s²} / ∑ e^{-i²/2s²}` (finite sum).
  `varianceSampled_nonneg`, `varianceSampled_le`: `0 ≤ var ≤ h²`.
* `pdf_eq_gw`, `continuous_second_moment`: the weights are Mathlib's Gaussian density
  `gaussianPDFReal 0 (s²)` at the integers (up to the normalising constant), and that density has
  second moment `s²` (from Mathlib's `variance_id_gaussianReal` and the density form of the measure).

## Modelled, not proved

* `CraftBlur/Recursive.lean`: exact rational model of `Recursive::new` (decimal constants, the
  starting polynomial `q₀`, five Newton steps, feedback coefficients and gain) and a closed form for
  the variance of `y[n] = g x[n] + f₁y[n-1] + f₂y[n-2] + f₃y[n-3]`, `-D''/D + D'²/D² - D'/D` at
  `x = 1`, `D(x) = 1 - f₁x - f₂x² - f₃x³`, doubled for the forward + backward passes. Evaluated at
  the grid this gives `σ²` to better than `1e-40` and a two-pass DC gain of exactly 1 to that
  precision. That the infinite impulse response has that variance is a power-series argument that is
  NOT machine-checked here; the Rust test measures the actual impulse response against it
  (observed agreement `1e-8`). `var_conv_reflect` covers the doubling only for finite kernels.

## NOT proved

* That the bisection in `gaussian_kernel` finds a width whose truncated sampled kernel has variance
  exactly `σ²` (existence by the intermediate value theorem, and 48 bisection steps). The CSV value
  for kernel `sampled` is therefore the calibration TARGET `σ²`, not something derived from `exp`;
  the Rust test measures the real `f32` kernel against it (observed agreement `2e-8`).
* Convergence of the finite sampled variance to `σ²` as the support grows (Riemann sum), and the
  truncation error at `h = ⌈3σ⌉`.
* Floating-point behaviour: all statements are over `ℚ`/`ℝ`; the Rust tolerance (`1e-6`) is
  empirical. The Lean model uses the decimal constants as exact rationals; Rust uses their `f64`
  values.
* Boundary handling (Triggs–Sdika, edge policies): everything here is the response to an impulse far
  from the edges.
* `⌊√(4σ²+1)⌋` in Rust is computed in `f64`; Lean uses the exact integer square root. They agree on
  the grid (`tests/variance.rs::box_widths_and_acceptance_match_lean`); a value of `σ` where `f64`
  rounding crosses an integer boundary would differ.

## Vectors (`../tests/vectors/lean-blur.csv`)

Columns `kernel,sigma,widths,accepted,var_rat,var_dec,dc_dec`; `sigma` is an exact fraction (all
grid values are exact in `f32`): `1/2, 3/4, 1, 3/2, 2, 5/2, 3, 4, 6, 8, 12, 16, 24, 32, 48, 64`.
* `sampled` (16 rows): calibration target `σ²`.
* `box3` (16 rows): exact widths and the exact variance `∑(w²-1)/12` (`var_rat`); `accepted` is
  Lean's `|V/σ² - 1| ≤ 1/(3σ)`, i.e. whether `Gaussian::new(σ, BoxApprox{3})` succeeds.
* `yvv` (11 rows, `σ ≥ 5/2`): `var_dec` is the closed-form effective variance (decimal, 30 places,
  rounded half up from the exact rational), `dc_dec` the two-pass DC gain.
`Generate.lean` also throws if the explicit convolution of the boxes (σ ≤ 8) disagrees with
`widthVar`, or if `variance_error_le` is violated. The same rows were independently produced
by a 40-line Python `fractions` script (byte-identical output); that script is not part of CI.

## Rust checks

`../tests/variance.rs`: 1-D (`filter_rows`) and separable 2-D (`apply`, marginals and covariance)
effective variance of the real impulse responses of `Quality::Exact`, `RecursiveRaw`, `Recursive`
(σ ≥ 8) and `BoxApprox{3}`, equal to the CSV within `1e-6` relative; widths and accept/reject equal
to Lean. `../tests/old_blur.rs` (#203): the same measurement for each app's old blur, vendored under
`../tests/old/`; where the old blur is the three-box cascade its variance equals the Lean `box3`
value exactly and respects `relative_error_le`.

## Regenerating

```sh
export PATH="$HOME/.elan/bin:$PATH"
cd crates/blur/lean && lake exe cache get && lake build
lake env lean Generate.lean > ../tests/vectors/lean-blur.csv   # after a reviewed change only
./check.sh
```
