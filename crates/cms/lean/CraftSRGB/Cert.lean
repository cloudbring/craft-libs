import CraftSRGB.Bounds

/-!
Certificate checkers used by `Generate.lean`. `Real.rpow` is not computable, so the generator
finds candidate bounds with an *untrusted* integer root and then runs these decidable checks; the
theorems say that whenever a check passes, the true real value lies in the stated interval.
-/
namespace CraftSRGB

/-- `lo ≤ ((x+0.055)/1.055)^2.4 < lo + w`, as a decidable rational condition. -/
def DecodeOK (x lo w : ℚ) : Prop :=
  0 ≤ lo ∧ 0 ≤ w ∧ lo ^ 5 ≤ ((x + 11 / 200) / (211 / 200)) ^ 12 ∧ ((x + 11 / 200) / (211 / 200)) ^ 12 < (lo + w) ^ 5

instance (x lo w : ℚ) : Decidable (DecodeOK x lo w) := by unfold DecodeOK; infer_instance

theorem decodeOK_sound {x lo w : ℚ} (hx : 0 ≤ x) (h : DecodeOK x lo w) :
    (lo : ℝ) ≤ dr x ∧ dr x < (lo : ℝ) + w := by
  obtain ⟨h0, hw0, h1, h2⟩ := h
  have hx' : (0:ℝ) ≤ (x:ℝ) := by exact_mod_cast hx
  have hlo : (0:ℝ) ≤ (lo:ℝ) := by exact_mod_cast h0
  have hu : (0:ℝ) ≤ ((x:ℝ) + 11 / 200) / (211 / 200) := by positivity
  have h1' := (Rat.cast_le (K := ℝ)).2 h1
  have h2' := (Rat.cast_lt (K := ℝ)).2 h2
  push_cast at h1' h2'
  have hw : (0:ℝ) ≤ (lo:ℝ) + w := add_nonneg hlo (by exact_mod_cast hw0)
  exact ⟨(le_rpow125 hlo hu).2 h1', (rpow125_lt hw hu).2 h2'⟩

/-- `lo ≤ y^(5/12) < lo + w`, as a decidable rational condition. -/
def RootOK (y lo w : ℚ) : Prop :=
  0 ≤ lo ∧ 0 ≤ w ∧ lo ^ 12 ≤ y ^ 5 ∧ y ^ 5 < (lo + w) ^ 12

instance (y lo w : ℚ) : Decidable (RootOK y lo w) := by unfold RootOK; infer_instance

theorem rootOK_sound {y lo w : ℚ} (hy : 0 ≤ y) (h : RootOK y lo w) :
    (lo : ℝ) ≤ (y : ℝ) ^ (5 / 12 : ℝ) ∧ (y : ℝ) ^ (5 / 12 : ℝ) < (lo : ℝ) + w := by
  obtain ⟨h0, hw0, h1, h2⟩ := h
  have hy' : (0:ℝ) ≤ (y:ℝ) := by exact_mod_cast hy
  have hlo : (0:ℝ) ≤ (lo:ℝ) := by exact_mod_cast h0
  have h1' := (Rat.cast_le (K := ℝ)).2 h1
  have h2' := (Rat.cast_lt (K := ℝ)).2 h2
  push_cast at h1' h2'
  have hw : (0:ℝ) ≤ (lo:ℝ) + w := add_nonneg hlo (by exact_mod_cast hw0)
  exact ⟨(le_rpow512 hlo hy').2 h1', (rpow512_lt hw hy').2 h2'⟩

end CraftSRGB
