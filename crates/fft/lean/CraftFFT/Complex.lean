import CraftFFT.Generic
import Mathlib.Analysis.Fourier.ZMod
import Mathlib.Analysis.SpecialFunctions.Complex.CircleAddChar

/-!
# The complex FFT equals Mathlib's `ZMod.dft`

`omega m = exp(-2πi/2^m)`.  `fft_eq_zmod_dft` states that the radix-2 FFT with this root
equals Mathlib's `ZMod.dft` (the `𝓕` of `Mathlib/Analysis/Fourier/ZMod.lean`) on `ZMod (2^m)`.
-/

namespace CraftFFT

open Complex Finset
open scoped Real

/-- Forward root of unity `exp(-2πi/2^m)`. -/
noncomputable def omega (m : ℕ) : ℂ := exp (-(2 * π * I) / (2 : ℂ) ^ m)

theorem omega_pow (m n : ℕ) : omega m ^ n = exp (-(2 * π * I) * n / (2 : ℂ) ^ m) := by
  rw [omega, ← Complex.exp_nat_mul]
  congr 1
  ring

theorem omega_roots : ∀ m : ℕ, Roots (omega m) m
  | 0 => trivial
  | m + 1 => by
    show omega (m + 1) ^ (2 ^ m) = -1
    rw [omega_pow]
    have h : -(2 * (π : ℂ) * I) * ((2 ^ m : ℕ) : ℂ) / (2 : ℂ) ^ (m + 1) = -(π * I) := by
      push_cast
      field_simp
      ring
    rw [h, Complex.exp_neg, Complex.exp_pi_mul_I]
    simp

theorem fft_eq_dft_complex (m : ℕ) (x : ℕ → ℂ) (k : ℕ) (hk : k < 2 ^ m) :
    fft m (omega m) x k = dft (omega m) (2 ^ m) x k :=
  fft_eq_dft m _ x (omega_roots m) k hk

/-- **Main theorem**: the radix-2 FFT equals Mathlib's DFT on `ZMod (2^m)`. -/
theorem fft_eq_zmod_dft (m : ℕ) (x : ℕ → ℂ) (k : ℕ) (hk : k < 2 ^ m) :
    haveI : NeZero (2 ^ m) := ⟨by positivity⟩
    fft m (omega m) x k =
      ZMod.dft (fun j : ZMod (2 ^ m) => x j.val) (k : ZMod (2 ^ m)) := by
  haveI : NeZero (2 ^ m) := ⟨by positivity⟩
  rw [fft_eq_dft_complex m x k hk, ZMod.dft_apply, dft]
  symm
  refine Finset.sum_nbij' (fun j : ZMod (2 ^ m) => j.val) (fun n : ℕ => (n : ZMod (2 ^ m)))
    (fun j _ => by simp [ZMod.val_lt]) (fun n _ => by simp)
    (fun j _ => by simp) (fun n hn => ?_) (fun j _ => ?_)
  · have hn' : n < 2 ^ m := by simpa using hn
    simp [ZMod.val_natCast_of_lt hn']
  · have hv : ((j.val : ℕ) : ZMod (2 ^ m)) = j := ZMod.natCast_zmod_val j
    have h1 : (ZMod.stdAddChar (-(j * (k : ZMod (2 ^ m)))) : ℂ) = omega m ^ (j.val * k) := by
      have : (-(j * (k : ZMod (2 ^ m)))) = (((-((j.val * k : ℕ) : ℤ)) : ℤ) : ZMod (2 ^ m)) := by
        push_cast; rw [hv]
      rw [this, ZMod.stdAddChar_coe, omega_pow]
      congr 1
      push_cast
      ring
    rw [h1, smul_eq_mul, mul_comm]

end CraftFFT
