import CraftIDCT.Exact

/-! Whole-transform theorems.  Coefficient bound: H.264 (03/2010+) 8.5.12.2 / 8.5.13.2 require the
dequantised coefficients `d_ij` of an 8-bit-depth stream to lie in `[-2^(7+bitDepth), 2^(7+bitDepth)-1]`
= `[-2^15, 2^15 - 1]`, and `idct4_add` / `idct8_add` additionally clamp to exactly that range (the Rust
`coefficient.clamp(-32768, 32767)`).  The general theorems take any `|d| ≤ M` with `M ≤ 2^21`
(`bitDepth = 14`, the largest H.264 allows), of which the Rust range is the case `M = 2^15`. -/

namespace CraftIDCT

theorem rndv_toInt (v : W) (a : Int) (hv : v.toInt = a) (h : -2 ^ 31 ≤ a + 32 ∧ a + 32 < 2 ^ 31) :
    (rndv v).toInt = rnd a := by
  unfold rndv rndb rnd
  apply sshr_ok
  exact add_ok hv (by decide) h

/-- 4x4: `Int` model = `i32` model, for every coefficient matrix with entries in `[-M, M]`, `M ≤ 2^21`. -/
theorem idct4v_toInt (M : Int) (hM : M ≤ 2 ^ 21) (d : Fin 4 → Fin 4 → W)
    (hd : ∀ i j, -M ≤ (d i j).toInt ∧ (d i j).toInt ≤ M) :
    ∀ i j, (idct4v d i j).toInt = idct4 (fun i j => (d i j).toInt) i j := by
  intro i j
  have hrow : ∀ i j, (rowsv bf4v d i j).toInt = rows bf4 (fun i j => (d i j).toInt) i j :=
    fun i j => bf4v_toInt M (by omega) (d i) (hd i) j
  have hrb : ∀ i j, -(4 * M) ≤ (rowsv bf4v d i j).toInt ∧ (rowsv bf4v d i j).toInt ≤ 4 * M := by
    intro i j; rw [hrow]; exact bf4_bound M (fun k => (d i k).toInt) (hd i) j
  have hcol : (colsv bf4v (rowsv bf4v d) i j).toInt =
      cols bf4 (rows bf4 (fun i j => (d i j).toInt)) i j := by
    have := bf4v_toInt (4 * M) (by omega) (fun k => rowsv bf4v d k j) (fun k => hrb k j) i
    have e : (fun k => (rowsv bf4v d k j).toInt) = fun k => rows bf4 (fun i j => (d i j).toInt) k j :=
      funext fun k => hrow k j
    rw [e] at this
    exact this
  have hb := bf4_bound (4 * M) (fun k => rows bf4 (fun i j => (d i j).toInt) k j)
    (fun k => by have := hrb k j; rw [hrow] at this; exact this) i
  unfold idct4v idct4b idct4
  apply rndv_toInt _ _ hcol
  simp only [cols] at hb ⊢
  omega

/-- 8x8, likewise. -/
theorem idct8v_toInt (M : Int) (hM : M ≤ 2 ^ 21) (d : Fin 8 → Fin 8 → W)
    (hd : ∀ i j, -M ≤ (d i j).toInt ∧ (d i j).toInt ≤ M) :
    ∀ i j, (idct8v d i j).toInt = idct8 (fun i j => (d i j).toInt) i j := by
  intro i j
  have hrow : ∀ i j, (rowsv bf8v d i j).toInt = rows bf8 (fun i j => (d i j).toInt) i j :=
    fun i j => bf8v_toInt M (by omega) (d i) (hd i) j
  have hrb : ∀ i j, -(8 * M + 1) ≤ (rowsv bf8v d i j).toInt ∧ (rowsv bf8v d i j).toInt ≤ 8 * M + 1 := by
    intro i j; rw [hrow]; exact bf8_bound M (fun k => (d i k).toInt) (hd i) j
  have hcol : (colsv bf8v (rowsv bf8v d) i j).toInt =
      cols bf8 (rows bf8 (fun i j => (d i j).toInt)) i j := by
    have := bf8v_toInt (8 * M + 1) (by omega) (fun k => rowsv bf8v d k j) (fun k => hrb k j) i
    have e : (fun k => (rowsv bf8v d k j).toInt) = fun k => rows bf8 (fun i j => (d i j).toInt) k j :=
      funext fun k => hrow k j
    rw [e] at this
    exact this
  have hb := bf8_bound (8 * M + 1) (fun k => rows bf8 (fun i j => (d i j).toInt) k j)
    (fun k => by have := hrb k j; rw [hrow] at this; exact this) i
  unfold idct8v idct8b idct8
  apply rndv_toInt _ _ hcol
  simp only [cols] at hb ⊢
  omega

/-- The Rust coefficient range `[-32768, 32767]` (8-bit video; also the clamp in `idct4_add`). -/
def RustRange (v : Int) : Prop := -32768 ≤ v ∧ v ≤ 32767

theorem idct4_rust_i32_exact (d : Fin 4 → Fin 4 → W) (hd : ∀ i j, RustRange (d i j).toInt) :
    ∀ i j, (idct4v d i j).toInt = idct4 (fun i j => (d i j).toInt) i j :=
  idct4v_toInt (2 ^ 15) (by decide) d fun i j => by have := hd i j; unfold RustRange at this; omega

