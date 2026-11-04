import CraftSRGB.Defs

namespace CraftSRGB

private lemma base_nonneg {x : ℝ} (hx : 0 ≤ x) : 0 ≤ (x + 11 / 200) / (211 / 200) := by positivity

lemma dr_nonneg {x : ℝ} (hx : 0 ≤ x) : 0 ≤ dr x := Real.rpow_nonneg (base_nonneg hx) _

lemma er_dr {x : ℝ} (hx : 0 ≤ x) : er (dr x) = x := by
  unfold er dr
  rw [rpow_inv_cancel1 (base_nonneg hx)]
  field_simp; ring

lemma dr_er {y : ℝ} (hy : 0 ≤ y) : dr (er y) = y := by
  unfold dr er
  have : (211 / 200 * y ^ (5 / 12 : ℝ) - 11 / 200 + 11 / 200) / (211 / 200) = y ^ (5 / 12 : ℝ) := by
    field_simp; ring
  rw [this, rpow_inv_cancel2 hy]

lemma dr_lt {x x' : ℝ} (hx : 0 ≤ x) (h : x < x') : dr x < dr x' := by
  unfold dr
  exact Real.rpow_lt_rpow (base_nonneg hx) (by linarith) (by norm_num)

lemma dr_le {x x' : ℝ} (hx : 0 ≤ x) (h : x ≤ x') : dr x ≤ dr x' := by
  unfold dr
  exact Real.rpow_le_rpow (base_nonneg hx) (by linarith) (by norm_num)

lemma er_lt {y y' : ℝ} (hy : 0 ≤ y) (h : y < y') : er y < er y' := by
  unfold er
  have := Real.rpow_lt_rpow hy h (show (0:ℝ) < 5 / 12 by norm_num)
  linarith

lemma er_le {y y' : ℝ} (hy : 0 ≤ y) (h : y ≤ y') : er y ≤ er y' := by
  unfold er
  have := Real.rpow_le_rpow hy h (show (0:ℝ) ≤ 5 / 12 by norm_num)
  linarith

lemma dr_one : dr 1 = 1 := by unfold dr; norm_num
lemma er_one : er 1 = 1 := by unfold er; norm_num

/-! ### Knee constants -/

lemma kneeA_val : kneeA = 809 / 20000 := rfl
lemma kneeB_val : kneeB = 7827 / 2500000 := rfl

lemma Lval_pos : 0 < Lval := by unfold Lval kneeA; norm_num
lemma kneeB_pos : 0 < kneeB := by unfold kneeB; norm_num
lemma kval_lt_kneeA : kval < kneeA := by unfold kval kneeA kneeB; norm_num
lemma kneeB_lt_Lval : kneeB < Lval := by unfold Lval kneeA kneeB; norm_num
lemma kneeA_lt_one : kneeA < 1 := by unfold kneeA; norm_num
lemma kval_pos : 0 < kval := by unfold kval kneeB; norm_num

/-- The decode jump is positive: `0.04045/12.92 < ((0.04045+0.055)/1.055)^2.4`. -/
lemma Lval_lt_Rval : Lval < Rval := by
  unfold Rval dr
  rw [lt_rpow125 Lval_pos.le (by unfold kneeA; norm_num)]
  unfold Lval kneeA; norm_num

lemma kneeB_lt_Rval : kneeB < Rval := lt_trans kneeB_lt_Lval Lval_lt_Rval

lemma Rval_pos : 0 < Rval := lt_trans Lval_pos Lval_lt_Rval

/-- `dr kval`: where the encode power branch crosses `kval`. -/
lemma kneeB_lt_dr_kval : kneeB < dr kval := by
  unfold dr
  rw [lt_rpow125 kneeB_pos.le (by unfold kval kneeB; norm_num)]
  unfold kval kneeB; norm_num

/-- Encode is *discontinuous* at its knee: the power branch starts strictly below `12.92 * 0.0031308`. -/
lemma er_kneeB_lt_kval : er kneeB < kval := by
  have h := er_lt kneeB_pos.le kneeB_lt_dr_kval
  rwa [er_dr kval_pos.le] at h

lemma dr_zero_lt_kneeB : dr 0 < kneeB := by
  unfold dr
  rw [rpow125_lt kneeB_pos.le (by norm_num)]
  unfold kneeB; norm_num

lemma er_kneeB_pos : 0 < er kneeB := by
  have h := er_lt (dr_nonneg (le_refl (0:ℝ))) dr_zero_lt_kneeB
  rwa [er_dr (le_refl (0:ℝ))] at h

lemma er_pos {y : ℝ} (hy : kneeB < y) : 0 < er y :=
  lt_trans er_kneeB_pos (er_lt kneeB_pos.le hy)

lemma er_Rval : er Rval = kneeA := er_dr (by unfold kneeA; norm_num)

end CraftSRGB
