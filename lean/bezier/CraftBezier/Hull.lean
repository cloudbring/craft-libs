import CraftBezier.Basic
import Mathlib.Analysis.Convex.Combination
import Mathlib.Analysis.Convex.Hull
import Mathlib.Data.Real.Basic
import Mathlib.Tactic.IntervalCases

/-!
# Convex hull and bounding box of a Bézier curve (over `ℝ`)

Bernstein weights are non-negative on `[0,1]` and sum to one (Mathlib's `bernsteinPolynomial.sum`),
so `B(t)` is a convex combination of the control points.
-/

namespace CraftBezier

open Finset

section Real

variable {E : Type*} [AddCommGroup E] [Module ℝ E]

theorem bern_nonneg {t : ℝ} (ht : t ∈ Set.Icc (0 : ℝ) 1) (n ν : ℕ) : 0 ≤ bern n ν t := by
  have h1 : 0 ≤ 1 - t := sub_nonneg.2 ht.2
  unfold bern
  have := ht.1
  positivity

theorem bern_sum (n : ℕ) (t : ℝ) : ∑ ν ∈ range (n + 1), bern n ν t = 1 := by
  have := congrArg (Polynomial.eval t) (bernsteinPolynomial.sum ℝ n)
  simpa [Polynomial.eval_finset_sum, eval_bernstein] using this

/-- (a) Convex-hull property: for `t ∈ [0,1]`, `B(t)` lies in the convex hull of the control points. -/
theorem bez_mem_convexHull (n : ℕ) (P : ℕ → E) {t : ℝ} (ht : t ∈ Set.Icc (0 : ℝ) 1) :
    bez n P t ∈ convexHull ℝ (P '' (range (n + 1) : Set ℕ)) :=
  (convex_convexHull ℝ _).sum_mem (fun ν _ => bern_nonneg ht n ν) (bern_sum n t)
    (fun ν hν => subset_convexHull ℝ _ ⟨ν, hν, rfl⟩)

/-- Four control points as a sequence (the rest is padded with the last point). -/
def ctrl4 (a b c d : E) : ℕ → E
  | 0 => a
  | 1 => b
  | 2 => c
  | _ => d

/-- (a) for a cubic: `B(t) ∈ convexHull {P0, P1, P2, P3}`. -/
theorem cubic_mem_convexHull (a b c d : E) {t : ℝ} (ht : t ∈ Set.Icc (0 : ℝ) 1) :
    bez 3 (ctrl4 a b c d) t ∈ convexHull ℝ {a, b, c, d} := by
  refine convexHull_mono ?_ (bez_mem_convexHull 3 (ctrl4 a b c d) ht)
  rintro _ ⟨i, hi, rfl⟩
  have : i < 4 := by simpa using hi
  interval_cases i <;> simp [ctrl4]

end Real

/-- (b) Axis-aligned bounding box of the control polygon, in `ℝ × ℝ`. -/
def bboxX (n : ℕ) (P : ℕ → ℝ × ℝ) : Set ℝ :=
  Set.Icc ((range (n + 1)).inf' ⟨0, by simp⟩ fun i => (P i).1)
    ((range (n + 1)).sup' ⟨0, by simp⟩ fun i => (P i).1)

def bboxY (n : ℕ) (P : ℕ → ℝ × ℝ) : Set ℝ :=
  Set.Icc ((range (n + 1)).inf' ⟨0, by simp⟩ fun i => (P i).2)
    ((range (n + 1)).sup' ⟨0, by simp⟩ fun i => (P i).2)

/-- (b) The curve lies in the axis-aligned bounding box of its control points
(min/max of the control coordinates), for `t ∈ [0,1]`. -/
theorem bez_mem_bbox (n : ℕ) (P : ℕ → ℝ × ℝ) {t : ℝ} (ht : t ∈ Set.Icc (0 : ℝ) 1) :
    (bez n P t).1 ∈ bboxX n P ∧ (bez n P t).2 ∈ bboxY n P := by
  have hconv : Convex ℝ (bboxX n P ×ˢ bboxY n P) :=
    (convex_Icc _ _).prod (convex_Icc _ _)
  have hsub : P '' (range (n + 1) : Set ℕ) ⊆ bboxX n P ×ˢ bboxY n P := by
    rintro _ ⟨i, hi, rfl⟩
    have hi' : i ∈ range (n + 1) := hi
    exact ⟨⟨inf'_le (fun i => (P i).1) hi', le_sup' (fun i => (P i).1) hi'⟩,
      ⟨inf'_le (fun i => (P i).2) hi', le_sup' (fun i => (P i).2) hi'⟩⟩
  exact convexHull_min hsub hconv (bez_mem_convexHull n P ht)

end CraftBezier
