# craft-cms sRGB transfer Lean development (ticket cloudbring/artcraft-algorithms#218): what is proved

Toolchain `leanprover/lean4:v4.24.0`, Mathlib `v4.24.0` (rev `f897ebcf72cd16f89ab4577d0c826cd14afaafc7`),
the same pin as `crates/blend/lean` and `crates/fft/lean`. `./check.sh` builds, audits axioms and regenerates
the vectors and `cmp`s them; `./tamper.sh` flips one digit of the CSV and requires `check.sh` to fail.
`Audit.lean` prints the axioms of 30 theorems; `check.sh` fails on anything outside `propext`,
`Classical.choice`, `Quot.sound`, or on any `sorry`. No `native_decide`, no `axiom`.

## Model (CraftSRGB/Defs.lean), over `ℝ` with `Real.rpow`

`decode x = if x ≤ 0.04045 then x/12.92 else ((x+0.055)/1.055)^2.4`
`encode y = if y ≤ 0.0031308 then 12.92*y else 1.055*y^(1/2.4) - 0.055`
(constants as exact rationals, exponents `12/5` and `5/12`). This is IEC 61966-2-1 as written.

## The knee mismatch, stated exactly

`0.04045/12.92 = 0.00313080495...` but the encode knee is `0.0031308`, and `12.92*0.0031308 = 0.040449936`,
not `0.04045`. The two pieces are not exactly continuous or exactly inverse. With
`L = 0.04045/12.92` (decode left limit), `R = ((0.04045+0.055)/1.055)^2.4` (decode right limit, not attained):

| quantity | proved enclosure |
|---|---|
| decode jump `R - L` (`Lval_lt_Rval`, `decode_jump_bounds`) | `(2.3295e-9, 2.3296e-9)`, positive: decode jumps UP |
| encode jump `encDip = 12.92*0.0031308 - er(0.0031308)` (`er_kneeB_lt_kval`, `encode_dip_bounds`) | `(2.8517e-8, 2.8518e-8)`: encode jumps DOWN |
| worst `|encode(decode x) - x|` on `(0.040449936, 0.04045]` (`edBound_bounds`) | `<= (9.2517e-8, 9.2518e-8)` proved bound |
| `0.04045 - encode(decode 0.04045)` (`roundtrip_error_at_knee`) | `(2.9591e-8, 2.9592e-8)` (value at the knee; numerically the sup) |
| worst `|decode(encode y) - y|` on `(0.0031308, R]` (`deBound_bounds`) | `<= (9.49e-9, 9.491e-9)` proved bound |

Consequences, all proved:

* `decode` is monotone on `[0,1]` (`decode_monotoneOn`) but discontinuous, and **not onto**: its image is exactly
  `[0, L] ∪ (R, 1]` (`decode_image`); `decode_not_surjective` exhibits `R` as unattained.
* `encode` is **not monotone** on `[0,1]` (`encode_not_monotone`: `encode(0.0031308) > encode(0.003130801)`).
  It is monotone on `[0, 0.0031308]` (`encode_monotoneOn_toe`), strictly increasing on `(0.0031308, 1]`
  (`encode_strictMonoOn_power`), and monotone up to `encDip` globally (`encode_almost_monotone`). Because the
  pieces overlap, `encode` IS onto `[0,1]` (`encode_image`). Both maps send `0 -> 0`, `1 -> 1`, `[0,1] -> [0,1]`
  (`decode_mapsTo`, `encode_mapsTo`, `*_zero`, `*_one`).
* `encode ∘ decode = id` exactly on `[0, 0.040449936]` (`encode_decode_toe`) and on `(0.04045, 1]`
  (`encode_decode_power`); on `(0.040449936, 0.04045]` it is off by at most `edBound` (`encode_decode_gap`).
* `decode ∘ encode = id` exactly on `[0, 0.0031308]` (`decode_encode_toe`) and on `(R, 1]`
  (`decode_encode_power`); on `(0.0031308, R]` it is off by at most `deBound` (`decode_encode_gap`).
* Every 8-bit code round-trips exactly over the reals: `encode(decode(c/255)) = c/255` for `c ≤ 255`
  (`eight_bit_roundtrip`: codes ≤ 10 lie in the toe, ≥ 11 above `0.04045`).

All enclosures are theorems (rational bounds via `x < y^(12/5) ↔ x^5 < y^12`), not numerics.

## Vectors (`../tests/vectors/srgb.csv`, 8,453 data rows)

`kind,index,input,lower_bound`: `decode8` (256 codes `c/255`), `decode_grid` and `encode_grid` (4,096 points
`i/4095`, `i = 0..4095`, decode and encode), `knee` (5 rows: `L`, `R`, `k = 12.92*0.0031308`, `Eb = er(0.0031308)`,
`EDa = encode(decode(0.04045))`). Values are printed to 33 decimals.

* **Proved vs computed.** `Real.rpow` is not computable, so `Generate.lean` finds candidates with an untrusted
  Newton integer root and accepts a value only if the decidable certificate `DecodeOK`/`RootOK`
  (CraftSRGB/Cert.lean) passes; `decodeOK_sound`/`rootOK_sound` prove that then the true real value lies in
  `[v, v + 1e-30)` (`+1.055e-30` for encode). A failed certificate throws, so a wrong root cannot reach the CSV.
  What is trusted is Lean's evaluation of the decidable check and the printing function `dec`
  (`floor(q*10^33)` formatting), not proved; the CSV is `cmp`-checked byte for byte in CI.
* Toe values are exact rationals printed to floor at 33 digits.

## Rust check (`../tests/lean_vectors.rs`, `cargo test -p craft-cms --test lean_vectors -- --nocapture`)

