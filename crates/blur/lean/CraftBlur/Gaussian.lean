import Mathlib

/-!
# Truncated sampled Gaussian kernel (`craft_blur::gaussian_kernel`)

Weights `exp(-i²/(2 s²))` on offsets `-h..h` normalised to sum 1.  `craft-blur` chooses `s` by
bisection so that the variance below equals `σ²` (`h = ⌈3σ⌉`); this file proves the finite-sum
facts that statement relies on.  The existence of `s` (intermediate value theorem on the
variance) is NOT proved here.
-/

namespace CraftBlur

open Finset

noncomputable def gw (s : ℝ) (i : ℤ) : ℝ := Real.exp (-((i : ℝ) ^ 2) / (2 * s ^ 2))

def supp (h : ℕ) : Finset ℤ := Finset.Icc (-(h : ℤ)) h

noncomputable def Z (s : ℝ) (h : ℕ) : ℝ := ∑ i ∈ supp h, gw s i

noncomputable def weight (s : ℝ) (h : ℕ) (i : ℤ) : ℝ := gw s i / Z s h

lemma zero_mem_supp (h : ℕ) : (0 : ℤ) ∈ supp h := by simp [supp]

lemma Z_pos (s : ℝ) (h : ℕ) : 0 < Z s h := by
  unfold Z
  apply Finset.sum_pos
  · intro i _; exact Real.exp_pos _
  · exact ⟨0, zero_mem_supp h⟩

/-- Normalised weights sum to 1. -/
theorem weights_sum_one (s : ℝ) (h : ℕ) : ∑ i ∈ supp h, weight s h i = 1 := by
  unfold weight
  rw [← Finset.sum_div]
  exact div_self (Z_pos s h).ne'

lemma weight_pos (s : ℝ) (h : ℕ) (i : ℤ) : 0 < weight s h i :=
  div_pos (Real.exp_pos _) (Z_pos s h)

lemma sum_neg_supp (h : ℕ) (f : ℤ → ℝ) : ∑ i ∈ supp h, f (-i) = ∑ i ∈ supp h, f i := by
  apply Finset.sum_nbij' (fun i => -i) (fun i => -i)
  · intro i hi; simp only [supp, Finset.mem_Icc] at *; omega
  · intro i hi; simp only [supp, Finset.mem_Icc] at *; omega
  · intro i _; simp
  · intro i _; simp
  · intro i _; rfl

lemma weight_neg (s : ℝ) (h : ℕ) (i : ℤ) : weight s h (-i) = weight s h i := by
  simp [weight, gw]

/-- The kernel is symmetric, so its mean is 0. -/
theorem mean_zero (s : ℝ) (h : ℕ) : ∑ i ∈ supp h, (i : ℝ) * weight s h i = 0 := by
  have := sum_neg_supp h (fun i => (i : ℝ) * weight s h i)
  simp only [Int.cast_neg, neg_mul, weight_neg, Finset.sum_neg_distrib] at this
  linarith

/-- Variance of the normalised sampled kernel, as a finite sum (mean is 0). -/
noncomputable def varianceSampled (s : ℝ) (h : ℕ) : ℝ := ∑ i ∈ supp h, (i : ℝ) ^ 2 * weight s h i

theorem varianceSampled_eq (s : ℝ) (h : ℕ) :
    varianceSampled s h = (∑ i ∈ supp h, (i : ℝ) ^ 2 * gw s i) / Z s h := by
  unfold varianceSampled weight
  rw [Finset.sum_div]
  apply Finset.sum_congr rfl
  intro i _; ring

theorem varianceSampled_nonneg (s : ℝ) (h : ℕ) : 0 ≤ varianceSampled s h :=
  Finset.sum_nonneg fun i _ => mul_nonneg (sq_nonneg _) (weight_pos s h i).le

/-- Truncation bound: the variance never exceeds `h²`. -/
theorem varianceSampled_le (s : ℝ) (h : ℕ) : varianceSampled s h ≤ (h : ℝ) ^ 2 := by
  unfold varianceSampled
  calc ∑ i ∈ supp h, (i : ℝ) ^ 2 * weight s h i
      ≤ ∑ i ∈ supp h, (h : ℝ) ^ 2 * weight s h i := by
        apply Finset.sum_le_sum
        intro i hi
        simp only [supp, Finset.mem_Icc] at hi
        have : (i : ℝ) ^ 2 ≤ (h : ℝ) ^ 2 := by
          have a : (-(h : ℝ)) ≤ i := by exact_mod_cast hi.1
          have b : (i : ℝ) ≤ h := by exact_mod_cast hi.2
          nlinarith
        exact mul_le_mul_of_nonneg_right this (weight_pos s h i).le
    _ = (h : ℝ) ^ 2 := by rw [← Finset.mul_sum, weights_sum_one, mul_one]

/-! ### The continuous limit (Mathlib's Gaussian)

The sampled weights are `gw s i` normalised, i.e. the Gaussian density `gaussianPDFReal 0 (s²)`
evaluated at integers (the constant cancels in the normalisation).  Mathlib proves that this
density has second moment `s²`; that is the `σ²` the sampled kernel approximates.  We do NOT prove
the Riemann-sum convergence of `varianceSampled s h` to it as `h → ∞`. -/

open ProbabilityTheory NNReal in
/-- The density's shape at a point is `gw`: `gaussianPDFReal 0 (s²) i = (√(2π s²))⁻¹ * gw s i`. -/
theorem pdf_eq_gw (s : ℝ≥0) (i : ℤ) :
    gaussianPDFReal 0 (s ^ 2) (i : ℝ) = (Real.sqrt (2 * Real.pi * ((s ^ 2 : ℝ≥0) : ℝ)))⁻¹ * gw s i := by
  simp only [gaussianPDFReal_def, gw]
  push_cast
  simp [sub_zero]

open ProbabilityTheory NNReal in
/-- Second moment of the Gaussian density equals its variance parameter. -/
theorem continuous_second_moment (v : ℝ≥0) (hv : v ≠ 0) :
    ∫ x : ℝ, x ^ 2 * gaussianPDFReal 0 v x = v := by
  have h := variance_id_gaussianReal (μ := 0) (v := v)
  rw [variance_eq_integral measurable_id.aemeasurable] at h
  simp only [id, integral_id_gaussianReal, sub_zero] at h
  rw [integral_gaussianReal_eq_integral_smul hv] at h
  simpa [smul_eq_mul, mul_comm] using h

end CraftBlur
