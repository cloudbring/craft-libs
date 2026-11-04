import Mathlib
import CraftBlur.Dist

/-!
# Three-box width selection (Kovesi) as implemented by `craft_blur::box_widths`

Exact rational model.  `lowerWidth σ` is `⌊√(4σ²+1)⌋` forced odd (and at least 1);
`count σ` is the number of boxes (of three) that get the lower width, the rest get `lower + 2`.
-/

namespace CraftBlur

/-- `⌊√(4σ²+1)⌋` via the integer square root of `⌊4σ²+1⌋` (exact: `⌊√q⌋ = isqrt ⌊q⌋`),
then `s - 1` when even, and at least 1. -/
def oddFloor (s : ℕ) : ℕ := max (if s % 2 = 0 then s - 1 else s) 1

def lowerWidth (σ : ℚ) : ℕ := oddFloor (Nat.sqrt ⌊4 * σ ^ 2 + 1⌋₊)

/-- The real-valued number of lower-width boxes that would hit `σ²` exactly. -/
def mStar (σ : ℚ) : ℚ :=
  (12 * σ ^ 2 - 3 * (lowerWidth σ : ℚ) ^ 2 - 12 * (lowerWidth σ : ℚ) - 9) /
    (-4 * (lowerWidth σ : ℚ) - 4)

/-- `m* = (3l² + 12l + 9 - 12σ²) / (4l + 4)`. -/
lemma mStar_eq (σ : ℚ) :
    mStar σ = (3 * (lowerWidth σ : ℚ) ^ 2 + 12 * lowerWidth σ + 9 - 12 * σ ^ 2) /
      (4 * lowerWidth σ + 4) := by
  unfold mStar
  rw [← neg_div_neg_eq]; congr 1 <;> ring

/-- `round(m*)` (half up; `m* > 0` is proved below so this is half-away-from-zero). -/
def count (σ : ℚ) : ℕ := ⌊mStar σ + 1 / 2⌋₊

/-- The three widths, in cascade order. -/
def widths (σ : ℚ) : List ℕ :=
  (List.range 3).map fun (i : ℕ) => if i < count σ then lowerWidth σ else lowerWidth σ + 2

lemma oddFloor_odd (s : ℕ) : oddFloor s % 2 = 1 := by
  unfold oddFloor
  split_ifs with h <;> omega

lemma oddFloor_le (s : ℕ) (hs : 0 < s) : oddFloor s ≤ s := by
  unfold oddFloor
  split_ifs with h <;> omega

lemma succ_le_oddFloor (s : ℕ) (hs : 0 < s) : s + 1 ≤ oddFloor s + 2 := by
  unfold oddFloor
  split_ifs with h <;> omega

lemma lowerWidth_odd (σ : ℚ) : lowerWidth σ % 2 = 1 := oddFloor_odd _

lemma sqrt_pos' {σ : ℚ} : 0 < Nat.sqrt ⌊4 * σ ^ 2 + 1⌋₊ := by
  apply Nat.sqrt_pos.2
  apply Nat.floor_pos.2
  nlinarith [sq_nonneg σ]

