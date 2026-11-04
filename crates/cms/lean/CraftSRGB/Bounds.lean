import CraftSRGB.Inverse

/-!
Proved rational enclosures of the knee-mismatch quantities. Each real quantity is bracketed by
exact rationals using `x < y^(12/5) ↔ x^5 < y^12` (and the 5/12 analogue), so nothing here is
numerical evidence: every bound is a theorem.
-/
namespace CraftSRGB

lemma er_lt_of {y t : ℝ} (hy : 0 ≤ y) (ht : 0 ≤ (200 * t + 11) / 211)
    (h : y ^ 5 < ((200 * t + 11) / 211) ^ 12) : er y < t := by
  have := (rpow512_lt ht hy).2 h
  unfold er; linarith [show (211:ℝ)/200 * ((200 * t + 11) / 211) = t + 11/200 by ring,
    mul_lt_mul_of_pos_left this (show (0:ℝ) < 211/200 by norm_num)]

lemma lt_er_of {y t : ℝ} (hy : 0 ≤ y) (ht : 0 ≤ (200 * t + 11) / 211)
    (h : ((200 * t + 11) / 211) ^ 12 < y ^ 5) : t < er y := by
  have := (lt_rpow512 ht hy).2 h
  unfold er; linarith [show (211:ℝ)/200 * ((200 * t + 11) / 211) = t + 11/200 by ring,
    mul_lt_mul_of_pos_left this (show (0:ℝ) < 211/200 by norm_num)]

/-- `er (0.0031308) = 0.04044990748269…`. -/
lemma er_kneeB_bounds : (4044990748269 / 100000000000000 : ℝ) < er kneeB ∧ er kneeB < 4044990748270 / 100000000000000 := by
  unfold kneeB
  exact ⟨lt_er_of (by norm_num) (by norm_num) (by norm_num), er_lt_of (by norm_num) (by norm_num) (by norm_num)⟩

lemma er_Lval_bounds : (4044997040812 / 100000000000000 : ℝ) < er Lval ∧ er Lval < 4044997040813 / 100000000000000 := by
  unfold Lval kneeA
  exact ⟨lt_er_of (by norm_num) (by norm_num) (by norm_num), er_lt_of (by norm_num) (by norm_num) (by norm_num)⟩

lemma Rval_bounds : (3130807283067 / 1000000000000000 : ℝ) < Rval ∧ Rval < 3130807283068 / 1000000000000000 := by
  unfold Rval dr kneeA
  constructor
  · rw [lt_rpow125 (by norm_num) (by norm_num)]; norm_num
  · rw [rpow125_lt (by norm_num) (by norm_num)]; norm_num

/-- Decode's jump at `0.04045`: `Rval - Lval ∈ (2.3295e-9, 2.3296e-9)`. -/
theorem decode_jump_bounds : (23295 / 10000000000000 : ℝ) < Rval - Lval ∧ Rval - Lval < 23296 / 10000000000000 := by
  have := Rval_bounds; unfold Lval kneeA; constructor <;> linarith [this.1, this.2]

/-- Encode's downward jump at `0.0031308`: `encDip ∈ (2.8517e-8, 2.8518e-8)`. -/
theorem encode_dip_bounds : (28517 / 1000000000000 : ℝ) < encDip ∧ encDip < 28518 / 1000000000000 := by
  have := er_kneeB_bounds; unfold encDip kval kneeB at *; constructor <;> linarith [this.1, this.2]

/-- `edBound ∈ (9.2517e-8, 9.2518e-8)`. -/
theorem edBound_bounds : (92517 / 1000000000000 : ℝ) < edBound ∧ edBound < 92518 / 1000000000000 := by
  have := er_kneeB_bounds; unfold edBound kneeA at *; constructor <;> linarith [this.1, this.2]

/-- `deBound ∈ (9.49e-9, 9.491e-9)`. -/
theorem deBound_bounds : (949 / 100000000000 : ℝ) < deBound ∧ deBound < 9491 / 1000000000000 := by
  have := er_kneeB_bounds; have := Rval_bounds
  unfold deBound; constructor <;> linarith [this.1, this.2, er_kneeB_bounds.1, er_kneeB_bounds.2]

/-- At the decode knee the round trip errs by `kneeA - encode(decode kneeA) ∈ (2.9591e-8, 2.9592e-8)`;
this is the supremum of `|encode(decode x) - x|` observed (the proved bound over the whole zone is
`edBound`). -/
theorem roundtrip_error_at_knee :
    encode (decode kneeA) = er Lval ∧
    (29591 / 1000000000000 : ℝ) < kneeA - encode (decode kneeA) ∧ kneeA - encode (decode kneeA) < 29592 / 1000000000000 := by
  have h : encode (decode kneeA) = er Lval := by
    rw [decode_left le_rfl]; exact encode_right kneeB_lt_Lval
  have := er_Lval_bounds
  refine ⟨h, ?_⟩
  rw [h]; unfold kneeA at *; constructor <;> linarith [this.1, this.2]

end CraftSRGB
