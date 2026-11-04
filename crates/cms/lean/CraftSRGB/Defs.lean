import Mathlib

/-!
sRGB transfer functions (IEC 61966-2-1) over `ℝ`, with `Real.rpow`.

Decimal constants as exact rationals: 12.92 = 323/25, 0.055 = 11/200, 1.055 = 211/200,
0.04045 = 809/20000, 0.0031308 = 7827/2500000, 2.4 = 12/5.
-/
namespace CraftSRGB

/-- Decode knee `0.04045` (encoded domain). -/
noncomputable def kneeA : ℝ := 809 / 20000
/-- Encode knee `0.0031308` (linear domain). -/
noncomputable def kneeB : ℝ := 7827 / 2500000

/-- Power branch of decode: `((x + 0.055) / 1.055) ^ 2.4`. -/
noncomputable def dr (x : ℝ) : ℝ := ((x + 11 / 200) / (211 / 200)) ^ (12 / 5 : ℝ)
/-- Power branch of encode: `1.055 * y ^ (1/2.4) - 0.055`. -/
noncomputable def er (y : ℝ) : ℝ := 211 / 200 * y ^ (5 / 12 : ℝ) - 11 / 200

/-- sRGB → linear. -/
noncomputable def decode (x : ℝ) : ℝ := if x ≤ kneeA then x / (323 / 25) else dr x
/-- linear → sRGB. -/
noncomputable def encode (y : ℝ) : ℝ := if y ≤ kneeB then 323 / 25 * y else er y

/-- `0.04045 / 12.92`: decode's left limit at the knee (attained at the knee). -/
noncomputable def Lval : ℝ := kneeA / (323 / 25)
/-- `((0.04045+0.055)/1.055)^2.4`: decode's right limit at the knee (not attained). -/
noncomputable def Rval : ℝ := dr kneeA
/-- `12.92 * 0.0031308 = 0.040449936`: encode's value at its knee. -/
noncomputable def kval : ℝ := 323 / 25 * kneeB

/-! ### rpow helpers: `u^(12/5)` and `y^(5/12)` via integer powers -/

lemma rpow125_pow5 {u : ℝ} (hu : 0 ≤ u) : (u ^ (12 / 5 : ℝ)) ^ 5 = u ^ 12 := by
  rw [← Real.rpow_natCast, ← Real.rpow_mul hu]; norm_num

lemma rpow512_pow12 {y : ℝ} (hy : 0 ≤ y) : (y ^ (5 / 12 : ℝ)) ^ 12 = y ^ 5 := by
  rw [← Real.rpow_natCast, ← Real.rpow_mul hy]; norm_num

/-- `c < u^(12/5) ↔ c^5 < u^12` (for `0 ≤ c`, `0 ≤ u`). -/
lemma lt_rpow125 {c u : ℝ} (hc : 0 ≤ c) (hu : 0 ≤ u) : c < u ^ (12 / 5 : ℝ) ↔ c ^ 5 < u ^ 12 := by
  have h5 := rpow125_pow5 hu
  constructor
  · intro h; rw [← h5]; exact pow_lt_pow_left₀ h hc (by norm_num)
  · intro h; rw [← h5] at h; exact lt_of_pow_lt_pow_left₀ 5 (Real.rpow_nonneg hu _) h

lemma rpow125_lt {c u : ℝ} (hc : 0 ≤ c) (hu : 0 ≤ u) : u ^ (12 / 5 : ℝ) < c ↔ u ^ 12 < c ^ 5 := by
  have h5 := rpow125_pow5 hu
  constructor
  · intro h; rw [← h5]; exact pow_lt_pow_left₀ h (Real.rpow_nonneg hu _) (by norm_num)
  · intro h; rw [← h5] at h; exact lt_of_pow_lt_pow_left₀ 5 hc h

lemma le_rpow125 {c u : ℝ} (hc : 0 ≤ c) (hu : 0 ≤ u) : c ≤ u ^ (12 / 5 : ℝ) ↔ c ^ 5 ≤ u ^ 12 := by
  have h5 := rpow125_pow5 hu
  constructor
  · intro h; rw [← h5]; exact pow_le_pow_left₀ hc h 5
  · intro h; rw [← h5] at h; exact le_of_pow_le_pow_left₀ (by norm_num) (Real.rpow_nonneg hu _) h

lemma rpow125_le {c u : ℝ} (hc : 0 ≤ c) (hu : 0 ≤ u) : u ^ (12 / 5 : ℝ) ≤ c ↔ u ^ 12 ≤ c ^ 5 := by
  have h5 := rpow125_pow5 hu
  constructor
  · intro h; rw [← h5]; exact pow_le_pow_left₀ (Real.rpow_nonneg hu _) h 5
  · intro h; rw [← h5] at h; exact le_of_pow_le_pow_left₀ (by norm_num) hc h

lemma lt_rpow512 {c y : ℝ} (hc : 0 ≤ c) (hy : 0 ≤ y) : c < y ^ (5 / 12 : ℝ) ↔ c ^ 12 < y ^ 5 := by
  have h5 := rpow512_pow12 hy
  constructor
  · intro h; rw [← h5]; exact pow_lt_pow_left₀ h hc (by norm_num)
  · intro h; rw [← h5] at h; exact lt_of_pow_lt_pow_left₀ 12 (Real.rpow_nonneg hy _) h

lemma rpow512_lt {c y : ℝ} (hc : 0 ≤ c) (hy : 0 ≤ y) : y ^ (5 / 12 : ℝ) < c ↔ y ^ 5 < c ^ 12 := by
  have h5 := rpow512_pow12 hy
  constructor
  · intro h; rw [← h5]; exact pow_lt_pow_left₀ h (Real.rpow_nonneg hy _) (by norm_num)
  · intro h; rw [← h5] at h; exact lt_of_pow_lt_pow_left₀ 12 hc h

lemma le_rpow512 {c y : ℝ} (hc : 0 ≤ c) (hy : 0 ≤ y) : c ≤ y ^ (5 / 12 : ℝ) ↔ c ^ 12 ≤ y ^ 5 := by
  have h5 := rpow512_pow12 hy
  constructor
  · intro h; rw [← h5]; exact pow_le_pow_left₀ hc h 12
  · intro h; rw [← h5] at h; exact le_of_pow_le_pow_left₀ (by norm_num) (Real.rpow_nonneg hy _) h

lemma rpow512_le {c y : ℝ} (hc : 0 ≤ c) (hy : 0 ≤ y) : y ^ (5 / 12 : ℝ) ≤ c ↔ y ^ 5 ≤ c ^ 12 := by
  have h5 := rpow512_pow12 hy
  constructor
  · intro h; rw [← h5]; exact pow_le_pow_left₀ (Real.rpow_nonneg hy _) h 12
  · intro h; rw [← h5] at h; exact le_of_pow_le_pow_left₀ (by norm_num) hc h

lemma rpow_inv_cancel1 {u : ℝ} (hu : 0 ≤ u) : (u ^ (12 / 5 : ℝ)) ^ (5 / 12 : ℝ) = u := by
  rw [← Real.rpow_mul hu]; norm_num

lemma rpow_inv_cancel2 {y : ℝ} (hy : 0 ≤ y) : (y ^ (5 / 12 : ℝ)) ^ (12 / 5 : ℝ) = y := by
  rw [← Real.rpow_mul hy]; norm_num

end CraftSRGB
