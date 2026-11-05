import CraftIDCT.Basic
import Std.Tactic.BVDecide

/-! Independent SAT cross-check with `bv_decide` (Std.Tactic.BVDecide, CaDiCaL + LRAT certificate
checked by the kernel via `Lean.ofReduceBool`): the 32-bit pipeline equals the 64-bit pipeline,
sign-extended, on the whole input box.  A 64-bit pipeline cannot overflow on this box, so this
re-proves "no `i32` wrap" for the full transform without the interval argument. -/

namespace CraftIDCT

/-- Signed range `[-32768, 32767]` of an `i32`. -/
def inRange (x : BitVec 32) : Prop := BitVec.slt x (-32768) = false ∧ BitVec.sle x 32767 = true



set_option maxHeartbeats 40000000 in
theorem idct4_sat_flat (d00 d01 d02 d03 d10 d11 d12 d13 d20 d21 d22 d23 d30 d31 d32 d33 : BitVec 32)
    (h00 : inRange d00)
    (h01 : inRange d01)
    (h02 : inRange d02)
    (h03 : inRange d03)
    (h10 : inRange d10)
    (h11 : inRange d11)
    (h12 : inRange d12)
    (h13 : inRange d13)
    (h20 : inRange d20)
    (h21 : inRange d21)
    (h22 : inRange d22)
    (h23 : inRange d23)
    (h30 : inRange d30)
    (h31 : inRange d31)
    (h32 : inRange d32)
    (h33 : inRange d33) :
    ∀ i j, ((idct4b ![![d00,d01,d02,d03],![d10,d11,d12,d13],![d20,d21,d22,d23],![d30,d31,d32,d33]]) i j).signExtend 64 =
      idct4b (fun i j => ((![![d00,d01,d02,d03],![d10,d11,d12,d13],![d20,d21,d22,d23],![d30,d31,d32,d33]]) i j).signExtend 64) i j := by
  simp only [inRange] at h00 h01 h02 h03 h10 h11 h12 h13 h20 h21 h22 h23 h30 h31 h32 h33
  intro i j
  fin_cases i <;> fin_cases j <;>
    simp only [idct4b, rndb, colsb, rowsb, bf4b, Matrix.cons_val, Matrix.cons_val_zero,
      Matrix.cons_val_one, Fin.isValue, Fin.reduceFinMk] <;> bv_decide

theorem idct4_sat (d : Fin 4 → Fin 4 → BitVec 32) (hd : ∀ i j, inRange (d i j)) :
    ∀ i j, (idct4b d i j).signExtend 64 = idct4b (fun i j => (d i j).signExtend 64) i j := by
  have key := idct4_sat_flat (d 0 0) (d 0 1) (d 0 2) (d 0 3) (d 1 0) (d 1 1) (d 1 2) (d 1 3) (d 2 0) (d 2 1) (d 2 2) (d 2 3) (d 3 0) (d 3 1) (d 3 2) (d 3 3) (hd 0 0) (hd 0 1) (hd 0 2) (hd 0 3) (hd 1 0) (hd 1 1) (hd 1 2) (hd 1 3) (hd 2 0) (hd 2 1) (hd 2 2) (hd 2 3) (hd 3 0) (hd 3 1) (hd 3 2) (hd 3 3)
  have hD : (![![d 0 0,d 0 1,d 0 2,d 0 3],![d 1 0,d 1 1,d 1 2,d 1 3],![d 2 0,d 2 1,d 2 2,d 2 3],![d 3 0,d 3 1,d 3 2,d 3 3]] : Fin 4 → Fin 4 → BitVec 32) = d := by
    funext i j; fin_cases i <;> fin_cases j <;> rfl
  rw [hD] at key
  exact key

