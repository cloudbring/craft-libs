import CraftIDCT.Transform

/-! How tight are the bounds?  Exact extremes of the row pass over the Rust box (proved by `omega`
and attained by explicit inputs), and attained values of the whole transform (these are witnesses,
i.e. lower bounds on the true maximum; the proved upper bounds are `Transform.lean`'s). -/

namespace CraftIDCT

private def box (v : Int) : Prop := -32768 ≤ v ∧ v ≤ 32767

/-- 4-point butterfly outputs over the Rust box are exactly within `[-114688, 114686]`. -/
theorem bf4_tight (x : Fin 4 → Int) (hx : ∀ k, box (x k)) :
    ∀ i, -114688 ≤ bf4 x i ∧ bf4 x i ≤ 114686 := by
  have h0 := hx 0; have h1 := hx 1; have h2 := hx 2; have h3 := hx 3
  simp only [box] at h0 h1 h2 h3
  intro i
  fin_cases i <;> simp [bf4] <;> simp only [Int.shiftRight_eq_div_pow] <;> omega

/-- Attained: `e - h` with `a = c = 32767`, `b = -32768`, `d = -32768` gives `114686`;
`e + h` with all `-32768` gives `-114688`. -/
theorem bf4_tight_attained :
    bf4 ![32767, -32768, 32767, -32768] 3 = 114686 ∧ bf4 ![-32768, -32768, -32768, -32768] 0 = -114688 := by
  decide +kernel

/-- 8-point butterfly outputs over the Rust box are within `[-241664, 241660]`. -/
theorem bf8_tight (x : Fin 8 → Int) (hx : ∀ k, box (x k)) :
    ∀ i, -241664 ≤ bf8 x i ∧ bf8 x i ≤ 241660 := by
  have h0 := hx 0; have h1 := hx 1; have h2 := hx 2; have h3 := hx 3
  have h4 := hx 4; have h5 := hx 5; have h6 := hx 6; have h7 := hx 7
  simp only [box] at h0 h1 h2 h3 h4 h5 h6 h7
  intro i
  fin_cases i <;> simp [bf8] <;> simp only [Int.shiftRight_eq_div_pow] <;> omega

theorem bf8_tight_attained :
    bf8 ![32767, 32767, 32767, -32768, -32768, -32768, -32768, -32768] 1 = 241660 ∧
    bf8 ![-32768, -32768, -32768, -32768, -32768, -32768, -32768, -32768] 0 = -241664 := by
  decide +kernel

/-- Whole 4x4 transform witnesses: top two rows `32767`, bottom two `-32768` reaches `401402` at
position `(1,0)`; the all-`-32768` matrix reaches `-401408` at `(0,0)`.  The proved bound is `±524288`
(`idct4_rust_bounds`), `i32` allows `2^31`. -/
theorem idct4_attained :
    cols bf4 (rows bf4 (fun i _ => if i.val < 2 then (32767 : Int) else -32768)) 1 0 = 401402 ∧
    cols bf4 (rows bf4 (fun _ _ => (-32768 : Int))) 0 0 = -401408 := by
  decide +kernel

/-- `i16` would NOT suffice: the row pass already leaves `[-32768, 32767]` on the Rust box. -/
theorem i16_would_overflow : ∃ x : Fin 4 → Int, (∀ k, box (x k)) ∧ 32767 < bf4 x 3 :=
  ⟨![32767, -32768, 32767, -32768], by
    intro k; fin_cases k <;> simp [box], by decide +kernel⟩

end CraftIDCT
