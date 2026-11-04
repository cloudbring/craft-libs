import CraftBezier.Hull
import Mathlib.Topology.Algebra.Polynomial
import Mathlib.Topology.Order.IntermediateValue
import Mathlib.Topology.Algebra.Order.Field

/-!
# Cubic-Bézier easing: monotone `x(t)` gives a unique solution

A CSS-style `cubic-bezier(x1, y1, x2, y2)` easing has control x-values `0, x1, x2, 1`.  If the
control x-values are monotone (`0 ≤ x1 ≤ x2 ≤ 1`) then `x(t)` is monotone on `[0,1]`, and strictly so
(whenever the end values differ), so `x(t) = X` has exactly one solution `t ∈ [0,1]` for each
`X ∈ [0,1]`.
-/

namespace CraftBezier

/-- Explicit cubic Bernstein form over `ℝ`. -/
theorem cubic_eq (a b c d t : ℝ) :
    bez 3 (ctrl4 a b c d) t =
      (1 - t) ^ 3 * a + 3 * (1 - t) ^ 2 * t * b + 3 * (1 - t) * t ^ 2 * c + t ^ 3 * d := by
  simp [bez, bern, Finset.sum_range_succ, ctrl4, Nat.choose]
  ring

/-- The x-coordinate of a cubic easing curve with control x-values `x0 x1 x2 x3`. -/
noncomputable def xCurve (x0 x1 x2 x3 t : ℝ) : ℝ := bez 3 (ctrl4 x0 x1 x2 x3) t

theorem xCurve_decomp (x0 x1 x2 x3 t : ℝ) :
    xCurve x0 x1 x2 x3 t = x0 + (x1 - x0) * (1 - (1 - t) ^ 3) +
      (x2 - x1) * (3 * t ^ 2 - 2 * t ^ 3) + (x3 - x2) * t ^ 3 := by
  unfold xCurve; rw [cubic_eq]; ring

theorem xCurve_zero (x0 x1 x2 x3 : ℝ) : xCurve x0 x1 x2 x3 0 = x0 := by
  rw [xCurve_decomp]; ring

theorem xCurve_one (x0 x1 x2 x3 : ℝ) : xCurve x0 x1 x2 x3 1 = x3 := by
  rw [xCurve_decomp]; ring

private theorem F0_strict {s t : ℝ} (hst : s < t) (ht : t ≤ 1) :
    (1 - (1 - s) ^ 3) < (1 - (1 - t) ^ 3) := by
  have h1 : 0 ≤ 1 - t := by linarith
  have h2 : 0 < 1 - s := by linarith
  have key : (1 - s) ^ 3 - (1 - t) ^ 3 = (t - s) * ((1 - s) ^ 2 + (1 - s) * (1 - t) + (1 - t) ^ 2) := by
    ring
  have : 0 < (t - s) * ((1 - s) ^ 2 + (1 - s) * (1 - t) + (1 - t) ^ 2) :=
    mul_pos (by linarith) (by positivity)
  linarith

private theorem F1_strict {s t : ℝ} (hs : 0 ≤ s) (hst : s < t) (ht : t ≤ 1) :
    (3 * s ^ 2 - 2 * s ^ 3) < (3 * t ^ 2 - 2 * t ^ 3) := by
  have key : (3 * t ^ 2 - 2 * t ^ 3) - (3 * s ^ 2 - 2 * s ^ 3) =
      (t - s) * (3 * (t + s) - 2 * (t ^ 2 + t * s + s ^ 2)) := by ring
  have h0 : 0 < t := by linarith
  have h1 : 0 < 1 - s := by linarith
  have inner : 0 < 3 * (t + s) - 2 * (t ^ 2 + t * s + s ^ 2) := by
    nlinarith [mul_pos h0 h1, mul_nonneg hs (sub_nonneg.2 ht), mul_nonneg h0.le (sub_nonneg.2 ht),
      mul_nonneg hs (sub_nonneg.2 (by linarith : s ≤ 1))]
  have := mul_pos (sub_pos.2 hst) inner
  linarith

private theorem F2_strict {s t : ℝ} (hs : 0 ≤ s) (hst : s < t) :
    s ^ 3 < t ^ 3 := by
  have h0 : 0 < t := by linarith
  have key : t ^ 3 - s ^ 3 = (t - s) * (t ^ 2 + t * s + s ^ 2) := by ring
  have : 0 < (t - s) * (t ^ 2 + t * s + s ^ 2) := mul_pos (by linarith) (by positivity)
  linarith

