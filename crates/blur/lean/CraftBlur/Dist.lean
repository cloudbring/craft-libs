import Mathlib

/-!
# Finite discrete kernels, moments and convolution

A kernel is a finite list of `(offset, weight)` pairs.  `mom k d = ∑ w * offset^k`.
The box cascade is a convolution of uniform boxes, so its variance is computed from
these moments exactly (over `ℚ`).
-/

namespace CraftBlur

abbrev Dist := List (ℤ × ℚ)

def mom (k : ℕ) (d : Dist) : ℚ := (d.map fun p => p.2 * (p.1 : ℚ) ^ k).sum

/-- Discrete convolution of two finite kernels. -/
def conv (d e : Dist) : Dist := d.flatMap fun a => e.map fun b => (a.1 + b.1, a.2 * b.2)

/-- Variance of a unit-mass kernel: `E[x²] - E[x]²`. -/
def var (d : Dist) : ℚ := mom 2 d - mom 1 d ^ 2

@[simp] lemma mom_nil (k : ℕ) : mom k [] = 0 := by simp [mom]

lemma mom_cons (k : ℕ) (a : ℤ × ℚ) (d : Dist) :
    mom k (a :: d) = a.2 * (a.1 : ℚ) ^ k + mom k d := by simp [mom]

lemma mom_append (k : ℕ) (d e : Dist) : mom k (d ++ e) = mom k d + mom k e := by
  simp [mom]

/-- The inner sum of the convolution: one point of `d` against all of `e`. -/
lemma inner_mom0 (a : ℤ × ℚ) (e : Dist) :
    mom 0 (e.map fun b => (a.1 + b.1, a.2 * b.2)) = a.2 * mom 0 e := by
  induction e with
  | nil => simp
  | cons b e ih => simp only [List.map_cons, mom_cons] at *; rw [ih]; simp; ring

lemma inner_mom1 (a : ℤ × ℚ) (e : Dist) :
    mom 1 (e.map fun b => (a.1 + b.1, a.2 * b.2)) = a.2 * (a.1 * mom 0 e + mom 1 e) := by
  induction e with
  | nil => simp
  | cons b e ih => simp only [List.map_cons, mom_cons] at *; rw [ih]; push_cast; ring

lemma inner_mom2 (a : ℤ × ℚ) (e : Dist) :
    mom 2 (e.map fun b => (a.1 + b.1, a.2 * b.2)) =
      a.2 * ((a.1 : ℚ) ^ 2 * mom 0 e + 2 * a.1 * mom 1 e + mom 2 e) := by
  induction e with
  | nil => simp
  | cons b e ih => simp only [List.map_cons, mom_cons] at *; rw [ih]; push_cast; ring

theorem mom0_conv (d e : Dist) : mom 0 (conv d e) = mom 0 d * mom 0 e := by
  induction d with
  | nil => simp [conv]
  | cons a d ih =>
    simp only [conv, List.flatMap_cons, mom_append, mom_cons] at *
    rw [inner_mom0, ih]; simp; ring

theorem mom1_conv (d e : Dist) :
    mom 1 (conv d e) = mom 1 d * mom 0 e + mom 0 d * mom 1 e := by
  induction d with
  | nil => simp [conv]
  | cons a d ih =>
    simp only [conv, List.flatMap_cons, mom_append, mom_cons] at *
    rw [inner_mom1, ih]; simp; ring

theorem mom2_conv (d e : Dist) :
    mom 2 (conv d e) =
      mom 2 d * mom 0 e + 2 * mom 1 d * mom 1 e + mom 0 d * mom 2 e := by
  induction d with
  | nil => simp [conv]
  | cons a d ih =>
    simp only [conv, List.flatMap_cons, mom_append, mom_cons] at *
    rw [inner_mom2, ih]; simp; ring

/-- Variance is additive under convolution of unit-mass, zero-mean kernels. -/
theorem var_conv (d e : Dist) (hd0 : mom 0 d = 1) (he0 : mom 0 e = 1)
    (hd1 : mom 1 d = 0) (he1 : mom 1 e = 0) :
    mom 0 (conv d e) = 1 ∧ mom 1 (conv d e) = 0 ∧ var (conv d e) = var d + var e := by
  refine ⟨?_, ?_, ?_⟩
  · rw [mom0_conv, hd0, he0]; norm_num
  · rw [mom1_conv, hd0, he0, hd1, he1]; norm_num
  · unfold var; rw [mom2_conv, mom1_conv, hd0, he0, hd1, he1]; ring

/-- Mirror image of a kernel. -/
def reflect (d : Dist) : Dist := d.map fun p => (-p.1, p.2)

lemma mom_reflect (k : ℕ) (d : Dist) : mom k (reflect d) = (-1 : ℚ) ^ k * mom k d := by
  induction d with
  | nil => simp [reflect]
  | cons a d ih =>
    simp only [reflect, List.map_cons, mom_cons] at *
    rw [ih]; push_cast; rw [neg_pow]; ring

