import Mathlib.Tactic.NormNum
import Mathlib.Tactic.Linarith
import Mathlib.Tactic.Ring
import Mathlib.Tactic.SplitIfs
import Mathlib.Data.Real.Sqrt
import Mathlib.Data.Rat.Sqrt
import Mathlib.Data.Rat.Cast.Order

namespace CraftBlend

inductive Mode where
  | normal | multiply | screen | overlay | darken | lighten
  | colorDodge | colorBurn | hardLight | softLight | difference | exclusion
  deriving DecidableEq, Repr

def modes : List Mode := [.normal, .multiply, .screen, .overlay, .darken, .lighten,
  .colorDodge, .colorBurn, .hardLight, .softLight, .difference, .exclusion]

section Algebra

variable {K : Type*} [Field K] [LinearOrder K] [IsStrictOrderedRing K]

def UnitInterval (value : K) : Prop := 0 ≤ value ∧ value ≤ 1

def screen (backdrop source : K) : K := backdrop + source - backdrop * source

def hardLight (backdrop source : K) : K :=
  if source ≤ 1 / 2 then backdrop * (2 * source)
  else screen backdrop (2 * source - 1)

def softD (backdrop root : K) : K :=
  if backdrop ≤ 1 / 4 then ((16 * backdrop - 12) * backdrop + 4) * backdrop
  else root

def blend (mode : Mode) (backdrop source root : K) : K :=
  match mode with
  | .normal => source
  | .multiply => backdrop * source
  | .screen => screen backdrop source
  | .overlay => hardLight source backdrop
  | .darken => min backdrop source
  | .lighten => max backdrop source
  | .colorDodge => if backdrop = 0 then 0 else if source = 1 then 1
      else min 1 (backdrop / (1 - source))
  | .colorBurn => if backdrop = 1 then 1 else if source = 0 then 0
      else 1 - min 1 ((1 - backdrop) / source)
  | .hardLight => hardLight backdrop source
  | .softLight => if source ≤ 1 / 2 then
      backdrop - (1 - 2 * source) * backdrop * (1 - backdrop)
      else backdrop + (2 * source - 1) * (softD backdrop root - backdrop)
  | .difference => abs (backdrop - source)
  | .exclusion => backdrop + source - 2 * backdrop * source

def alpha (backdropAlpha sourceAlpha : K) : K :=
  sourceAlpha + backdropAlpha * (1 - sourceAlpha)

def premul (backdrop source backdropAlpha sourceAlpha blended : K) : K :=
  (1 - backdropAlpha) * sourceAlpha * source + backdropAlpha * sourceAlpha * blended +
    (1 - sourceAlpha) * backdropAlpha * backdrop

def straight (backdrop source backdropAlpha sourceAlpha blended : K) : K :=
  premul backdrop source backdropAlpha sourceAlpha blended / alpha backdropAlpha sourceAlpha

omit [IsStrictOrderedRing K] [LinearOrder K] in
theorem normalized_weights_eq (backdrop source backdropAlpha sourceAlpha blended : K) :
    (1 - backdropAlpha) * (sourceAlpha / alpha backdropAlpha sourceAlpha) * source +
      backdropAlpha * (sourceAlpha / alpha backdropAlpha sourceAlpha) * blended +
      (1 - sourceAlpha) * (backdropAlpha / alpha backdropAlpha sourceAlpha) * backdrop =
    straight backdrop source backdropAlpha sourceAlpha blended := by
  unfold straight premul
  simp only [div_eq_mul_inv]
  ring

theorem product_bounds {left right : K} (hl : UnitInterval left) (hr : UnitInterval right) :
    UnitInterval (left * right) := by
  exact ⟨mul_nonneg hl.1 hr.1, (mul_le_mul_of_nonneg_right hl.2 hr.1).trans (by simpa using hr.2)⟩

theorem screen_bounds {backdrop source : K}
    (hb : UnitInterval backdrop) (hs : UnitInterval source) :
    UnitInterval (screen backdrop source) := by
  have hp := product_bounds (show UnitInterval (1 - backdrop) by constructor <;> linarith [hb.1, hb.2])
    (show UnitInterval (1 - source) by constructor <;> linarith [hs.1, hs.2])
  unfold screen
  constructor <;> nlinarith [hp.1, hp.2]

theorem hardLight_bounds {backdrop source : K}
    (hb : UnitInterval backdrop) (hs : UnitInterval source) :
    UnitInterval (hardLight backdrop source) := by
  unfold hardLight
  split_ifs with h
  · apply product_bounds hb
    constructor <;> linarith [hs.1]
  · apply screen_bounds hb
    constructor <;> linarith [hs.2]

