import CraftIDCT.Bounds

/-! `BitVec 32` model = `Int` model whenever the inputs are bounded, so Rust's `i32` arithmetic never
wraps.  Each operation is lifted by a lemma carrying its no-overflow side condition; `omega` discharges
the side conditions from the interval bounds. -/

namespace CraftIDCT

theorem add_ok {x y : W} {a b : Int} (hx : x.toInt = a) (hy : y.toInt = b)
    (h : -2 ^ 31 ≤ a + b ∧ a + b < 2 ^ 31) : (x + y).toInt = a + b := by
  rw [BitVec.toInt_add, hx, hy]
  exact Int.bmod_eq_of_le (by push_cast; omega) (by push_cast; omega)

theorem sub_ok {x y : W} {a b : Int} (hx : x.toInt = a) (hy : y.toInt = b)
    (h : -2 ^ 31 ≤ a - b ∧ a - b < 2 ^ 31) : (x - y).toInt = a - b := by
  rw [BitVec.toInt_sub, hx, hy]
  exact Int.bmod_eq_of_le (by push_cast; omega) (by push_cast; omega)

theorem neg_ok {x : W} {a : Int} (hx : x.toInt = a)
    (h : -2 ^ 31 < a ∧ a < 2 ^ 31) : (-x).toInt = -a := by
  rw [BitVec.toInt_neg, hx]
  exact Int.bmod_eq_of_le (by push_cast; omega) (by push_cast; omega)

theorem sshr_ok {x : W} {a : Int} (n : Nat) (hx : x.toInt = a) :
    (x.sshiftRight n).toInt = a >>> n := by
  rw [BitVec.toInt_sshiftRight, hx]

/-- Lift an `Int`-model equation to `BitVec 32`, discharging each overflow side condition. -/
macro "lift_tac" : tactic => `(tactic| (
  repeat' first
    | (with_reducible apply add_ok)
    | (with_reducible apply sub_ok)
    | (with_reducible apply neg_ok)
    | (with_reducible apply sshr_ok)
    | ((try simp only [Int.shiftRight_eq_div_pow]); omega)
  all_goals try rfl))

theorem bf4v_toInt (M : Int) (hM2 : M ≤ 2 ^ 27) (x : Fin 4 → W)
    (hx : ∀ k, -M ≤ (x k).toInt ∧ (x k).toInt ≤ M) :
    ∀ i, (bf4v x i).toInt = bf4 (fun k => (x k).toInt) i := by
  have h0 := hx 0; have h1 := hx 1; have h2 := hx 2; have h3 := hx 3
  intro i
  fin_cases i <;> simp [bf4v, bf4b, bf4, -BitVec.toInt_add, -BitVec.toInt_sub, -BitVec.toInt_sshiftRight] <;> lift_tac

theorem bf8v_toInt (M : Int) (hM2 : M ≤ 2 ^ 25) (x : Fin 8 → W)
    (hx : ∀ k, -M ≤ (x k).toInt ∧ (x k).toInt ≤ M) :
    ∀ i, (bf8v x i).toInt = bf8 (fun k => (x k).toInt) i := by
  have h0 := hx 0; have h1 := hx 1; have h2 := hx 2; have h3 := hx 3
  have h4 := hx 4; have h5 := hx 5; have h6 := hx 6; have h7 := hx 7
  intro i
  fin_cases i <;> simp [bf8v, bf8b, bf8, -BitVec.toInt_add, -BitVec.toInt_sub, -BitVec.toInt_sshiftRight] <;> lift_tac

end CraftIDCT