/-- A forward pass followed by its mirror image (a causal then an anti-causal pass) has
twice the variance of one pass, whatever the mean of the single pass is. -/
theorem var_conv_reflect (d : Dist) (h0 : mom 0 d = 1) :
    mom 1 (conv d (reflect d)) = 0 ∧ var (conv d (reflect d)) = 2 * var d := by
  have h0' : mom 0 (reflect d) = 1 := by rw [mom_reflect, h0]; norm_num
  have h1' : mom 1 (reflect d) = - mom 1 d := by rw [mom_reflect]; norm_num
  have h2' : mom 2 (reflect d) = mom 2 d := by rw [mom_reflect]; norm_num
  refine ⟨?_, ?_⟩
  · rw [mom1_conv, h0, h0', h1']; ring
  · unfold var; rw [mom2_conv, mom1_conv, h0, h0', h1', h2']; ring

/-! ### Uniform boxes -/

lemma list_sum_range (f : ℕ → ℚ) (n : ℕ) :
    ((List.range n).map f).sum = ∑ i ∈ Finset.range n, f i := by
  induction n with
  | zero => simp
  | succ n ih => rw [List.range_succ, List.map_append, List.sum_append, ih, Finset.sum_range_succ]; simp

lemma sum_range_id (n : ℕ) : ∑ i ∈ Finset.range n, (i : ℚ) = n * (n - 1) / 2 := by
  induction n with
  | zero => simp
  | succ n ih => rw [Finset.sum_range_succ, ih]; push_cast; ring

lemma sum_range_sq (n : ℕ) :
    ∑ i ∈ Finset.range n, (i : ℚ) ^ 2 = n * (n - 1) * (2 * n - 1) / 6 := by
  induction n with
  | zero => simp
  | succ n ih => rw [Finset.sum_range_succ, ih]; push_cast; ring

/-- Uniform box of radius `r` (odd width `2r+1`), weights `1/(2r+1)`. -/
def box (r : ℕ) : Dist := (List.range (2 * r + 1)).map fun (i : ℕ) => ((i : ℤ) - r, 1 / (2 * (r : ℚ) + 1))

theorem box_mom0 (r : ℕ) : mom 0 (box r) = 1 := by
  unfold mom box
  rw [List.map_map, list_sum_range]
  simp only [Function.comp, pow_zero, mul_one, Finset.sum_const, Finset.card_range, nsmul_eq_mul]
  push_cast
  field_simp

theorem box_mom1 (r : ℕ) : mom 1 (box r) = 0 := by
  unfold mom box
  rw [List.map_map, list_sum_range]
  simp only [Function.comp, pow_one]
  push_cast
  rw [← Finset.mul_sum, Finset.sum_sub_distrib, sum_range_id]
  simp only [Finset.sum_const, Finset.card_range, nsmul_eq_mul]
  push_cast
  ring

/-- Variance of one odd-width box: `(w² - 1)/12` with `w = 2r+1`. -/
theorem box_mom2 (r : ℕ) : mom 2 (box r) = ((2 * (r : ℚ) + 1) ^ 2 - 1) / 12 := by
  unfold mom box
  rw [List.map_map, list_sum_range]
  simp only [Function.comp]
  have : ∀ i : ℕ, 1 / (2 * (r : ℚ) + 1) * (((i : ℤ) - r : ℤ) : ℚ) ^ 2 =
      1 / (2 * (r : ℚ) + 1) * ((i : ℚ) ^ 2 - 2 * r * i + (r : ℚ) ^ 2) := by
    intro i; push_cast; ring
  simp only [this]
  rw [← Finset.mul_sum, Finset.sum_add_distrib, Finset.sum_sub_distrib, sum_range_sq,
    ← Finset.mul_sum, sum_range_id]
  simp only [Finset.sum_const, Finset.card_range, nsmul_eq_mul]
  push_cast
  field_simp
  ring

theorem box_var (r : ℕ) : var (box r) = ((2 * (r : ℚ) + 1) ^ 2 - 1) / 12 := by
  unfold var; rw [box_mom2, box_mom1]; ring

/-! ### Cascades -/

/-- Cascade of boxes with the given radii (convolution of all of them). -/
def cascade : List ℕ → Dist
  | [] => [(0, 1)]
  | r :: rs => conv (box r) (cascade rs)

/-- Sum of the single-box variances for a list of radii. -/
def boxVarSum (rs : List ℕ) : ℚ := (rs.map fun (r : ℕ) => ((2 * (r : ℚ) + 1) ^ 2 - 1) / 12).sum

theorem cascade_moments (rs : List ℕ) :
    mom 0 (cascade rs) = 1 ∧ mom 1 (cascade rs) = 0 ∧ var (cascade rs) = boxVarSum rs := by
  induction rs with
  | nil => simp [cascade, mom, var, boxVarSum]
  | cons r rs ih =>
    obtain ⟨h0, h1, hv⟩ := ih
    obtain ⟨a, b, c⟩ := var_conv (box r) (cascade rs) (box_mom0 r) h0 (box_mom1 r) h1
    refine ⟨a, b, ?_⟩
    simp only [cascade]
    rw [c, box_var, hv]; simp [boxVarSum]

/-- Width form: odd widths `w = 2r+1`; the variance of the cascade is `∑ (w² - 1)/12`. -/
def cascadeW (ws : List ℕ) : Dist := cascade (ws.map (· / 2))

def widthVar (ws : List ℕ) : ℚ := (ws.map fun (w : ℕ) => ((w : ℚ) ^ 2 - 1) / 12).sum

theorem cascadeW_var (ws : List ℕ) (hodd : ∀ w ∈ ws, w % 2 = 1) :
    mom 0 (cascadeW ws) = 1 ∧ mom 1 (cascadeW ws) = 0 ∧ var (cascadeW ws) = widthVar ws := by
  unfold cascadeW
  obtain ⟨a, b, c⟩ := cascade_moments (ws.map (· / 2))
  refine ⟨a, b, ?_⟩
  rw [c]
  unfold boxVarSum widthVar
  rw [List.map_map]
  congr 1
  apply List.map_congr_left
  intro w hw
  have h := hodd w hw
  have : (2 * ((w / 2 : ℕ) : ℚ) + 1) = (w : ℚ) := by
    have : 2 * (w / 2) + 1 = w := by omega
    exact_mod_cast this
  simp [Function.comp, this]

end CraftBlur
