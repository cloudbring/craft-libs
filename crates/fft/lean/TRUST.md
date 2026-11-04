# craft-fft Lean development (ticket cloudbring/artcraft-algorithms#214): what is proved

Toolchain `leanprover/lean4:v4.24.0`, Mathlib `v4.24.0` (rev `f897ebcf72cd16f89ab4577d0c826cd14afaafc7`),
the same pin as `crates/blend/lean`. Run `./check.sh` (build, axiom audit, regenerate vectors and `cmp`).
`Audit.lean` prints the axioms of every theorem below; `check.sh` fails on anything outside
`propext`, `Classical.choice`, `Quot.sound`, or on any `sorry`. There is no `sorry` and no `native_decide`
(the table checks use `decide +kernel`, which adds no axiom).

## Proved (CraftFFT/Generic.lean, over any commutative ring `R`)

* `dft ω n x k = ∑_{j<n} x j * ω^(j*k)`.
* `fft m ω x`: the recursive radix-2 decimation-in-time FFT of length `2^m` (even/odd split,
  twiddle `ω^k`, butterfly `E_k ± ω^k O_k`, recursion with `ω^2`).
* `fft_eq_dft`: for every `m`, every `ω` with `ω^(2^(m-1)) = -1` (`Roots ω m`; no condition for
  `m = 0`) and every `k < 2^m`: `fft m ω x k = dft ω (2^m) x k`. Supporting lemmas `dft_split`,
  `dft_shift`, `sum_range_two_mul`, `Roots.sq`.

## Proved (CraftFFT/Complex.lean): agreement with Mathlib

* `omega m = exp(-2πi/2^m)`, `omega_roots : Roots (omega m) m`.
* `fft_eq_zmod_dft`: for `k < 2^m`, `fft m (omega m) x k = ZMod.dft (fun j : ZMod (2^m) => x j.val) k`
  where `ZMod.dft` (`𝓕`) is Mathlib's `Mathlib/Analysis/Fourier/ZMod.lean` DFT, defined by
  `∑ j, stdAddChar (-(j*k)) • Φ j`. Sign convention: forward, `exp(-2πi jk/N)`, no scaling.

## Proved (CraftFFT/NTT.lean): exact transform over `ZMod 998244353`

* `nttRoot m = 3^((p-1)/2^m)` is a literal table for `m ≤ 13`; `nttRoot_sq` (`root(m+1)^2 = root m`) and
  `nttRoot_one` (`root 1 = -1`) are checked by kernel evaluation, giving `nttRoot_roots`.
* `ntt_eq_dft`: `fft m (nttRoot m) x k = dft (nttRoot m) (2^m) x k` in `ZMod 998244353`, `m ≤ 13`.
  (Primality of `p` and primitivity of 3 are not used by the proof; only `ω^(N/2) = -1` is.)

## NOT proved

* The inverse transform and Parseval are not formalised here (Mathlib has `ZMod.dft` as a linear
  equivalence with inverse `𝓕⁻`; the Rust inverse check below is empirical only).
* Floating-point behaviour. The Lean statements are over exact rings. The `f32`/`f64` error bound is
  an empirical check, not a theorem.
* Complex reference vectors are computed, not proved. `Generate.lean` evaluates the defining DFT sum
  with `ω = exp(-2πi/N)` obtained by a half-angle chain `ω_{2N} = (ω_N + 1)/|ω_N + 1|` from the exact
  `ω_1 = 1, ω_2 = -1, ω_4 = -i`, in 60-digit fixed point with an integer square root, then rounds to
  20 decimals. That the chain converges to `exp(-2πi/N)` is standard trigonometry but is not
  machine-checked; the arithmetic error budget is below `1e-50` per twiddle (each half-angle step does
  not amplify error, `ω^t` is built by at most 4095 multiplications each rounded to `1e-60`), so the
  reference is dominated by the `5e-21` per-component output rounding, i.e. >= 4 orders of magnitude
  below `f64` roundoff (1.1e-16). Values were also cross-checked once against an independent Python
  double-precision DFT (agreement to the Python rounding level); that check is not part of CI.
* NTT rows come from the *proved* `fft` evaluated in the interpreter, and `Generate.lean` additionally
  throws if `fft` and `dft` disagree for `N <= 32`.

## Vectors (`../tests/vectors/lean-fft.csv`, 56 rows)

* 16 `ntt` rows: N = 2..256, cases `impulse1` and `random` (LCG). Larger N is impractical because the
  interpreter evaluates the closure-based `fft` with `ZMod` powers.
* 40 `complex` rows: N = 2..4096 (all 12 powers of two): `impulse1` (all N), `real` (N <= 512),
  `complex` (N <= 1024, dense), `sparse8` (N >= 16, eight nonzero samples). Inputs are multiples of 1/8
  in `[-8, 8]`, exact in `f32` and `f64`.
* The dense DFT is `O(N^2)` in the interpreter, hence sparse inputs for `N >= 2048`.

## Rust check (`../tests/lean_vectors.rs`)

`craft-fft` `ComplexFft` forward and inverse, `f32` and `f64`, all 40 complex rows, in the Euclidean norm:
`||err||_2 <= (C * max(log2 N, 1) + 1) * u * ||ref||_2`, `u = 2^-53` (`f64`) or `2^-24` (`f32`), `C = 2`;
inverse is applied to the rounded reference `X` and compared to `x` with `||x||_2`. Largest observed
ratio `err / (u * log2 N * ||ref||)` is about 0.34 (f64) and 0.31 (f32) (`cargo test -p craft-fft
--test lean_vectors -- --nocapture`), so `C = 2` has about 6x margin. `craft-fft` has no NTT: the 16 `ntt`
rows are checked only against an independent `u64` O(N^2) NTT in the test, i.e. they validate the Lean
model, not craft-fft. The private FFTs of gridcraft #23, filmcraft #105 and soundcraft #49 are not
checked here (they live in other repositories).

## Regenerating

```sh
export PATH="$HOME/.elan/bin:$PATH"
cd crates/fft/lean && lake exe cache get && lake build
lake env lean Generate.lean > ../tests/vectors/lean-fft.csv   # after a reviewed change only
./check.sh
```
