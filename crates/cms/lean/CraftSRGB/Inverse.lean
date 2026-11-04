import CraftSRGB.Encode

namespace CraftSRGB
open Set

/-- Maximum |encode(decode x) - x| over the non-inverse zone `(kval, kneeA]`. -/
noncomputable def edBound : ℝ := kneeA - er kneeB
/-- Maximum |decode(encode y) - y| over the non-inverse zone `(kneeB, Rval]`. -/
noncomputable def deBound : ℝ := Rval - er kneeB / (323 / 25)

/-- `encode ∘ decode = id` on the toe `[0, 0.040449936]`. -/
theorem encode_decode_toe {x : ℝ} (h0 : 0 ≤ x) (h : x ≤ kval) : encode (decode x) = x := by
  have h1 : x ≤ kneeA := h.trans kval_lt_kneeA.le
  rw [decode_left h1, encode_left (by unfold kval kneeB at h; unfold kneeB; linarith)]
  ring

/-- `encode ∘ decode = id` on `(0.04045, 1]`. -/
theorem encode_decode_power {x : ℝ} (h : kneeA < x) : encode (decode x) = x := by
  rw [decode_right h]
  have hr : Rval < dr x := dr_lt (by unfold kneeA; norm_num) h
  rw [encode_right (kneeB_lt_Rval.trans hr)]
  exact er_dr (by unfold kneeA at h; linarith)

/-- Non-inverse zone for `encode ∘ decode`: `x ∈ (0.040449936, 0.04045]`, error at most `edBound`. -/
theorem encode_decode_gap {x : ℝ} (h0 : kval < x) (h1 : x ≤ kneeA) :
    |encode (decode x) - x| ≤ edBound := by
  rw [decode_left h1]
  have hb : kneeB < x / (323 / 25) := by unfold kval at h0; linarith [show x / (323/25) = x * (25/323) by ring]
  rw [encode_right hb]
  have hlo : er kneeB ≤ er (x / (323 / 25)) := er_le kneeB_pos.le hb.le
  have hx : x / (323 / 25) ≤ Lval := by unfold Lval; exact div_le_div_of_nonneg_right h1 (by norm_num)
  have hhi : er (x / (323 / 25)) ≤ kneeA := by
    have := er_le (hb.le.trans' kneeB_pos.le |>.trans' le_rfl) (hx.trans Lval_lt_Rval.le)
    rwa [er_Rval] at this
  unfold edBound
  have : 0 < encDip := encDip_pos
  unfold encDip kval at this
  rw [abs_le]; constructor
  · unfold kval at h0; linarith
  · unfold kval at h0; linarith

/-- `decode ∘ encode = id` on the toe `[0, 0.0031308]`. -/
theorem decode_encode_toe {y : ℝ} (h0 : 0 ≤ y) (h : y ≤ kneeB) : decode (encode y) = y := by
  rw [encode_left h]
  have : 323 / 25 * y ≤ kneeA := by
    have : 323 / 25 * y ≤ kval := by unfold kval; nlinarith
    exact this.trans kval_lt_kneeA.le
  rw [decode_left this]; field_simp

/-- `decode ∘ encode = id` on `(Rval, 1]`. -/
theorem decode_encode_power {y : ℝ} (h : Rval < y) : decode (encode y) = y := by
  have hb := kneeB_lt_Rval.trans h
  rw [encode_right hb]
  have : kneeA < er y := by have := er_lt Rval_pos.le h; rwa [er_Rval] at this
  rw [decode_right this]; exact dr_er (kneeB_pos.le.trans hb.le)

/-- Non-inverse zone for `decode ∘ encode`: `y ∈ (0.0031308, Rval]`, error at most `deBound`. -/
theorem decode_encode_gap {y : ℝ} (h0 : kneeB < y) (h1 : y ≤ Rval) :
    |decode (encode y) - y| ≤ deBound := by
  rw [encode_right h0]
  have hle : er y ≤ kneeA := by have := er_le (kneeB_pos.le.trans h0.le) h1; rwa [er_Rval] at this
  rw [decode_left hle]
  have hlo : er kneeB ≤ er y := er_le kneeB_pos.le h0.le
  have hk := er_kneeB_lt_kval
  unfold deBound
  have e1 : er y / (323 / 25) ≤ Lval := by unfold Lval; exact div_le_div_of_nonneg_right hle (by norm_num)
  have e2 : er kneeB / (323 / 25) ≤ er y / (323 / 25) := div_le_div_of_nonneg_right hlo (by norm_num)
  have e3 : er kneeB / (323 / 25) < kneeB := by
    unfold kval at hk; linarith [show er kneeB / (323/25) = er kneeB * (25/323) by ring]
  have := Lval_lt_Rval
  rw [abs_le]; constructor <;> linarith

/-- Every 8-bit code round-trips exactly through the real-valued decode/encode. -/
theorem eight_bit_roundtrip (c : ℕ) (hc : c ≤ 255) :
    encode (decode ((c : ℝ) / 255)) = (c : ℝ) / 255 := by
  by_cases h : c ≤ 10
  · apply encode_decode_toe (by positivity)
    have : (c : ℝ) ≤ 10 := by exact_mod_cast h
    unfold kval kneeB; linarith [show (c:ℝ)/255 ≤ 10/255 by gcongr]
  · apply encode_decode_power
    have : (11 : ℝ) ≤ c := by exact_mod_cast (by omega : 11 ≤ c)
    unfold kneeA; linarith [show (11:ℝ)/255 ≤ c/255 by gcongr]

end CraftSRGB
