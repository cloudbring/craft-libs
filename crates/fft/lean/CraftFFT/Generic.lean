import Mathlib.Algebra.BigOperators.Ring.Finset
import Mathlib.Tactic.Ring

/-!
# Radix-2 decimation-in-time FFT over any commutative ring

Signals are functions `ℕ → R`; only indices below the transform length matter.
`dft ω n x k = ∑_{j<n} x j * ω^(j*k)`.  The only hypothesis on `ω` is the radix-2
condition `ω^(N/2) = -1` (`Roots`), so the theorem covers `ℂ` and `ZMod p` alike.
-/

namespace CraftFFT

open Finset

variable {R : Type*} [CommRing R]

/-- Defining sum of the length-`n` DFT with root `ω`. -/
def dft (ω : R) (n : ℕ) (x : ℕ → R) (k : ℕ) : R :=
  ∑ j ∈ range n, x j * ω ^ (j * k)

/-- Recursive radix-2 decimation-in-time FFT of length `2^m`. -/
def fft : (m : ℕ) → R → (ℕ → R) → (ℕ → R)
  | 0, _, x => fun _ => x 0
  | m + 1, ω, x =>
    let e := fft m (ω ^ 2) (fun j => x (2 * j))
    let o := fft m (ω ^ 2) (fun j => x (2 * j + 1))
    fun k =>
      if k < 2 ^ m then e k + ω ^ k * o k
      else e (k - 2 ^ m) - ω ^ (k - 2 ^ m) * o (k - 2 ^ m)

/-- `ω` is a root usable for a length-`2^m` transform: `ω^(2^(m-1)) = -1` when `m ≥ 1`. -/
def Roots (ω : R) : ℕ → Prop
  | 0 => True
  | m + 1 => ω ^ (2 ^ m) = -1

theorem Roots.sq {ω : R} : ∀ {m : ℕ}, Roots ω (m + 1) → Roots (ω ^ 2) m
  | 0, _ => trivial
  | m + 1, h => by
    show (ω ^ 2) ^ (2 ^ m) = -1
    rw [← pow_mul, show 2 * 2 ^ m = 2 ^ (m + 1) by ring]
    exact h

theorem sum_range_two_mul (f : ℕ → R) (n : ℕ) :
    ∑ j ∈ range (2 * n), f j = ∑ i ∈ range n, (f (2 * i) + f (2 * i + 1)) := by
  induction n with
  | zero => simp
  | succ n ih =>
    rw [show 2 * (n + 1) = 2 * n + 1 + 1 by ring, sum_range_succ, sum_range_succ, ih,
      sum_range_succ]
    ring

/-- Even/odd split of the DFT, valid for every `k`. -/
theorem dft_split (ω : R) (n : ℕ) (x : ℕ → R) (k : ℕ) :
    dft ω (2 * n) x k =
      dft (ω ^ 2) n (fun j => x (2 * j)) k + ω ^ k * dft (ω ^ 2) n (fun j => x (2 * j + 1)) k := by
  unfold dft
  rw [sum_range_two_mul, mul_sum, ← sum_add_distrib]
  refine sum_congr rfl fun i _ => ?_
  have h1 : ω ^ (2 * i * k) = (ω ^ 2) ^ (i * k) := by rw [← pow_mul, mul_assoc]
  have h2 : ω ^ ((2 * i + 1) * k) = ω ^ k * (ω ^ 2) ^ (i * k) := by
    rw [← pow_mul, ← pow_add]; congr 1; ring
  rw [h1, h2]; ring

/-- Periodicity of the half-length DFT when `(ω^2)^n = 1`. -/
theorem dft_shift (ω : R) (n : ℕ) (hn : (ω ^ 2) ^ n = 1) (y : ℕ → R) (k : ℕ) :
    dft (ω ^ 2) n y (k + n) = dft (ω ^ 2) n y k := by
  unfold dft
  refine sum_congr rfl fun j _ => ?_
  rw [mul_add, pow_add, mul_comm j n, pow_mul (ω ^ 2) n j, hn, one_pow, mul_one]

/-- **Radix-2 Cooley–Tukey correctness**: the recursive FFT equals the DFT. -/
theorem fft_eq_dft : ∀ (m : ℕ) (ω : R) (x : ℕ → R), Roots ω m →
    ∀ k, k < 2 ^ m → fft m ω x k = dft ω (2 ^ m) x k
  | 0, ω, x, _, k, hk => by
    have : k = 0 := by simpa using hk
    subst this
    simp [fft, dft]
  | m + 1, ω, x, hr, k, hk => by
    have hω : ω ^ (2 ^ m) = -1 := hr
    have hsq : (ω ^ 2) ^ (2 ^ m) = 1 := by
      rw [← pow_mul, mul_comm, pow_mul, hω]; simp
    have ihE := fft_eq_dft m (ω ^ 2) (fun j => x (2 * j)) hr.sq
    have ihO := fft_eq_dft m (ω ^ 2) (fun j => x (2 * j + 1)) hr.sq
    rw [pow_succ, mul_comm, dft_split]
    simp only [fft]
    split_ifs with h
    · rw [ihE k h, ihO k h]
    · have hk' : k - 2 ^ m < 2 ^ m := by
        have : 2 ^ (m + 1) = 2 ^ m + 2 ^ m := by ring
        omega
      have hk2 : k = (k - 2 ^ m) + 2 ^ m := by omega
      conv_rhs => rw [hk2]
      rw [dft_shift ω _ hsq, dft_shift ω _ hsq, ihE _ hk', ihO _ hk', pow_add, hω]
      ring

end CraftFFT