theorem softD_bounds {backdrop root : K} (hb : UnitInterval backdrop)
    (hr : backdrop ≤ root ∧ root ≤ 1) :
    backdrop ≤ softD backdrop root ∧ softD backdrop root ≤ 1 := by
  unfold softD
  split_ifs with h
  · have hp := mul_nonneg hb.1 (sq_nonneg (4 * backdrop - 1))
    have hq := mul_nonneg (show 0 ≤ 1 - 4 * backdrop by linarith) (sq_nonneg (2 * backdrop - 1))
    constructor <;> nlinarith [hb.1, hb.2]
  · exact hr

theorem blend_bounds (mode : Mode) {backdrop source root : K}
    (hb : UnitInterval backdrop) (hs : UnitInterval source)
    (hr : backdrop ≤ root ∧ root ≤ 1) :
    UnitInterval (blend mode backdrop source root) := by
  have hmul := product_bounds hb hs
  cases mode with
  | normal => exact hs
  | multiply => exact hmul
  | screen => exact screen_bounds hb hs
  | overlay => exact hardLight_bounds hs hb
  | darken => exact ⟨le_min hb.1 hs.1, (min_le_left _ _).trans hb.2⟩
  | lighten => exact ⟨hb.1.trans (le_max_left _ _), max_le hb.2 hs.2⟩
  | colorDodge =>
    simp only [blend]
    split_ifs with hzero hone
    · exact ⟨le_rfl, zero_le_one⟩
    · exact ⟨zero_le_one, le_rfl⟩
    · exact ⟨le_min zero_le_one (div_nonneg hb.1 (by linarith [hs.2])), min_le_left _ _⟩
  | colorBurn =>
    simp only [blend]
    split_ifs with hone hzero
    · exact ⟨zero_le_one, le_rfl⟩
    · exact ⟨le_rfl, zero_le_one⟩
    · have hlo : 0 ≤ min 1 ((1 - backdrop) / source) :=
        le_min zero_le_one (div_nonneg (by linarith [hb.2]) hs.1)
      have hhi := min_le_left (1 : K) ((1 - backdrop) / source)
      constructor <;> linarith
  | hardLight => exact hardLight_bounds hb hs
  | softLight =>
    simp only [blend]
    split_ifs with h
    · have hfactor : UnitInterval (1 - 2 * source) := by constructor <;> linarith [hs.1]
      have hprod := product_bounds hfactor
        (show UnitInterval (1 - backdrop) by constructor <;> linarith [hb.1, hb.2])
      have hlo := mul_nonneg hb.1 (show 0 ≤ 1 - (1 - 2 * source) * (1 - backdrop) by linarith [hprod.2])
      have hhi := mul_nonneg hb.1 hprod.1
      constructor <;> nlinarith [hb.2]
    · have hd := softD_bounds hb hr
      have hlo := mul_nonneg (show 0 ≤ 2 * source - 1 by linarith) (sub_nonneg.mpr hd.1)
      have hhi := mul_nonneg (show 0 ≤ 1 - (2 * source - 1) by linarith [hs.2]) (sub_nonneg.mpr hd.1)
      constructor <;> nlinarith [hb.1, hd.2]
  | difference =>
    exact ⟨abs_nonneg _, abs_le.mpr ⟨by linarith [hb.1, hs.2], by linarith [hb.2, hs.1]⟩⟩
  | exclusion =>
    have hlo := mul_nonneg hb.1 (show 0 ≤ 1 - source by linarith [hs.2])
    have hhi := mul_nonneg hs.1 (show 0 ≤ 1 - backdrop by linarith [hb.2])
    have hrest := mul_nonneg (show 0 ≤ 1 - backdrop by linarith [hb.2])
      (show 0 ≤ 1 - source by linarith [hs.2])
    constructor <;> dsimp [blend] <;> nlinarith [hmul.1]

theorem alpha_bounds {backdropAlpha sourceAlpha : K}
    (hb : UnitInterval backdropAlpha) (hs : UnitInterval sourceAlpha) :
    UnitInterval (alpha backdropAlpha sourceAlpha) := by
  have hp := product_bounds hb
    (show UnitInterval (1 - sourceAlpha) by constructor <;> linarith [hs.1, hs.2])
  have hu := mul_nonneg (show 0 ≤ 1 - backdropAlpha by linarith [hb.2])
    (show 0 ≤ 1 - sourceAlpha by linarith [hs.2])
  constructor <;> dsimp [alpha] <;> nlinarith [hs.1, hp.1]

