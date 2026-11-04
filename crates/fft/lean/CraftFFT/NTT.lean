import CraftFFT.Generic
import Mathlib.Data.ZMod.Basic

/-!
# Number-theoretic transform over `ZMod 998244353`

`nttRoot m = 3^((p-1)/2^m)` (3 is a primitive root of `p = 998244353 = 119·2^23 + 1`),
tabulated as literals.  The table is *checked*, not trusted: `nttRoot_sq` verifies
`nttRoot (m+1)^2 = nttRoot m` and `nttRoot 1 = -1` by kernel evaluation, from which
`Roots (nttRoot m) m` follows by induction, and the generic theorem applies.
-/

namespace CraftFFT

/-- The NTT modulus. -/
abbrev P : ℕ := 998244353

/-- `3^((P-1)/2^m)` for `m = 0..13`. -/
def nttRoots : List (ZMod P) :=
  [1, 998244352, 911660635, 372528824, 929031873, 452798380, 922799308, 781712469,
    476477967, 166035806, 258648936, 584193783, 63912897, 350007156]

/-- Largest supported `m` (length `2^13 = 8192`). -/
def nttMax : ℕ := 13

def nttRoot (m : ℕ) : ZMod P := nttRoots.getD m 1

theorem nttRoot_one : nttRoot 1 = -1 := by decide +kernel

theorem nttRoot_sq : ∀ m : Fin nttMax, nttRoot (m + 1) ^ 2 = nttRoot m := by decide +kernel

theorem nttRoot_roots : ∀ m : ℕ, m ≤ nttMax → Roots (nttRoot m) m
  | 0, _ => trivial
  | m + 1, hm => by
    show nttRoot (m + 1) ^ (2 ^ m) = -1
    induction m with
    | zero => simpa using nttRoot_one
    | succ m ih =>
      have h := nttRoot_sq ⟨m + 1, by unfold nttMax at hm ⊢; omega⟩
      rw [pow_succ, mul_comm, pow_mul, h]
      exact ih (by omega)

/-- The NTT (radix-2 FFT over `ZMod 998244353`) equals the defining DFT sum. -/
theorem ntt_eq_dft (m : ℕ) (hm : m ≤ nttMax) (x : ℕ → ZMod P) (k : ℕ) (hk : k < 2 ^ m) :
    fft m (nttRoot m) x k = dft (nttRoot m) (2 ^ m) x k :=
  fft_eq_dft m _ x (nttRoot_roots m hm) k hk

end CraftFFT
