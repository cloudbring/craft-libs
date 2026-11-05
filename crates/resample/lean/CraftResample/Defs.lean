import Mathlib

/-!
# Upsample, FIR, downsample, and the polyphase form of `craft-resample`

Sequences are `ℤ → R` over a commutative ring `R`; no finite-support hypothesis is needed because
the FIR sum is finite. The prototype is `h : ℕ → R` with `N` taps (`tap h N k = h k` for `k < N`,
else `0`), the same convention as the Rust `from_prototype`, which pads the last phase with zeros.
-/

namespace CraftResample

variable {R : Type*} [CommRing R]

/-- Zero-stuffing by `L`: `u[m] = x[m / L]` if `L ∣ m`, else `0`. -/
def upsample (L : ℕ) (x : ℤ → R) : ℤ → R :=
  fun m => if (L : ℤ) ∣ m then x (m / L) else 0

/-- Prototype taps: `h k` for `k < N`, zero beyond. -/
def tap (h : ℕ → R) (N k : ℕ) : R := if k < N then h k else 0

/-- Causal FIR of length `N`: `(h * u)[m] = ∑_{k<N} h[k] u[m-k]`. -/
def fir (h : ℕ → R) (N : ℕ) (u : ℤ → R) (m : ℤ) : R :=
  ∑ k ∈ Finset.range N, h k * u (m - k)

/-- Keep every `M`-th sample: `(↓M v)[n] = v[n M]`. -/
def downsample (M : ℕ) (v : ℤ → R) (n : ℕ) : R := v ((n * M : ℕ) : ℤ)

/-- The direct form: `↓M (h * ↑L x)`. -/
def direct (h : ℕ → R) (N L M : ℕ) (x : ℤ → R) (n : ℕ) : R :=
  downsample M (fir h N (upsample L x)) n

/-- One output of the polyphase form at whole position `i`, phase `p`:
`∑_{q<T} h[p + q L] x[i - q]` (Rust `Polyphase::process` inner loop; `T` = taps per phase). -/
def polyOut (h : ℕ → R) (N L T : ℕ) (x : ℤ → R) (i : ℤ) (p : ℕ) : R :=
  ∑ q ∈ Finset.range T, tap h N (p + q * L) * x (i - q)

/-- The sum of phase `p`'s coefficients. -/
def phaseSum (h : ℕ → R) (N L T p : ℕ) : R :=
  ∑ q ∈ Finset.range T, tap h N (p + q * L)

/-- Taps per phase as the Rust computes it: `⌈N / L⌉`. -/
def tapsPerPhase (N L : ℕ) : ℕ := (N + L - 1) / L

/-- One step of the integer position accumulator `(i, p)`: `p += M mod L`, `i += M div L`,
carry one into `i` when `p` reaches `L` (Rust `process` loop tail). -/
def step (L M : ℕ) (s : ℤ × ℕ) : ℤ × ℕ :=
  if s.2 + M % L ≥ L then (s.1 + (M / L : ℕ) + 1, s.2 + M % L - L)
  else (s.1 + (M / L : ℕ), s.2 + M % L)

/-- Accumulator state after `k` steps from output index `first`: Rust initialises it at
`(first*M div L, first*M mod L)`. -/
def accState (L M first k : ℕ) : ℤ × ℕ :=
  (step L M)^[k] (((first * M / L : ℕ) : ℤ), first * M % L)

/-- The streamed polyphase output `first + k`, exactly as `Polyphase::process` computes it. -/
def streamOut (h : ℕ → R) (N L M T : ℕ) (x : ℤ → R) (first k : ℕ) : R :=
  polyOut h N L T x (accState L M first k).1 (accState L M first k).2

/-- Rust `out_len`: `⌈((len-1) L + N) / M⌉`. -/
def outLen (len N L M : ℕ) : ℕ := ((len - 1) * L + N + M - 1) / M

end CraftResample
