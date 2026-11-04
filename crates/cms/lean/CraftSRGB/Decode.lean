import CraftSRGB.Branch

namespace CraftSRGB
open Set

lemma decode_left {x : ℝ} (h : x ≤ kneeA) : decode x = x / (323 / 25) := by simp [decode, h]
lemma decode_right {x : ℝ} (h : kneeA < x) : decode x = dr x := by simp [decode, not_le.2 h]
lemma encode_left {y : ℝ} (h : y ≤ kneeB) : encode y = 323 / 25 * y := by simp [encode, h]
lemma encode_right {y : ℝ} (h : kneeB < y) : encode y = er y := by simp [encode, not_le.2 h]

lemma decode_zero : decode 0 = 0 := by
  rw [decode_left (by unfold kneeA; norm_num)]; simp
lemma decode_one : decode 1 = 1 := by
  rw [decode_right kneeA_lt_one]; exact dr_one
lemma encode_zero : encode 0 = 0 := by
  rw [encode_left kneeB_pos.le]; simp
lemma encode_one : encode 1 = 1 := by
  rw [encode_right (by unfold kneeB; norm_num)]; exact er_one

/-- Decode is monotone on `[0,1]` (the knee jump is upward, `Lval < Rval`). -/
theorem decode_monotoneOn : MonotoneOn decode (Icc 0 1) := by
  intro x hx x' hx' h
  by_cases h1 : x' ≤ kneeA
  · rw [decode_left (h.trans h1), decode_left h1]
    exact div_le_div_of_nonneg_right h (by norm_num)
  · rw [not_le] at h1
    rw [decode_right h1]
    by_cases h2 : x ≤ kneeA
    · rw [decode_left h2]
      calc x / (323 / 25) ≤ Lval := div_le_div_of_nonneg_right h2 (by norm_num)
        _ ≤ Rval := Lval_lt_Rval.le
        _ ≤ dr x' := dr_le (by unfold kneeA; norm_num) h1.le
    · rw [not_le] at h2
      rw [decode_right h2]; exact dr_le hx.1 h

theorem decode_mapsTo : MapsTo decode (Icc 0 1) (Icc 0 1) := by
  intro x hx
  by_cases h : x ≤ kneeA
  · rw [decode_left h]
    refine ⟨by have := hx.1; positivity, ?_⟩
    have : x / (323 / 25) ≤ kneeA / (323 / 25) := div_le_div_of_nonneg_right h (by norm_num)
    unfold kneeA at this; linarith
  · rw [not_le] at h
    rw [decode_right h]
    exact ⟨dr_nonneg hx.1, by rw [← dr_one]; exact dr_le hx.1 hx.2⟩

/-- The exact image of `[0,1]` under decode: `[0, Lval] ∪ (Rval, 1]`. -/
theorem decode_image : decode '' Icc 0 1 = Icc 0 Lval ∪ Ioc Rval 1 := by
  ext v; constructor
  · rintro ⟨x, hx, rfl⟩
    by_cases h : x ≤ kneeA
    · left; rw [decode_left h]
      exact ⟨by have := hx.1; positivity, div_le_div_of_nonneg_right h (by norm_num)⟩
    · rw [not_le] at h
      right; rw [decode_right h]
      refine ⟨?_, by rw [← dr_one]; exact dr_le hx.1 hx.2⟩
      exact dr_lt (by unfold kneeA; norm_num) h
  · rintro (⟨h0, h1⟩ | ⟨h0, h1⟩)
    · refine ⟨323 / 25 * v, ⟨by positivity, ?_⟩, ?_⟩
      · have : 323 / 25 * v ≤ kneeA := by unfold Lval at h1; unfold kneeA at h1 ⊢; linarith
        unfold kneeA at this; linarith
      · have hle : 323 / 25 * v ≤ kneeA := by unfold Lval at h1; unfold kneeA at h1 ⊢; linarith
        rw [decode_left hle]; field_simp
    · have hv : 0 ≤ v := (Rval_pos.trans h0).le
      refine ⟨er v, ⟨?_, ?_⟩, ?_⟩
      · have h3 : 0 < er Rval := by rw [er_Rval]; unfold kneeA; norm_num
        have := er_lt Rval_pos.le h0
        linarith
      · rw [← er_one]; exact er_le hv h1
      · have : kneeA < er v := by
          have := er_lt Rval_pos.le h0; rwa [er_Rval] at this
        rw [decode_right this]; exact dr_er hv

/-- Decode is **not** onto `[0,1]`: `Rval` itself has no preimage. -/
theorem decode_not_surjective : ¬ Icc (0:ℝ) 1 ⊆ decode '' Icc 0 1 := by
  intro h
  have hm : Rval ∈ Icc (0:ℝ) 1 := ⟨Rval_pos.le, by
    have := decode_mapsTo (show kneeA ∈ Icc (0:ℝ) 1 from ⟨by unfold kneeA; norm_num, kneeA_lt_one.le⟩)
    have h1 : Rval ≤ 1 := by unfold Rval; rw [← dr_one]; exact dr_le (by unfold kneeA; norm_num) kneeA_lt_one.le
    exact h1⟩
  have := h hm
  rw [decode_image] at this
  rcases this with ⟨_, h1⟩ | ⟨h1, _⟩
  · exact absurd Lval_lt_Rval (not_lt.2 h1)
  · exact lt_irrefl _ h1

end CraftSRGB