lemma lower_le_sqrt (σ : ℚ) : lowerWidth σ ≤ Nat.sqrt ⌊4 * σ ^ 2 + 1⌋₊ :=
  oddFloor_le _ (@sqrt_pos' σ)

lemma sqrt_succ_le_upper (σ : ℚ) : Nat.sqrt ⌊4 * σ ^ 2 + 1⌋₊ + 1 ≤ lowerWidth σ + 2 :=
  succ_le_oddFloor _ (@sqrt_pos' σ)

/-- `l² ≤ 4σ² + 1`. -/
theorem lower_sq_le (σ : ℚ) : ((lowerWidth σ : ℚ)) ^ 2 ≤ 4 * σ ^ 2 + 1 := by
  have h1 := lower_le_sqrt σ
  have h2 : Nat.sqrt ⌊4 * σ ^ 2 + 1⌋₊ ^ 2 ≤ ⌊4 * σ ^ 2 + 1⌋₊ := Nat.sqrt_le' _
  have h3 : lowerWidth σ ^ 2 ≤ ⌊4 * σ ^ 2 + 1⌋₊ :=
    le_trans (Nat.pow_le_pow_left h1 2) h2
  have h4 : ((⌊4 * σ ^ 2 + 1⌋₊ : ℕ) : ℚ) ≤ 4 * σ ^ 2 + 1 := Nat.floor_le (by nlinarith [sq_nonneg σ])
  have h5 : ((lowerWidth σ ^ 2 : ℕ) : ℚ) ≤ ((⌊4 * σ ^ 2 + 1⌋₊ : ℕ) : ℚ) := by exact_mod_cast h3
  push_cast at h5
  linarith

/-- `4σ² + 1 < (l + 2)²`. -/
theorem lt_upper_sq (σ : ℚ) : 4 * σ ^ 2 + 1 < ((lowerWidth σ : ℚ) + 2) ^ 2 := by
  have h1 := sqrt_succ_le_upper σ
  have h2 : ⌊4 * σ ^ 2 + 1⌋₊ < (Nat.sqrt ⌊4 * σ ^ 2 + 1⌋₊ + 1) ^ 2 := Nat.lt_succ_sqrt' _
  have h3 : ⌊4 * σ ^ 2 + 1⌋₊ < (lowerWidth σ + 2) ^ 2 :=
    lt_of_lt_of_le h2 (Nat.pow_le_pow_left h1 2)
  have h4 : 4 * σ ^ 2 + 1 < ((⌊4 * σ ^ 2 + 1⌋₊ : ℕ) : ℚ) + 1 := Nat.lt_floor_add_one _
  have h5 : ((⌊4 * σ ^ 2 + 1⌋₊ : ℕ) : ℚ) + 1 ≤ (((lowerWidth σ + 2) ^ 2 : ℕ) : ℚ) := by
    exact_mod_cast h3
  push_cast at h5
  linarith

lemma lower_ge_one (σ : ℚ) : (1 : ℚ) ≤ lowerWidth σ := by
  have : 1 ≤ lowerWidth σ := by unfold lowerWidth; exact le_max_right _ _
  exact_mod_cast this

/-- `m* ∈ (0, 3]`: some, but at most all three, boxes take the lower width. -/
theorem mStar_bounds (σ : ℚ) : 0 < mStar σ ∧ mStar σ ≤ 3 := by
  have h1 := lower_sq_le σ
  have h2 := lt_upper_sq σ
  have hl := lower_ge_one σ
  rw [mStar_eq]
  generalize (lowerWidth σ : ℚ) = l at *
  have hd : (0 : ℚ) < 4 * l + 4 := by linarith
  constructor
  · apply div_pos _ hd; nlinarith
  · rw [div_le_iff₀ hd]; nlinarith

theorem count_le_three (σ : ℚ) : count σ ≤ 3 := by
  obtain ⟨h0, h3⟩ := mStar_bounds σ
  unfold count
  have : ⌊mStar σ + 1 / 2⌋₊ ≤ ⌊(3 : ℚ) + 1 / 2⌋₊ := Nat.floor_le_floor (by linarith)
  have h7 : ⌊(3 : ℚ) + 1 / 2⌋₊ = 3 := by
    rw [Nat.floor_eq_iff (by norm_num)]; norm_num
  omega

/-- All box widths are odd. -/
theorem widths_odd (σ : ℚ) : ∀ w ∈ widths σ, w % 2 = 1 := by
  intro w hw
  have := lowerWidth_odd σ
  unfold widths at hw
  simp only [List.mem_map, List.mem_range] at hw
  obtain ⟨i, _, rfl⟩ := hw
  split_ifs <;> omega

theorem widthVar_widths (σ : ℚ) :
    widthVar (widths σ) =
      (3 * ((lowerWidth σ : ℚ) ^ 2 - 1) + (3 - (count σ : ℚ)) * (4 * lowerWidth σ + 4)) / 12 := by
  have h := count_le_three σ
  unfold widths widthVar
  generalize count σ = m at *
  generalize lowerWidth σ = l at *
  interval_cases m <;> simp [List.range_succ] <;> ring

/-- The exact variance target is hit by the real-valued `m*`. -/
theorem target_eq_mStar (σ : ℚ) :
    σ ^ 2 = (3 * ((lowerWidth σ : ℚ) ^ 2 - 1) + (3 - mStar σ) * (4 * lowerWidth σ + 4)) / 12 := by
  have hl := lower_ge_one σ
  rw [mStar_eq]
  generalize (lowerWidth σ : ℚ) = l at *
  have hd : (4 * l + 4) ≠ 0 := by linarith
  field_simp
  ring

/-- **Width-selection accuracy.**  The cascade variance is within `(l+1)/6` of `σ²`
(`l` the lower width), because `count` rounds `m*` by at most one half. -/
theorem variance_error_le (σ : ℚ) :
    |widthVar (widths σ) - σ ^ 2| ≤ ((lowerWidth σ : ℚ) + 1) / 6 := by
  have hl := lower_ge_one σ
  have hm := count_le_three σ
  obtain ⟨h0, _⟩ := mStar_bounds σ
  have hfl := Nat.floor_le (show 0 ≤ mStar σ + 1 / 2 by linarith)
  have hlt := Nat.lt_floor_add_one (mStar σ + 1 / 2)
  have e1 := widthVar_widths σ
  have e2 := target_eq_mStar σ
  unfold count at e1 hfl
  rw [e1, e2]
  generalize (lowerWidth σ : ℚ) = l at *
  generalize (⌊mStar σ + 1 / 2⌋₊ : ℚ) = m at *
  generalize mStar σ = ms at *
  rw [abs_le]
  constructor <;> nlinarith

/-- `l ≤ 2σ + 1` for `σ ≥ 0`. -/
theorem lower_le (σ : ℚ) (hσ : 0 ≤ σ) : (lowerWidth σ : ℚ) ≤ 2 * σ + 1 := by
  have h := lower_sq_le σ
  have hl := lower_ge_one σ
  nlinarith

/-- **Relative error of the cascade**: `|σ_eff²/σ² - 1| ≤ (σ+1)/(3σ²) = (1/(3σ))(1 + 1/σ)`,
the `~1/(3σ)` under-blur of every integer-box Gaussian. -/
theorem relative_error_le (σ : ℚ) (hσ : 0 < σ) :
    |widthVar (widths σ) / σ ^ 2 - 1| ≤ (σ + 1) / (3 * σ ^ 2) := by
  have h1 := variance_error_le σ
  have h2 := lower_le σ hσ.le
  have hs : 0 < σ ^ 2 := by positivity
  have : widthVar (widths σ) / σ ^ 2 - 1 = (widthVar (widths σ) - σ ^ 2) / σ ^ 2 := by
    field_simp
  rw [this, abs_div, abs_of_pos hs, div_le_div_iff₀ hs (by positivity)]
  have : |widthVar (widths σ) - σ ^ 2| ≤ (σ + 1) / 3 := by linarith
  calc |widthVar (widths σ) - σ ^ 2| * (3 * σ ^ 2) ≤ (σ + 1) / 3 * (3 * σ ^ 2) := by gcongr
    _ = (σ + 1) * σ ^ 2 := by ring

/-! ### Integer sigma: the `1/(3σ)` deficit is exact -/

lemma lowerWidth_nat (n : ℕ) (hn : 1 ≤ n) : lowerWidth (n : ℚ) = 2 * n - 1 := by
  have h1 : (4 * (n : ℚ) ^ 2 + 1) = ((4 * n ^ 2 + 1 : ℕ) : ℚ) := by push_cast; ring
  have h2 : Nat.sqrt (4 * n ^ 2 + 1) = 2 * n := by
    symm
    rw [Nat.eq_sqrt]
    constructor <;> nlinarith
  unfold lowerWidth
  rw [h1, Nat.floor_natCast, h2]
  unfold oddFloor
  have : 2 * n % 2 = 0 := by omega
  rw [if_pos this]
  omega

/-- For every integer `σ = n ≥ 1` the three widths are `2n-1, 2n-1, 2n+1`, the variance is exactly
`n² - n/3`, and so the relative deficit is exactly `1/(3n)`. -/
theorem integer_sigma (n : ℕ) (hn : 1 ≤ n) :
    widths (n : ℚ) = [2 * n - 1, 2 * n - 1, 2 * n + 1] ∧
    widthVar (widths (n : ℚ)) = (n : ℚ) ^ 2 - n / 3 ∧
    widthVar (widths (n : ℚ)) / (n : ℚ) ^ 2 - 1 = -(1 / (3 * (n : ℚ))) := by
  have hl := lowerWidth_nat n hn
  have hcast : ((lowerWidth (n : ℚ) : ℕ) : ℚ) = 2 * n - 1 := by
    rw [hl]; push_cast [Nat.cast_sub (by omega : 1 ≤ 2 * n)]; ring
  have hm : mStar (n : ℚ) = 3 / 2 := by
    rw [mStar_eq, hcast]
    have : (n : ℚ) ≠ 0 := by positivity
    field_simp
    ring
  have hc : count (n : ℚ) = 2 := by
    unfold count
    rw [hm, Nat.floor_eq_iff (by norm_num)]
    norm_num
  have hw : widths (n : ℚ) = [2 * n - 1, 2 * n - 1, 2 * n + 1] := by
    unfold widths
    rw [hc, hl]
    simp [List.range_succ]
    omega
  have hv : widthVar (widths (n : ℚ)) = (n : ℚ) ^ 2 - n / 3 := by
    rw [widthVar_widths, hc, hcast]; push_cast; ring
  refine ⟨hw, hv, ?_⟩
  rw [hv]
  have : (n : ℚ) ≠ 0 := by positivity
  field_simp
  ring

end CraftBlur