set_option maxHeartbeats 40000000 in
theorem idct8_sat_flat (d00 d01 d02 d03 d04 d05 d06 d07 d10 d11 d12 d13 d14 d15 d16 d17 d20 d21 d22 d23 d24 d25 d26 d27 d30 d31 d32 d33 d34 d35 d36 d37 d40 d41 d42 d43 d44 d45 d46 d47 d50 d51 d52 d53 d54 d55 d56 d57 d60 d61 d62 d63 d64 d65 d66 d67 d70 d71 d72 d73 d74 d75 d76 d77 : BitVec 32)
    (h00 : inRange d00)
    (h01 : inRange d01)
    (h02 : inRange d02)
    (h03 : inRange d03)
    (h04 : inRange d04)
    (h05 : inRange d05)
    (h06 : inRange d06)
    (h07 : inRange d07)
    (h10 : inRange d10)
    (h11 : inRange d11)
    (h12 : inRange d12)
    (h13 : inRange d13)
    (h14 : inRange d14)
    (h15 : inRange d15)
    (h16 : inRange d16)
    (h17 : inRange d17)
    (h20 : inRange d20)
    (h21 : inRange d21)
    (h22 : inRange d22)
    (h23 : inRange d23)
    (h24 : inRange d24)
    (h25 : inRange d25)
    (h26 : inRange d26)
    (h27 : inRange d27)
    (h30 : inRange d30)
    (h31 : inRange d31)
    (h32 : inRange d32)
    (h33 : inRange d33)
    (h34 : inRange d34)
    (h35 : inRange d35)
    (h36 : inRange d36)
    (h37 : inRange d37)
    (h40 : inRange d40)
    (h41 : inRange d41)
    (h42 : inRange d42)
    (h43 : inRange d43)
    (h44 : inRange d44)
    (h45 : inRange d45)
    (h46 : inRange d46)
    (h47 : inRange d47)
    (h50 : inRange d50)
    (h51 : inRange d51)
    (h52 : inRange d52)
    (h53 : inRange d53)
    (h54 : inRange d54)
    (h55 : inRange d55)
    (h56 : inRange d56)
    (h57 : inRange d57)
    (h60 : inRange d60)
    (h61 : inRange d61)
    (h62 : inRange d62)
    (h63 : inRange d63)
    (h64 : inRange d64)
    (h65 : inRange d65)
    (h66 : inRange d66)
    (h67 : inRange d67)
    (h70 : inRange d70)
    (h71 : inRange d71)
    (h72 : inRange d72)
    (h73 : inRange d73)
    (h74 : inRange d74)
    (h75 : inRange d75)
    (h76 : inRange d76)
    (h77 : inRange d77) :
    ∀ i j, ((idct8b ![![d00,d01,d02,d03,d04,d05,d06,d07],![d10,d11,d12,d13,d14,d15,d16,d17],![d20,d21,d22,d23,d24,d25,d26,d27],![d30,d31,d32,d33,d34,d35,d36,d37],![d40,d41,d42,d43,d44,d45,d46,d47],![d50,d51,d52,d53,d54,d55,d56,d57],![d60,d61,d62,d63,d64,d65,d66,d67],![d70,d71,d72,d73,d74,d75,d76,d77]]) i j).signExtend 64 =
      idct8b (fun i j => ((![![d00,d01,d02,d03,d04,d05,d06,d07],![d10,d11,d12,d13,d14,d15,d16,d17],![d20,d21,d22,d23,d24,d25,d26,d27],![d30,d31,d32,d33,d34,d35,d36,d37],![d40,d41,d42,d43,d44,d45,d46,d47],![d50,d51,d52,d53,d54,d55,d56,d57],![d60,d61,d62,d63,d64,d65,d66,d67],![d70,d71,d72,d73,d74,d75,d76,d77]]) i j).signExtend 64) i j := by
  simp only [inRange] at h00 h01 h02 h03 h04 h05 h06 h07 h10 h11 h12 h13 h14 h15 h16 h17 h20 h21 h22 h23 h24 h25 h26 h27 h30 h31 h32 h33 h34 h35 h36 h37 h40 h41 h42 h43 h44 h45 h46 h47 h50 h51 h52 h53 h54 h55 h56 h57 h60 h61 h62 h63 h64 h65 h66 h67 h70 h71 h72 h73 h74 h75 h76 h77
  intro i j
  fin_cases i <;> fin_cases j <;>
    simp only [idct8b, rndb, colsb, rowsb, bf8b, Matrix.cons_val, Matrix.cons_val_zero,
      Matrix.cons_val_one, Fin.isValue, Fin.reduceFinMk] <;> bv_decide