| path | measured max | stated tolerance |
|---|---|---|
| `srgb_trc()` `eval64` decode (all 4,352 decode rows) | 5.00 f64 ulp | <= 6 ulp |
| `eval_inverse64` encode (4,096 rows) | 3.00 f64 ulp | <= 4 ulp |
| `eval` / `eval_inverse` (f32 in/out) decode / encode | 2.36 / 0.97 f32 ulp | <= 3 ulp |
| 8-bit round trip `encode(decode(c))`, 256 codes | 1.1e-16 (f64), 3.0e-8 (f32); every code returns exactly `c` | rounds to `c` |
| `Transform` sRGB↔linear sRGB gray ramp, `eval` (f32) | 5.3e-8 decode, 4.8e-8 encode abs | < 1e-6 |
| `Transform::eval_fast` (table shaper) | 9.0e-8 decode, 1.6e-7 encode abs | < 1e-4 |

(The f32 ulp columns include rounding of the f32 input.) Findings about craft-cms:

* The `Curve` takes the power branch at `x >= 0.04045` (the knee point itself), IEC and the Lean model take the linear
  branch at `x <= 0.04045`; they differ only at that single point, by `R - L = 2.3295e-9` (asserted).
* `Curve::eval_inverse64` (`para_inverse`) is the exact inverse of its own decode: it switches branch at `R`
  (`~0.0031308073`), not at `0.0031308`. It therefore differs from the IEC encode on `(0.0031308, R]` by up to
  `encDip = 2.85e-8` (asserted: it returns `12.92*y = 0.040449936` where IEC gives `0.040449907`). Harmless
  at 8 bits (1 code = 3.9e-3) but a deliberate choice, now documented.
* No dedicated LUT, polynomial or u8 sRGB fast path exists in craft-cms. Fast paths are the table shaper
  (`eval_fast`, tested above) and the 3D CLUT used by `convert_u8` for non-matrix profiles. #218's "error bounds
  for the LUT/polynomial approximations the apps use" is therefore only partly done; the apps' own LUTs are not here.

## NOT proved

* Floating-point behaviour: all theorems are over `ℝ`; f32/f64 error is the empirical table above.
* The Rust code is not extracted from Lean or linked to it; correspondence is by vectors.
* The shaders are not run here (no GPU test harness); see the constant audit below.
* Sup of `|encode∘decode - id|` is proved only as `<= edBound` (monotonicity argument), not as the tighter
  numerically observed `2.9592e-8`; `deBound` likewise.

## Duplicated sRGB constants (report only; apps not edited)

Scan of 63 `*.wgsl` files across all app repositories (`rg` for `0.04045|0.03928|0.0031308|12.92|1.055|2.4|2.2|0.4545`).
Fourteen files contain sRGB constants (#42's "13 WGSL files" is 14 now). **No WGSL file uses a non-standard knee**:
all 14 use `0.04045` and `0.0031308` with `12.92`, `1.055`, `0.055`, `2.4`.

Standard (decode and/or encode): `pdfcraft/vendor/egui-wgpu/src/egui.wgsl`; `filmcraft/crates/gpu/src/{fx,composite}.wgsl`;
`texturecraft/crates/render/src/shaders/{trace_post,post,util,mesh}.wgsl`; `photocraft/crates/gpu/src/{camera_raw,compose}.wgsl`;
`lightcraft/crates/gpu/src/wgsl/finish.wgsl` (writes the `e > 0.04045` test inverted, equivalent);
`effectcraft/crates/gpu/src/shaders/{fx_lut,common,adv3d,advanced3d}.wgsl`.

Differing from the sRGB standard:
* `effectcraft/crates/gpu/src/shaders/kernels.wgsl:759-767`: pure `2.2` power for linear light (`EXPOSURE_LINEAR_GAMMA`);
  differs from sRGB by up to 8.5e-3 (about 2.2 of 255 codes) near 0.75.
* `effectcraft/crates/gpu/src/shaders/common.wgsl:475,519,534`: `2.4` pure power (Rec. 709/2020 / BT.1886 mode,
  intentionally not sRGB).
* `filmcraft/crates/gpu/src/fx.wgsl:319,326`: `1 + (v-1)/2.4` highlight extension above 1.0 (a limiter, not sRGB).
* Clamping differs: `filmcraft composite.wgsl:70,361,407`, `texturecraft post.wgsl:25`, `effectcraft common.wgsl:419`
  clamp negatives with `max(v, 0)`; `texturecraft util.wgsl:58`, `mesh.wgsl:211`, `lightcraft finish.wgsl:246`,
  `filmcraft fx.wgsl` (no `max` in the power branch inputs of `linear_to_srgb1`/`srgb_to_linear1`) and the
  others do not.

Rust files (not WGSL) using the superseded `0.03928` knee (differs from the standard by up to 7.6e-7 on `(0.03928, 0.04045]`):
`filmcraft/crates/ui-egui/tests/it/captions_font_ui.rs:121`, `pdfcraft/crates/audit/src/lib.rs:302`,
`lightcraft/crates/ui-egui/src/widgets/button.rs:113`, `craft-anything/crates/ui-egui/src/theme.rs:632`.
craft-libs itself has inline copies of the standard formula at `crates/pdf/tests/support/synthetic_icc.rs:37`,
`crates/update/src/dialog.rs:423` and `crates/compare/src/image.rs:184` (all `<= 0.04045`, standard decode).

## Regenerating

```sh
export PATH="$HOME/.elan/bin:$PATH"
cd crates/cms/lean && lake exe cache get && lake build
lake env lean Generate.lean > ../tests/vectors/srgb.csv   # after a reviewed change only
./check.sh && ./tamper.sh
```
