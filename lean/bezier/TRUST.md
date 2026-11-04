# craft-bezier Lean development (ticket cloudbring/artcraft-algorithms#216): what is proved

Lean-only project (there is no geometry crate in craft-libs). Toolchain `leanprover/lean4:v4.24.0`,
Mathlib `v4.24.0` (the same pin as `crates/blend/lean`, `crates/fft/lean`, `crates/blur/lean`).
Run `./check.sh` (build, axiom audit, regenerate vectors and `cmp`). `Audit.lean` prints the axioms
of every theorem below; `check.sh` fails on anything outside `propext`, `Classical.choice`,
`Quot.sound`, or on any `sorry`. No `native_decide`, no `sorry`.

## Model

`CraftBezier/Basic.lean`: control points are a sequence `P : ℕ → E` (only `P 0 … P n` are read) in an
`R`-module `E` (`R` a commutative ring). `bern n ν t = C(n,ν) t^ν (1-t)^(n-ν)` and
`eval_bernstein` proves it is the evaluation of Mathlib's `bernsteinPolynomial R n ν`.
`bez n P t = ∑_{ν ≤ n} bern n ν t • P ν` is the Bernstein form. `step`, `lvl` are de Casteljau
(`lvl k t P` = `k` rounds); `leftPt s P k = lvl k s P 0`, `rightPt n s P k = lvl (n-k) s P k` are the
two sub-polygons. The same computable definitions are run over `ℚ × ℚ` by `Generate.lean` and
proved about here, so the vectors are an evaluation of the proved objects.

## Proved (all `R = ℝ` or generic `R`; degree `n` arbitrary unless stated)

* `bern_succ_succ`, `bez_succ`, `bez_step`: the Bernstein recurrence and
  `B_{P0..Pn+1}(t) = (1-t) B_{P0..Pn}(t) + t B_{P1..Pn+1}(t)`.
* (c) `deCasteljau_eq_bez`: `lvl n t P 0 = bez n P t` (de Casteljau evaluation = Bernstein form).
* (d) `subdiv_left`: `bez n (leftPt s P) u = bez n P (s·u)`;
  `subdiv_right`: `bez n (rightPt n s P) u = bez n P (s + (1-s)·u)`. The two de Casteljau
  sub-curves reparametrise the original exactly (for every `u`, not just `[0,1]`).
* (a) `bern_nonneg`, `bern_sum` (from Mathlib's `bernsteinPolynomial.sum`), `bez_mem_convexHull`:
  for `t ∈ [0,1]`, `B(t) ∈ convexHull ℝ {P 0 … P n}` in any real vector space.
  `cubic_mem_convexHull`: the cubic case, `convexHull ℝ {a,b,c,d}`.
* (b) `bez_mem_bbox`: in `ℝ × ℝ`, `B(t)` lies in the axis-aligned box
  `[min xᵢ, max xᵢ] × [min yᵢ, max yᵢ]` of the control points (justifies the control-polygon bbox
  fast path).
* (e) `cubic_eq` (explicit cubic form), `xCurve_monotoneOn`: control values `x0 ≤ x1 ≤ x2 ≤ x3` give a
  monotone `x(t)` on `[0,1]`; `xCurve_strictMonoOn` strictly so when `x0 < x3`;
  `easing_unique_solution`: for `0 ≤ x1 ≤ x2 ≤ 1` (so `x0 = 0`, `x3 = 1`) and every `X ∈ [0,1]`
  there is exactly one `t ∈ [0,1]` with `x(t) = X` (existence by the intermediate value theorem on
  `xCurve_continuous`).

## NOT proved

* Floating-point behaviour: everything is over `ℚ`/`ℝ`; the Rust tests check agreement within an
  `f64` tolerance, not bit equality. De Casteljau in `f32`/`f64` rounding is out of scope.
* Tightness: the bbox and hull are conservative bounds; nothing is claimed about the exact curve
  extrema (those need the derivative roots).
* Easing with `x1`/`x2` outside `[0,1]` (non-monotone control sequence) and the Newton / bisection
  solver's iteration count or convergence: only existence and uniqueness of the solution.
* Rational Béziers, surfaces, arc-length or flatness bounds, and curve-fitting (Schneider) error.
* The y-coordinate of an easing curve is unconstrained (it may over/undershoot, as CSS allows).

## Vectors (`tests/vectors/lean-bezier.csv`)

Columns `name,s,p0x,p0y,…,p3x,p3y,bx,by,l0x,…,l3y,r0x,…,r3y`, every field an exact reduced
fraction `n/d`. 10 cubic control polygons (four CSS easings, quarter circle, loop, S-curve,
collinear, degenerate point, mixed signs) times 9 split parameters
`0, 1/8, 1/4, 1/3, 1/2, 2/3, 3/4, 7/8, 1`: 90 rows. `b` is `B(s)`, `l*` the left and `r*` the right
sub-polygon of the de Casteljau split at `s` (so `l3 = r0 = b`). `Generate.lean` aborts if the
de Casteljau point differs from the Bernstein form, or if either sub-curve fails to reproduce the
original at five `u` values (the proved theorems, re-checked on the data). An independent Python
`fractions` script reproduced all 90 rows (not part of CI).

## Regenerating

```sh
export PATH="$HOME/.elan/bin:$PATH"
cd lean/bezier && lake exe cache get && lake build
lake env lean Generate.lean > tests/vectors/lean-bezier.csv   # after a reviewed change only
./check.sh
```