theorem idct8_sat (d : Fin 8 → Fin 8 → BitVec 32) (hd : ∀ i j, inRange (d i j)) :
    ∀ i j, (idct8b d i j).signExtend 64 = idct8b (fun i j => (d i j).signExtend 64) i j := by
  have key := idct8_sat_flat (d 0 0) (d 0 1) (d 0 2) (d 0 3) (d 0 4) (d 0 5) (d 0 6) (d 0 7) (d 1 0) (d 1 1) (d 1 2) (d 1 3) (d 1 4) (d 1 5) (d 1 6) (d 1 7) (d 2 0) (d 2 1) (d 2 2) (d 2 3) (d 2 4) (d 2 5) (d 2 6) (d 2 7) (d 3 0) (d 3 1) (d 3 2) (d 3 3) (d 3 4) (d 3 5) (d 3 6) (d 3 7) (d 4 0) (d 4 1) (d 4 2) (d 4 3) (d 4 4) (d 4 5) (d 4 6) (d 4 7) (d 5 0) (d 5 1) (d 5 2) (d 5 3) (d 5 4) (d 5 5) (d 5 6) (d 5 7) (d 6 0) (d 6 1) (d 6 2) (d 6 3) (d 6 4) (d 6 5) (d 6 6) (d 6 7) (d 7 0) (d 7 1) (d 7 2) (d 7 3) (d 7 4) (d 7 5) (d 7 6) (d 7 7) (hd 0 0) (hd 0 1) (hd 0 2) (hd 0 3) (hd 0 4) (hd 0 5) (hd 0 6) (hd 0 7) (hd 1 0) (hd 1 1) (hd 1 2) (hd 1 3) (hd 1 4) (hd 1 5) (hd 1 6) (hd 1 7) (hd 2 0) (hd 2 1) (hd 2 2) (hd 2 3) (hd 2 4) (hd 2 5) (hd 2 6) (hd 2 7) (hd 3 0) (hd 3 1) (hd 3 2) (hd 3 3) (hd 3 4) (hd 3 5) (hd 3 6) (hd 3 7) (hd 4 0) (hd 4 1) (hd 4 2) (hd 4 3) (hd 4 4) (hd 4 5) (hd 4 6) (hd 4 7) (hd 5 0) (hd 5 1) (hd 5 2) (hd 5 3) (hd 5 4) (hd 5 5) (hd 5 6) (hd 5 7) (hd 6 0) (hd 6 1) (hd 6 2) (hd 6 3) (hd 6 4) (hd 6 5) (hd 6 6) (hd 6 7) (hd 7 0) (hd 7 1) (hd 7 2) (hd 7 3) (hd 7 4) (hd 7 5) (hd 7 6) (hd 7 7)
  have hD : (![![d 0 0,d 0 1,d 0 2,d 0 3,d 0 4,d 0 5,d 0 6,d 0 7],![d 1 0,d 1 1,d 1 2,d 1 3,d 1 4,d 1 5,d 1 6,d 1 7],![d 2 0,d 2 1,d 2 2,d 2 3,d 2 4,d 2 5,d 2 6,d 2 7],![d 3 0,d 3 1,d 3 2,d 3 3,d 3 4,d 3 5,d 3 6,d 3 7],![d 4 0,d 4 1,d 4 2,d 4 3,d 4 4,d 4 5,d 4 6,d 4 7],![d 5 0,d 5 1,d 5 2,d 5 3,d 5 4,d 5 5,d 5 6,d 5 7],![d 6 0,d 6 1,d 6 2,d 6 3,d 6 4,d 6 5,d 6 6,d 6 7],![d 7 0,d 7 1,d 7 2,d 7 3,d 7 4,d 7 5,d 7 6,d 7 7]] : Fin 8 → Fin 8 → BitVec 32) = d := by
    funext i j; fin_cases i <;> fin_cases j <;> rfl
  rw [hD] at key
  exact key

end CraftIDCT