theorem premul_bounds {backdrop source backdropAlpha sourceAlpha blended : K}
    (hb : UnitInterval backdrop) (hs : UnitInterval source)
    (hba : UnitInterval backdropAlpha) (hsa : UnitInterval sourceAlpha)
    (hm : UnitInterval blended) :
    0 ≤ premul backdrop source backdropAlpha sourceAlpha blended ∧
    premul backdrop source backdropAlpha sourceAlpha blended ≤ alpha backdropAlpha sourceAlpha := by
  have hw₁ : 0 ≤ (1 - backdropAlpha) * sourceAlpha := mul_nonneg (by linarith [hba.2]) hsa.1
  have hw₂ : 0 ≤ backdropAlpha * sourceAlpha := mul_nonneg hba.1 hsa.1
  have hw₃ : 0 ≤ (1 - sourceAlpha) * backdropAlpha := mul_nonneg (by linarith [hsa.2]) hba.1
  have hlo₁ := mul_nonneg hw₁ hs.1
  have hlo₂ := mul_nonneg hw₂ hm.1
  have hlo₃ := mul_nonneg hw₃ hb.1
  have hhi₁ := mul_le_mul_of_nonneg_left hs.2 hw₁
  have hhi₂ := mul_le_mul_of_nonneg_left hm.2 hw₂
  have hhi₃ := mul_le_mul_of_nonneg_left hb.2 hw₃
  constructor <;> dsimp [premul, alpha] <;> nlinarith

theorem straight_bounds {backdrop source backdropAlpha sourceAlpha blended : K}
    (hb : UnitInterval backdrop) (hs : UnitInterval source)
    (hba : UnitInterval backdropAlpha) (hsa : UnitInterval sourceAlpha)
    (hm : UnitInterval blended) :
    UnitInterval (straight backdrop source backdropAlpha sourceAlpha blended) := by
  have hp := premul_bounds hb hs hba hsa hm
  have ha := alpha_bounds hba hsa
  by_cases hz : alpha backdropAlpha sourceAlpha = 0
  · simp [straight, hz, UnitInterval]
  · exact ⟨div_nonneg hp.1 ha.1, (div_le_one (lt_of_le_of_ne ha.1 (Ne.symm hz))).mpr hp.2⟩

theorem all_modes_bounds (mode : Mode) {backdrop source backdropAlpha sourceAlpha root : K}
    (hb : UnitInterval backdrop) (hs : UnitInterval source)
    (hba : UnitInterval backdropAlpha) (hsa : UnitInterval sourceAlpha)
    (hr : backdrop ≤ root ∧ root ≤ 1) :
    UnitInterval (alpha backdropAlpha sourceAlpha) ∧
    UnitInterval (straight backdrop source backdropAlpha sourceAlpha (blend mode backdrop source root)) :=
  ⟨alpha_bounds hba hsa, straight_bounds hb hs hba hsa (blend_bounds mode hb hs hr)⟩

omit [IsStrictOrderedRing K] [LinearOrder K] in
theorem transparent_source_premul (backdrop source backdropAlpha blended : K) :
    premul backdrop source backdropAlpha 0 blended = backdropAlpha * backdrop ∧
      alpha backdropAlpha 0 = backdropAlpha := by
  simp [premul, alpha]

omit [IsStrictOrderedRing K] [LinearOrder K] in
theorem transparent_backdrop_premul (backdrop source sourceAlpha blended : K) :
    premul backdrop source 0 sourceAlpha blended = sourceAlpha * source ∧
      alpha 0 sourceAlpha = sourceAlpha := by
  simp [premul, alpha]

omit [IsStrictOrderedRing K] in
theorem transparent_source (backdrop source backdropAlpha blended : K)
    (canonical : backdropAlpha = 0 → backdrop = 0) :
    straight backdrop source backdropAlpha 0 blended = backdrop ∧ alpha backdropAlpha 0 = backdropAlpha := by
  by_cases hz : backdropAlpha = 0
  · simp [straight, premul, alpha, hz, canonical hz]
  · simp [straight, premul, alpha, mul_div_cancel_left₀ _ hz]

omit [IsStrictOrderedRing K] in
theorem transparent_backdrop (backdrop source sourceAlpha blended : K)
    (canonical : sourceAlpha = 0 → source = 0) :
    straight backdrop source 0 sourceAlpha blended = source ∧ alpha 0 sourceAlpha = sourceAlpha := by
  by_cases hz : sourceAlpha = 0
  · simp [straight, premul, alpha, hz, canonical hz]
  · simp [straight, premul, alpha, mul_div_cancel_left₀ _ hz]

