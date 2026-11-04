import CraftSRGB.Decode

namespace CraftSRGB
open Set

/-- The size of encode's downward jump at its knee: `12.92*0.0031308 - er(0.0031308)`. -/
noncomputable def encDip : ℝ := kval - er kneeB

lemma encDip_pos : 0 < encDip := by unfold encDip; linarith [er_kneeB_lt_kval]

/-- Encode is monotone on `[0, 0.0031308]`. -/
theorem encode_monotoneOn_toe : MonotoneOn encode (Icc 0 kneeB) := by
  intro y hy y' hy' h
  rw [encode_left hy.2, encode_left hy'.2]; linarith

/-- Encode is strictly increasing on `(0.0031308, 1]`. -/
theorem encode_strictMonoOn_power : StrictMonoOn encode (Ioc kneeB 1) := by
  intro y hy y' hy' h
  rw [encode_right hy.1, encode_right hy'.1]
  exact er_lt (kneeB_pos.le.trans hy.1.le) h

/-- Encode is monotone *up to* `encDip`: `y ≤ y' → encode y ≤ encode y' + encDip` on `[0,1]`. -/
theorem encode_almost_monotone {y y' : ℝ} (hy : 0 ≤ y) (h : y ≤ y') :
    encode y ≤ encode y' + encDip := by
  by_cases h1 : y' ≤ kneeB
  · rw [encode_left (h.trans h1), encode_left h1]
    have : 0 < encDip := encDip_pos
    linarith
  · rw [not_le] at h1
    rw [encode_right h1]
    have h2 := er_lt kneeB_pos.le h1
    by_cases h3 : y ≤ kneeB
    · rw [encode_left h3]
      have : 323 / 25 * y ≤ kval := by unfold kval; nlinarith
      unfold encDip at *; linarith
    · rw [not_le] at h3
      rw [encode_right h3]
      have := er_le hy h
      have : 0 < encDip := encDip_pos
      linarith

/-- Encode is **not** monotone: just above the knee it dips below its own knee value. -/
theorem encode_not_monotone : ¬ MonotoneOn encode (Icc 0 1) := by
  intro hm
  have hy : kneeB ∈ Icc (0:ℝ) 1 := ⟨kneeB_pos.le, by unfold kneeB; norm_num⟩
  have hy' : (3130801 / 1000000000 : ℝ) ∈ Icc (0:ℝ) 1 := by norm_num
  have hle : kneeB ≤ 3130801 / 1000000000 := by unfold kneeB; norm_num
  have := hm hy hy' hle
  have hgt : kneeB < 3130801 / 1000000000 := by unfold kneeB; norm_num
  rw [encode_left le_rfl, encode_right hgt] at this
  have : er (3130801 / 1000000000) < kval := by
    have hlt : (3130801 / 1000000000 : ℝ) < dr kval := by
      unfold dr
      rw [lt_rpow125 (by norm_num) (by unfold kval kneeB; norm_num)]
      unfold kval kneeB; norm_num
    have := er_lt (by norm_num) hlt
    rwa [er_dr kval_pos.le] at this
  unfold kval at this
  linarith

theorem encode_mapsTo : MapsTo encode (Icc 0 1) (Icc 0 1) := by
  intro y hy
  by_cases h : y ≤ kneeB
  · rw [encode_left h]
    refine ⟨by have := hy.1; positivity, ?_⟩
    unfold kneeB at h; linarith
  · rw [not_le] at h
    rw [encode_right h]
    exact ⟨(er_pos h).le, by rw [← er_one]; exact er_le hy.1 hy.2⟩

/-- Encode *is* onto `[0,1]` (the dip makes the two branches overlap, so there is no gap). -/
theorem encode_image : encode '' Icc 0 1 = Icc 0 1 := by
  apply Subset.antisymm
  · rintro _ ⟨y, hy, rfl⟩; exact encode_mapsTo hy
  · intro v hv
    by_cases h : v ≤ kval
    · refine ⟨v / (323 / 25), ⟨by have := hv.1; positivity, ?_⟩, ?_⟩
      · unfold kval kneeB at h; linarith [hv.1]
      · have : v / (323 / 25) ≤ kneeB := by unfold kval at h; linarith [show v / (323/25) = v * (25/323) by ring]
        rw [encode_left this]; field_simp
    · rw [not_le] at h
      have hv0 : 0 ≤ v := hv.1
      have hk : dr kval < dr v := dr_lt kval_pos.le h
      have hb := kneeB_lt_dr_kval.trans hk
      refine ⟨dr v, ⟨(dr_nonneg hv0), ?_⟩, ?_⟩
      · rw [← dr_one]; exact dr_le hv0 hv.2
      · rw [encode_right hb]; exact er_dr hv0

end CraftSRGB
