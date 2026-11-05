import CraftIDCT.Basic

/-! Interval (omega) bounds on the Int model: each 1-D butterfly maps inputs in `[-M, M]` to
outputs in `[-4M, 4M]` (4-point) or `[-(8M+1), 8M+1]` (8-point), and every internal value
(`e f g h`, `a_i`, `b_i`) stays within `i32`. -/

namespace CraftIDCT

/-- The range of `i32`. -/
def I32 (v : Int) : Prop := -2 ^ 31 ≤ v ∧ v < 2 ^ 31

/-- Every value computed inside `bf4` (including the four outputs). -/
def inter4 (x : Fin 4 → Int) : List Int :=
  let e := x 0 + x 2
  let f := x 0 - x 2
  let g := (x 1 >>> 1) - x 3
  let h := x 1 + (x 3 >>> 1)
  [x 1 >>> 1, x 3 >>> 1, e, f, g, h, e + h, f + g, f - g, e - h]

def inter8 (d : Fin 8 → Int) : List Int :=
  let a0 := d 0 + d 4
  let a4 := d 0 - d 4
  let a2 := (d 2 >>> 1) - d 6
  let a6 := d 2 + (d 6 >>> 1)
  let b0 := a0 + a6
  let b2 := a4 + a2
  let b4 := a4 - a2
  let b6 := a0 - a6
  let a1 := -d 3 + d 5 - d 7 - (d 7 >>> 1)
  let a3 := d 1 + d 7 - d 3 - (d 3 >>> 1)
  let a5 := -d 1 + d 7 + d 5 + (d 5 >>> 1)
  let a7 := d 3 + d 5 + d 1 + (d 1 >>> 1)
  let b1 := a1 + (a7 >>> 2)
  let b7 := a7 - (a1 >>> 2)
  let b3 := a3 + (a5 >>> 2)
  let b5 := (a3 >>> 2) - a5
  [a0, a4, a2, a6, b0, b2, b4, b6, a1, a3, a5, a7, b1, b7, b3, b5,
   b0 + b7, b2 + b5, b4 + b3, b6 + b1, b6 - b1, b4 - b3, b2 - b5, b0 - b7]

theorem bf4_bound (M : Int) (x : Fin 4 → Int) (hx : ∀ k, -M ≤ x k ∧ x k ≤ M) :
    ∀ i, -(4 * M) ≤ bf4 x i ∧ bf4 x i ≤ 4 * M := by
  have h0 := hx 0; have h1 := hx 1; have h2 := hx 2; have h3 := hx 3
  intro i
  fin_cases i <;> simp [bf4] <;> simp only [Int.shiftRight_eq_div_pow] <;> omega

theorem bf4_inter_i32 (M : Int) (hM2 : M ≤ 2 ^ 27) (x : Fin 4 → Int)
    (hx : ∀ k, -M ≤ x k ∧ x k ≤ M) : ∀ v ∈ inter4 x, I32 v := by
  have h0 := hx 0; have h1 := hx 1; have h2 := hx 2; have h3 := hx 3
  intro v hv
  simp only [inter4, List.mem_cons, List.not_mem_nil, or_false] at hv
  unfold I32
  simp only [Int.shiftRight_eq_div_pow] at hv
  rcases hv with rfl | rfl | rfl | rfl | rfl | rfl | rfl | rfl | rfl | rfl <;> omega

theorem bf8_bound (M : Int) (x : Fin 8 → Int) (hx : ∀ k, -M ≤ x k ∧ x k ≤ M) :
    ∀ i, -(8 * M + 1) ≤ bf8 x i ∧ bf8 x i ≤ 8 * M + 1 := by
  have h0 := hx 0; have h1 := hx 1; have h2 := hx 2; have h3 := hx 3
  have h4 := hx 4; have h5 := hx 5; have h6 := hx 6; have h7 := hx 7
  intro i
  fin_cases i <;> simp [bf8] <;> simp only [Int.shiftRight_eq_div_pow] <;> omega

theorem bf8_inter_i32 (M : Int) (hM2 : M ≤ 2 ^ 25) (x : Fin 8 → Int)
    (hx : ∀ k, -M ≤ x k ∧ x k ≤ M) : ∀ v ∈ inter8 x, I32 v := by
  have h0 := hx 0; have h1 := hx 1; have h2 := hx 2; have h3 := hx 3
  have h4 := hx 4; have h5 := hx 5; have h6 := hx 6; have h7 := hx 7
  intro v hv
  simp only [inter8, List.mem_cons, List.not_mem_nil, or_false] at hv
  unfold I32
  simp only [Int.shiftRight_eq_div_pow] at hv
  rcases hv with rfl | rfl | rfl | rfl | rfl | rfl | rfl | rfl | rfl | rfl | rfl | rfl | rfl | rfl |
    rfl | rfl | rfl | rfl | rfl | rfl | rfl | rfl | rfl | rfl <;> omega

end CraftIDCT