omit [IsStrictOrderedRing K] in
theorem opaque_normal (backdrop source backdropAlpha root : K) :
    straight backdrop source backdropAlpha 1 (blend .normal backdrop source root) = source ∧
      alpha backdropAlpha 1 = 1 := by
  simp [straight, premul, alpha, blend]
  ring

def oldTexturecraft (backdrop source backdropAlpha sourceAlpha blended : K) : K :=
  if sourceAlpha ≤ 0 then backdrop
  else let mixed := if 0 < backdropAlpha then blended else source
       backdrop + (mixed - backdrop) * (sourceAlpha / max (alpha backdropAlpha sourceAlpha) (1 / 100000))

end Algebra

def sqrtQ (value : ℚ) : Option ℚ :=
  let root : ℚ := (value.num.natAbs.sqrt : ℚ) / (value.den.sqrt : ℚ)
  if 0 ≤ root ∧ root * root = value then some root else none

def blendQ (mode : Mode) (backdrop source : ℚ) : Option ℚ :=
  if mode = .softLight ∧ 1 / 2 < source ∧ 1 / 4 < backdrop then
    (sqrtQ backdrop).map fun root => blend mode backdrop source root
  else some (blend mode backdrop source 1)

theorem sqrtQ_sound {value root : ℚ} (h : sqrtQ value = some root) :
    0 ≤ root ∧ root * root = value := by
  dsimp only [sqrtQ] at h
  split_ifs at h with hc
  · cases Option.some.inj h
    exact hc

theorem root_bounds {value root : ℚ} (hv : UnitInterval value)
    (hr : 0 ≤ root ∧ root * root = value) : value ≤ root ∧ root ≤ 1 := by
  constructor <;> nlinarith [hv.1, hv.2, hr.1, hr.2, sq_nonneg (root - 1)]

theorem rational_blend_bounds (mode : Mode) {backdrop source result : ℚ}
    (hb : UnitInterval backdrop) (hs : UnitInterval source)
    (he : blendQ mode backdrop source = some result) : UnitInterval result := by
  unfold blendQ at he
  split_ifs at he with hc
  · cases hroot : sqrtQ backdrop with
    | none => simp [hroot] at he
    | some root =>
      simp only [hroot, Option.map_some, Option.some.injEq] at he
      rw [← he]
      exact blend_bounds mode hb hs (root_bounds hb (sqrtQ_sound hroot))
  · cases Option.some.inj he
    exact blend_bounds mode hb hs ⟨hb.2, le_rfl⟩

theorem rational_all_modes_bounds (mode : Mode) {backdrop source backdropAlpha sourceAlpha result : ℚ}
    (hb : UnitInterval backdrop) (hs : UnitInterval source)
    (hba : UnitInterval backdropAlpha) (hsa : UnitInterval sourceAlpha)
    (he : blendQ mode backdrop source = some result) :
    UnitInterval (alpha backdropAlpha sourceAlpha) ∧
    UnitInterval (straight backdrop source backdropAlpha sourceAlpha result) :=
  ⟨alpha_bounds hba hsa, straight_bounds hb hs hba hsa (rational_blend_bounds mode hb hs he)⟩

noncomputable def blendReal (mode : Mode) (backdrop source : ℝ) : ℝ :=
  blend mode backdrop source (Real.sqrt backdrop)

theorem cast_ite (condition : Prop) [Decidable condition] (left right : ℚ) :
    ((if condition then left else right : ℚ) : ℝ) =
      if condition then (left : ℝ) else (right : ℝ) := by
  split_ifs <;> rfl

theorem cast_eq_one (value : ℚ) : ((value : ℝ) = 1) ↔ value = 1 := by norm_cast

theorem cast_le_half (value : ℚ) : ((value : ℝ) ≤ 1 / 2) ↔ value ≤ 1 / 2 := by
  simpa only [Rat.cast_div, Rat.cast_one, Rat.cast_ofNat] using
    (Rat.cast_le (K := ℝ) (p := value) (q := 1 / 2))

theorem cast_le_quarter (value : ℚ) : ((value : ℝ) ≤ 1 / 4) ↔ value ≤ 1 / 4 := by
  simpa only [Rat.cast_div, Rat.cast_one, Rat.cast_ofNat] using
    (Rat.cast_le (K := ℝ) (p := value) (q := 1 / 4))

