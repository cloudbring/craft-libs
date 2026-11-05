import Mathlib.Data.Fin.VecNotation
import Mathlib.Tactic.Ring
import Mathlib.Tactic.FinCases

/-! Exact models of the H.264 inverse integer transforms in `crates/h264/src/transform.rs`
(`idct4_add`, `idct8_add`, `idct8_1d`), over `Int` (the mathematical model, `>>` is floor
division) and over `BitVec 32` (Rust `i32` with `+`, `-` wrapping and `>>` arithmetic).
See ../TRUST.md. -/

namespace CraftIDCT

/-! ## Int model -/

/-- 4-point butterfly (8.5.12.2): `e=a+c, f=a-c, g=(b>>1)-d, h=b+(d>>1)`. -/
def bf4 (x : Fin 4 → Int) : Fin 4 → Int :=
  let e := x 0 + x 2
  let f := x 0 - x 2
  let g := (x 1 >>> 1) - x 3
  let h := x 1 + (x 3 >>> 1)
  ![e + h, f + g, f - g, e - h]

/-- 8-point butterfly (8.5.13.2), mirrors `idct8_1d`. -/
def bf8 (d : Fin 8 → Int) : Fin 8 → Int :=
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
  ![b0 + b7, b2 + b5, b4 + b3, b6 + b1, b6 - b1, b4 - b3, b2 - b5, b0 - b7]

/-- Row pass, then column pass, on an `n×n` matrix, for any 1-D transform `f`. -/
def rows {n : Nat} (f : (Fin n → Int) → Fin n → Int) (d : Fin n → Fin n → Int) :
    Fin n → Fin n → Int := fun i => f (d i)

def cols {n : Nat} (f : (Fin n → Int) → Fin n → Int) (t : Fin n → Fin n → Int) :
    Fin n → Fin n → Int := fun i j => f (fun k => t k j) i

/-- Final rounding `(v + 32) >> 6` (8.5.14). -/
def rnd (v : Int) : Int := (v + 32) >>> 6

/-- Residual of the 4x4 transform of coefficient matrix `d` (what `idct4_add` adds to the prediction
before clipping). -/
def idct4 (d : Fin 4 → Fin 4 → Int) : Fin 4 → Fin 4 → Int :=
  fun i j => rnd (cols bf4 (rows bf4 d) i j)

def idct8 (d : Fin 8 → Fin 8 → Int) : Fin 8 → Fin 8 → Int :=
  fun i j => rnd (cols bf8 (rows bf8 d) i j)

/-! ## BitVec model (Rust `i32` is width 32) -/

abbrev W := BitVec 32

/-- `bf4` over `BitVec w`: wrapping `+ -`, arithmetic shift. -/
def bf4b {w : Nat} (x : Fin 4 → BitVec w) : Fin 4 → BitVec w :=
  let e := x 0 + x 2
  let f := x 0 - x 2
  let g := (x 1).sshiftRight 1 - x 3
  let h := x 1 + (x 3).sshiftRight 1
  ![e + h, f + g, f - g, e - h]

def bf8b {w : Nat} (d : Fin 8 → BitVec w) : Fin 8 → BitVec w :=
  let a0 := d 0 + d 4
  let a4 := d 0 - d 4
  let a2 := (d 2).sshiftRight 1 - d 6
  let a6 := d 2 + (d 6).sshiftRight 1
  let b0 := a0 + a6
  let b2 := a4 + a2
  let b4 := a4 - a2
  let b6 := a0 - a6
  let a1 := -d 3 + d 5 - d 7 - (d 7).sshiftRight 1
  let a3 := d 1 + d 7 - d 3 - (d 3).sshiftRight 1
  let a5 := -d 1 + d 7 + d 5 + (d 5).sshiftRight 1
  let a7 := d 3 + d 5 + d 1 + (d 1).sshiftRight 1
  let b1 := a1 + a7.sshiftRight 2
  let b7 := a7 - a1.sshiftRight 2
  let b3 := a3 + a5.sshiftRight 2
  let b5 := a3.sshiftRight 2 - a5
  ![b0 + b7, b2 + b5, b4 + b3, b6 + b1, b6 - b1, b4 - b3, b2 - b5, b0 - b7]

def rowsb {w n : Nat} (f : (Fin n → BitVec w) → Fin n → BitVec w) (d : Fin n → Fin n → BitVec w) :
    Fin n → Fin n → BitVec w := fun i => f (d i)

def colsb {w n : Nat} (f : (Fin n → BitVec w) → Fin n → BitVec w) (t : Fin n → Fin n → BitVec w) :
    Fin n → Fin n → BitVec w := fun i j => f (fun k => t k j) i

def rndb {w : Nat} (v : BitVec w) : BitVec w := (v + 32).sshiftRight 6

def idct4b {w : Nat} (d : Fin 4 → Fin 4 → BitVec w) : Fin 4 → Fin 4 → BitVec w :=
  fun i j => rndb (colsb bf4b (rowsb bf4b d) i j)

def idct8b {w : Nat} (d : Fin 8 → Fin 8 → BitVec w) : Fin 8 → Fin 8 → BitVec w :=
  fun i j => rndb (colsb bf8b (rowsb bf8b d) i j)

/-- The Rust `i32` instances. -/
abbrev bf4v : (Fin 4 → W) → Fin 4 → W := bf4b
abbrev bf8v : (Fin 8 → W) → Fin 8 → W := bf8b
abbrev rowsv {n : Nat} (f : (Fin n → W) → Fin n → W) (d : Fin n → Fin n → W) := rowsb f d
abbrev colsv {n : Nat} (f : (Fin n → W) → Fin n → W) (t : Fin n → Fin n → W) := colsb f t
abbrev rndv (v : W) : W := rndb v
abbrev idct4v (d : Fin 4 → Fin 4 → W) : Fin 4 → Fin 4 → W := idct4b d
abbrev idct8v (d : Fin 8 → Fin 8 → W) : Fin 8 → Fin 8 → W := idct8b d

end CraftIDCT