/-- (e) Monotone control values `x0 ≤ x1 ≤ x2 ≤ x3` give a monotone cubic `x(t)` on `[0,1]`. -/
theorem xCurve_monotoneOn {x0 x1 x2 x3 : ℝ} (h01 : x0 ≤ x1) (h12 : x1 ≤ x2) (h23 : x2 ≤ x3) :
    MonotoneOn (xCurve x0 x1 x2 x3) (Set.Icc 0 1) := by
  intro s hs t ht hst
  rcases hst.lt_or_eq with h | h
  · rw [xCurve_decomp, xCurve_decomp]
    have a := F0_strict h ht.2
    have b := F1_strict hs.1 h ht.2
    have c := F2_strict hs.1 h
    nlinarith [mul_nonneg (sub_nonneg.2 h01) (sub_nonneg.2 a.le),
      mul_nonneg (sub_nonneg.2 h12) (sub_nonneg.2 b.le),
      mul_nonneg (sub_nonneg.2 h23) (sub_nonneg.2 c.le)]
  · rw [h]

/-- (e) Strict monotonicity when the end values differ. -/
theorem xCurve_strictMonoOn {x0 x1 x2 x3 : ℝ} (h01 : x0 ≤ x1) (h12 : x1 ≤ x2) (h23 : x2 ≤ x3)
    (hne : x0 < x3) : StrictMonoOn (xCurve x0 x1 x2 x3) (Set.Icc 0 1) := by
  intro s hs t ht h
  rw [xCurve_decomp, xCurve_decomp]
  have a := F0_strict h ht.2
  have b := F1_strict hs.1 h ht.2
  have c := F2_strict hs.1 h
  have d0 := sub_nonneg.2 h01
  have d1 := sub_nonneg.2 h12
  have d2 := sub_nonneg.2 h23
  have p0 := mul_nonneg d0 (sub_nonneg.2 a.le)
  have p1 := mul_nonneg d1 (sub_nonneg.2 b.le)
  have p2 := mul_nonneg d2 (sub_nonneg.2 c.le)
  by_cases e0 : 0 < x1 - x0
  · have := mul_pos e0 (sub_pos.2 a); nlinarith
  by_cases e1 : 0 < x2 - x1
  · have := mul_pos e1 (sub_pos.2 b); nlinarith
  have e2 : 0 < x3 - x2 := by linarith
  have := mul_pos e2 (sub_pos.2 c); nlinarith

theorem xCurve_continuous (x0 x1 x2 x3 : ℝ) : Continuous (xCurve x0 x1 x2 x3) := by
  have : xCurve x0 x1 x2 x3 = fun t => x0 + (x1 - x0) * (1 - (1 - t) ^ 3) +
      (x2 - x1) * (3 * t ^ 2 - 2 * t ^ 3) + (x3 - x2) * t ^ 3 := funext (xCurve_decomp x0 x1 x2 x3)
  rw [this]; fun_prop

/-- (e) The easing solve: for control x-values `0 ≤ x1 ≤ x2 ≤ 1` (`x0 = 0`, `x3 = 1`) and any
target `X ∈ [0,1]`, `x(t) = X` has exactly one solution `t ∈ [0,1]`. -/
theorem easing_unique_solution {x1 x2 : ℝ} (h1 : 0 ≤ x1) (h12 : x1 ≤ x2) (h2 : x2 ≤ 1)
    {X : ℝ} (hX : X ∈ Set.Icc (0 : ℝ) 1) :
    ∃! t, t ∈ Set.Icc (0 : ℝ) 1 ∧ xCurve 0 x1 x2 1 t = X := by
  have hmono := xCurve_strictMonoOn h1 h12 h2 zero_lt_one
  have hivt := intermediate_value_Icc (zero_le_one' ℝ)
    (xCurve_continuous 0 x1 x2 1).continuousOn
  rw [xCurve_zero, xCurve_one] at hivt
  obtain ⟨t, ht, hx⟩ := hivt hX
  exact ⟨t, ⟨ht, hx⟩, fun t' ⟨ht', hx'⟩ => hmono.injOn ht' ht (hx'.trans hx.symm)⟩

end CraftBezier