theorem cast_blend (mode : Mode) (backdrop source root : ℚ) :
    ((blend mode backdrop source root : ℚ) : ℝ) =
      blend mode (backdrop : ℝ) (source : ℝ) (root : ℝ) := by
  cases mode <;> dsimp only [blend, screen, hardLight, softD] <;>
    simp only [cast_ite, Rat.cast_add, Rat.cast_sub, Rat.cast_mul, Rat.cast_div,
      Rat.cast_min, Rat.cast_max, Rat.cast_abs, Rat.cast_zero, Rat.cast_one,
      Rat.cast_ofNat, cast_le_half, cast_le_quarter, Rat.cast_eq_zero, cast_eq_one]

theorem rational_blend_agrees_real (mode : Mode) {backdrop source result : ℚ}
    (hb : UnitInterval backdrop)
    (he : blendQ mode backdrop source = some result) :
    (result : ℝ) = blendReal mode (backdrop : ℝ) (source : ℝ) := by
  unfold blendQ at he
  split_ifs at he with hc
  · cases hroot : sqrtQ backdrop with
    | none => simp [hroot] at he
    | some root =>
      simp only [hroot, Option.map_some, Option.some.injEq] at he
      have hr := sqrtQ_sound hroot
      have hsqrt : Real.sqrt (backdrop : ℝ) = (root : ℝ) := by
        apply (Real.sqrt_eq_iff_mul_self_eq (by exact_mod_cast hb.1) (by exact_mod_cast hr.1)).mpr
        exact_mod_cast hr.2.symm
      rw [← he, cast_blend]
      simp [blendReal, hsqrt]
  · cases Option.some.inj he
    rw [cast_blend]
    unfold blendReal
    cases mode <;> simp_all [blend]
    by_cases hs : source ≤ 1 / 2
    · have hsReal : (source : ℝ) ≤ 1 / 2 := (cast_le_half source).mpr hs
      simp [show (source : ℝ) ≤ 2⁻¹ by simpa using hsReal]
    · have hbq : backdrop ≤ 1 / 4 := by
        by_contra hnot
        have := hc (by linarith)
        linarith
      have hbReal : (backdrop : ℝ) ≤ 1 / 4 := (cast_le_quarter backdrop).mpr hbq
      simp [softD, show (backdrop : ℝ) ≤ 4⁻¹ by simpa using hbReal]

theorem no_rational_sqrt_half : ¬ ∃ root : ℚ, root * root = 1 / 2 := by
  have hn : (1 / 2 : ℚ).num = 1 := Rat.num_div_eq_of_coprime (by norm_num) (by norm_num)
  have hdInt : ((1 / 2 : ℚ).den : ℤ) = 2 :=
    Rat.den_div_eq_of_coprime (a := 1) (b := 2) (by norm_num) (by norm_num)
  have hd : (1 / 2 : ℚ).den = 2 := by exact_mod_cast hdInt
  rw [Rat.exists_mul_self]
  norm_num [Rat.sqrt, hn, hd, Int.sqrt, Nat.sqrt_two]

theorem real_all_modes_bounds (mode : Mode) {backdrop source backdropAlpha sourceAlpha : ℝ}
    (hb : UnitInterval backdrop) (hs : UnitInterval source)
    (hba : UnitInterval backdropAlpha) (hsa : UnitInterval sourceAlpha) :
    UnitInterval (alpha backdropAlpha sourceAlpha) ∧
    UnitInterval (straight backdrop source backdropAlpha sourceAlpha (blendReal mode backdrop source)) := by
  have hroot := Real.sq_sqrt hb.1
  have hnonneg := Real.sqrt_nonneg backdrop
  apply all_modes_bounds mode hb hs hba hsa
  constructor <;> nlinarith [hb.1, hb.2, sq_nonneg (Real.sqrt backdrop - 1)]

theorem ticket_witness :
    straight (1 / 2 : ℚ) (4 / 5) (1 / 2) (1 / 2) (blend .multiply (1 / 2) (4 / 5) 1) = 17 / 30 ∧
    alpha (1 / 2 : ℚ) (1 / 2) = 3 / 4 := by
  norm_num [straight, premul, alpha, blend]

theorem old_ticket_witness :
    oldTexturecraft (1 / 2 : ℚ) (4 / 5) (1 / 2) (1 / 2) (blend .multiply (1 / 2) (4 / 5) 1) = 13 / 30 := by
  norm_num [oldTexturecraft, alpha, blend]

theorem old_differs_from_spec :
    oldTexturecraft (1 / 2 : ℚ) (4 / 5) (1 / 2) (1 / 2) (blend .multiply (1 / 2) (4 / 5) 1) ≠
    straight (1 / 2 : ℚ) (4 / 5) (1 / 2) (1 / 2) (blend .multiply (1 / 2) (4 / 5) 1) := by
  rw [old_ticket_witness, ticket_witness.1]
  norm_num

end CraftBlend
