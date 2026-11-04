# Gaussian blur variance: craft-blur against each app's old blur

Claim C13 (tickets cloudbring/artcraft-algorithms#203, #215; epic #2; related #33, #52, #176): the
apps' Gaussian blurs under-blur by about `1/(3 sigma)`, and craft-blur is variance-exact.

## Verdict

* **craft-blur is variance-exact.** The effective variance (second central moment of the measured
  impulse response, 1-D and separable 2-D) equals the Lean-generated value for every kernel
  configuration over sigma = 0.5..64 to a relative error of 2e-8 (`crates/blur/tests/variance.rs`,
  tolerance 1e-6). Its variance error in the table below is 0.0% at every sigma.
* **The `~1/(3 sigma)` claim holds, with three corrections.**
  1. It is a statement about the *variance*: `sigma_eff^2 = sigma^2 (1 - 1/(3 sigma))`. The deficit in
     the standard deviation is half of it, about `1/(6 sigma)` (sigma = 8: variance -4.2%,
     `sigma_eff` 7.832, i.e. -2.1%).
  2. It is **exact for every integer sigma >= 1** (Lean theorem `CraftBlur.integer_sigma`: the widths are
     `2n-1, 2n-1, 2n+1`, the variance is exactly `n^2 - n/3`) and is otherwise bounded by
     `(sigma + 1)/(3 sigma^2)` (Lean theorem `CraftBlur.relative_error_le`). Between integers it can be
     smaller (sigma = 2.5: -4.0% against 1/(3 sigma) = 13.3%; sigma = 1.5: -11.1% against 22.2%) and at some sub-pixel
     sigmas the cascade *over*-blurs (sigma = 3/4: +18.5%); at sigma = 0.5 three boxes of width 1 are the
     identity (-100%).
  3. It applies to the **three-box cascade** only. The sampled-kernel regime that photocraft (sigma <= 4),
     effectcraft and materialcraft (sigma < 3) use for small sigma under-blurs by far less
     (-0.4% to -1.9%, from truncating at 3 sigma), and texturecraft's blur is a different, radius-based
     API that over-blurs.
* Six of the seven apps (all but texturecraft) run the same Kovesi three-box cascade over most of the
  range (the ideal width is `sqrt(4 sigma^2 + 1)`), so they share its deficit: -8.3% at sigma = 4,
  -4.2% at 8, -2.1% at 16, -1.0% at 32, -0.5% at 64. effectcraft and materialcraft switch to it at
  sigma = 3, photocraft above 4. The test asserts that each app's measured variance equals the Lean
  exact value for that cascade.

## Method

For each app the OLD blur's kernel- and width-selection code is vendored unchanged under
`crates/blur/tests/old/<app>.rs` (header: repo, SHA of `origin/main` at the time, path). The 1-D impulse
response of one axis is built from it (a box cascade is the convolution of its running means; a
sampled kernel is used as is; texturecraft's `box_line` is run on a fixed-point impulse). Every app blurs
separably, so the 2-D response is the product of two 1-D responses and each marginal has the 1-D
variance; craft-blur's 2-D response is measured directly in `variance.rs`. `sigma_eff` is the square
root of `sum(i^2 h_i)/sum(h_i) - mean^2`. The test `crates/blur/tests/old_blur.rs` pins these values in
`crates/blur/tests/vectors/blur-old-apps.csv`, asserts craft-blur within 1e-4, and for the box regimes
asserts equality with the Lean `box3` value and the proved bound.

| app | repo @ origin/main | path | regime used by the old blur |
|---|---|---|---|
| photocraft | storytold/photocraft @ 327c4cc82e73 | `crates/algo/src/blur.rs`, `fxutil.rs` | sampled kernel for radius <= 4 (exact-variance below 1), three boxes above 4; `fxutil` (internal smoothing): always three boxes, identity below 0.3 |
| lightcraft | storytold/lightcraft @ 06c1b9ae0775 | `crates/raster/src/blur.rs` | three boxes; identity for sigma <= 0.3 |
| effectcraft | storytold/effectcraft @ 7870b5916ae0 | `crates/raster/src/blur.rs` | sampled kernel (radius 3 sigma) below 3, three boxes from 3 |
| filmcraft | storytold/filmcraft @ eb59ec09daae | `crates/render/src/effects.rs` | three boxes; identity for sigma <= 0.3 |
| materialcraft | storytold/materialcraft @ 613d603b8fc5 | `crates/nodes/src/ops.rs` | sampled kernel below 3, three boxes from 3 |
| vectorcraft | storytold/vectorcraft @ 11862f61198c | `crates/effects/src/pixel.rs` | three boxes; identity below 0.2 |
| texturecraft | storytold/texturecraft @ 4edb8b86bc67 | `crates/compose/src/eval.rs` | three boxes of one integer radius `r`, `sigma_eff^2 = r(r+1)`; compared at `r = round(sigma)` |

The consumer branches that replace these blurs with craft-blur (effectcraft, photocraft, lightcraft,
vectorcraft per #2) are not on `origin/main` yet; the table is the pre-migration behaviour.

## Results

Requested sigma along the top. `craft-blur` is `Quality::Recursive` (sampled kernel below 8,
Young-van Vliet from 8).

### Effective sigma (square root of the impulse-response variance), per requested sigma

| app | 0.5 | 1 | 2 | 4 | 8 | 16 | 32 | 64 |
|---|---|---|---|---|---|---|---|---|
| photocraft | 0.500 | 0.998 | 1.988 | 3.963 | 7.832 | 15.832 | 31.833 | 63.833 |
| photocraft (fxutil) | 0.816 | 0.816 | 1.826 | 3.830 | 7.832 | 15.832 | 31.833 | 63.833 |
| lightcraft | 0.000 | 0.816 | 1.826 | 3.830 | 7.832 | 15.832 | 31.833 | 63.833 |
| effectcraft | 0.464 | 0.998 | 1.988 | 3.830 | 7.832 | 15.832 | 31.833 | 63.833 |
| filmcraft | 0.000 | 0.816 | 1.826 | 3.830 | 7.832 | 15.832 | 31.833 | 63.833 |
| materialcraft | 0.464 | 0.998 | 1.988 | 3.830 | 7.832 | 15.832 | 31.833 | 63.833 |
| vectorcraft | 0.000 | 0.816 | 1.826 | 3.830 | 7.832 | 15.832 | 31.833 | 63.833 |
| texturecraft (r=round(sigma)) | 1.414 | 1.414 | 2.449 | 4.472 | 8.485 | 16.492 | 32.496 | 64.498 |
| craft-blur | 0.500 | 1.000 | 2.000 | 4.000 | 8.000 | 16.000 | 32.000 | 64.000 |

### Variance error `sigma_eff^2 / sigma^2 - 1` (the quantity the ~1/(3 sigma) claim is about)

| app | 0.5 | 1 | 2 | 4 | 8 | 16 | 32 | 64 |
|---|---|---|---|---|---|---|---|---|
| photocraft | +0.0% | -0.4% | -1.2% | -1.9% | -4.2% | -2.1% | -1.0% | -0.5% |
| photocraft (fxutil) | +166.7% | -33.3% | -16.7% | -8.3% | -4.2% | -2.1% | -1.0% | -0.5% |
| lightcraft | -100.0% | -33.3% | -16.7% | -8.3% | -4.2% | -2.1% | -1.0% | -0.5% |
| effectcraft | -14.0% | -0.4% | -1.2% | -8.3% | -4.2% | -2.1% | -1.0% | -0.5% |
| filmcraft | -100.0% | -33.3% | -16.7% | -8.3% | -4.2% | -2.1% | -1.0% | -0.5% |
| materialcraft | -14.0% | -0.4% | -1.2% | -8.3% | -4.2% | -2.1% | -1.0% | -0.5% |
| vectorcraft | -100.0% | -33.3% | -16.7% | -8.3% | -4.2% | -2.1% | -1.0% | -0.5% |
| texturecraft (r=round(sigma)) | +700.0% | +100.0% | +50.0% | +25.0% | +12.5% | +6.3% | +3.1% | +1.6% |
| craft-blur | -0.0% | +0.0% | +0.0% | +0.0% | +0.0% | +0.0% | +0.0% | -0.0% |
| *-1/(3 sigma), for reference* | -66.7% | -33.3% | -16.7% | -8.3% | -4.2% | -2.1% | -1.0% | -0.5% |

Reading the table:

* Where an app runs the three-box cascade the deficit is `-1/(3 sigma)` exactly at integer sigma:
  -33.3% at 1 and -16.7% at 2 (lightcraft, filmcraft, vectorcraft; effectcraft, materialcraft and
  photocraft use a sampled kernel there), and for every app but texturecraft -8.3% at 4 (photocraft
  excepted, -1.9%), -4.2% at 8, -2.1% at 16, -1.0% at 32, -0.5% at 64.
* At sigma = 0.5 lightcraft, filmcraft and vectorcraft blur **nothing** (three boxes of width 1).
  effectcraft and materialcraft are at -14.0% (sampled kernel at 3-sigma truncation, radius 2).
  photocraft's Gaussian Blur filter is exact here (it calibrates the kernel below radius 1) but its
  `fxutil` smoothing over-blurs by +167% (its all-zero-radii patch applies one 3-wide box).
* photocraft is the best of the family: sampled kernel to 4 (-0.4%, -1.2%, -1.9%), boxes above; its
  documented "within 0.4%" holds only for the sampled regime, not at 8 (-4.2%).
* texturecraft's `r(r+1)` over-blurs at every sigma when `r = round(sigma)` (+100% at 1, +12.5% at 8),
  because its parameter is a box radius, not a standard deviation.

## Reproduce

```sh
export CARGO_TARGET_DIR=$PWD/target-o
cargo test -p craft-blur --test variance --test old_blur -- --nocapture
BLUR_BLESS=1 cargo test -p craft-blur --test old_blur   # only after reviewing a change
cd crates/blur/lean && ./check.sh                        # proofs, axiom audit, vectors byte-identical
```

What is and is not proved is in `crates/blur/lean/TRUST.md`. In short: widths, box and cascade
variances, the rounding error of the width rule, the exact integer-sigma deficit, and the
normalisation and finite-sum variance of the sampled kernel are proved; the recursive filter's
variance is an exact-rational closed form checked against the measured impulse response (not proved
for the infinite response), and the bisection that calibrates the sampled kernel is checked by
measurement, not proved.