theorem idct8_rust_i32_exact (d : Fin 8 → Fin 8 → W) (hd : ∀ i j, RustRange (d i j).toInt) :
    ∀ i j, (idct8v d i j).toInt = idct8 (fun i j => (d i j).toInt) i j :=
  idct8v_toInt (2 ^ 15) (by decide) d fun i j => by have := hd i j; unfold RustRange at this; omega

/-! ### Every intermediate of the `Int` model is an `i32` (statement without BitVec) -/

/-- 4x4, all intermediates of the row pass, the column pass, and `v + 32` fit in `i32`. -/
theorem idct4_no_overflow (M : Int) (hM : M ≤ 2 ^ 21) (D : Fin 4 → Fin 4 → Int)
    (hD : ∀ i j, -M ≤ D i j ∧ D i j ≤ M) :
    (∀ i, ∀ v ∈ inter4 (D i), I32 v) ∧
    (∀ j, ∀ v ∈ inter4 (fun k => rows bf4 D k j), I32 v) ∧
    (∀ i j, I32 (cols bf4 (rows bf4 D) i j + 32)) := by
  have hb : ∀ k j, -(4 * M) ≤ rows bf4 D k j ∧ rows bf4 D k j ≤ 4 * M :=
    fun k j => bf4_bound M (D k) (hD k) j
  refine ⟨fun i => bf4_inter_i32 M (by omega) (D i) (hD i), fun j => ?_, fun i j => ?_⟩
  · exact bf4_inter_i32 (4 * M) (by omega) _ (fun k => hb k j)
  · have := bf4_bound (4 * M) (fun k => rows bf4 D k j) (fun k => hb k j) i
    simp only [cols] at this ⊢
    unfold I32; omega

theorem idct8_no_overflow (M : Int) (hM : M ≤ 2 ^ 21) (D : Fin 8 → Fin 8 → Int)
    (hD : ∀ i j, -M ≤ D i j ∧ D i j ≤ M) :
    (∀ i, ∀ v ∈ inter8 (D i), I32 v) ∧
    (∀ j, ∀ v ∈ inter8 (fun k => rows bf8 D k j), I32 v) ∧
    (∀ i j, I32 (cols bf8 (rows bf8 D) i j + 32)) := by
  have hb : ∀ k j, -(8 * M + 1) ≤ rows bf8 D k j ∧ rows bf8 D k j ≤ 8 * M + 1 :=
    fun k j => bf8_bound M (D k) (hD k) j
  refine ⟨fun i => bf8_inter_i32 M (by omega) (D i) (hD i), fun j => ?_, fun i j => ?_⟩
  · exact bf8_inter_i32 (8 * M + 1) (by omega) _ (fun k => hb k j)
  · have := bf8_bound (8 * M + 1) (fun k => rows bf8 D k j) (fun k => hb k j) i
    simp only [cols] at this ⊢
    unfold I32; omega

/-- With the Rust range the numbers are far from the limit: row pass within `±4·2^15 = ±131072`,
column pass within `±16·2^15 = ±524288` (4x4); `±(8·2^15+1)` and `±(64·2^15+9)` (8x8). -/
theorem idct4_rust_bounds (D : Fin 4 → Fin 4 → Int) (hD : ∀ i j, RustRange (D i j)) :
    (∀ i j, -131072 ≤ rows bf4 D i j ∧ rows bf4 D i j ≤ 131072) ∧
    (∀ i j, -524288 ≤ cols bf4 (rows bf4 D) i j ∧ cols bf4 (rows bf4 D) i j ≤ 524288) := by
  have hD' : ∀ i j, -(2 ^ 15) ≤ D i j ∧ D i j ≤ 2 ^ 15 := fun i j => by
    have := hD i j; unfold RustRange at this; omega
  have hb : ∀ k j, -(4 * 2 ^ 15) ≤ rows bf4 D k j ∧ rows bf4 D k j ≤ 4 * 2 ^ 15 :=
    fun k j => bf4_bound (2 ^ 15) (D k) (hD' k) j
  refine ⟨fun i j => ?_, fun i j => ?_⟩
  · have := hb i j; omega
  · have := bf4_bound (4 * 2 ^ 15) (fun k => rows bf4 D k j) (fun k => hb k j) i
    simp only [cols]; omega

theorem idct8_rust_bounds (D : Fin 8 → Fin 8 → Int) (hD : ∀ i j, RustRange (D i j)) :
    (∀ i j, -262145 ≤ rows bf8 D i j ∧ rows bf8 D i j ≤ 262145) ∧
    (∀ i j, -2097161 ≤ cols bf8 (rows bf8 D) i j ∧ cols bf8 (rows bf8 D) i j ≤ 2097161) := by
  have hD' : ∀ i j, -(2 ^ 15) ≤ D i j ∧ D i j ≤ 2 ^ 15 := fun i j => by
    have := hD i j; unfold RustRange at this; omega
  have hb : ∀ k j, -(8 * 2 ^ 15 + 1) ≤ rows bf8 D k j ∧ rows bf8 D k j ≤ 8 * 2 ^ 15 + 1 :=
    fun k j => bf8_bound (2 ^ 15) (D k) (hD' k) j
  refine ⟨fun i j => ?_, fun i j => ?_⟩
  · have := hb i j; omega
  · have := bf8_bound (8 * 2 ^ 15 + 1) (fun k => rows bf8 D k j) (fun k => hb k j) i
    simp only [cols]; omega

end CraftIDCT
