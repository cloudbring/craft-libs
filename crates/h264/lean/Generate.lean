import CraftIDCT

open CraftIDCT

/-! Test vectors for `crates/h264/src/transform.rs` (`idct4_add`, `idct8_add`).  See ../TRUST.md.

CSV columns: `n,case,coeffs,pre,res`, element lists space-separated, raster order (row-major).
* `coeffs`: the `n*n` input coefficients, all within the Rust/spec range `[-32768, 32767]`.
* `pre`: the column-pass output `cols (rows d)` (before rounding), from the `Int` model `CraftIDCT.bf4/bf8`.
* `res`: `(pre + 32) >> 6`, the residual `idct4_add`/`idct8_add` add to the prediction (`CraftIDCT.idct4/idct8`).
Cases: structural ones; `maxpre_P` / `minpre_P`: input pattern maximising / minimising `pre[P]` over the
box corners (4x4: exhaustive over all 2^16 corners, 8x8: coordinate ascent from the sign pattern;
the 4x4 ascent is checked against the exhaustive result); LCG random in the full range; LCG random corners. -/

def LO : Int := -32768
def HI : Int := 32767

def mk (n : Nat) (a : Array Int) : Fin n → Fin n → Int := fun i j => a[i.val * n + j.val]!

def lcgNext (s : Nat) : Nat := (s * 1103515245 + 12345) % 2147483648
def lcg (seed len : Nat) : Array Nat := Id.run do
  let mut s := seed
  let mut out := #[]
  for _ in [0:len] do
    s := lcgNext s
    out := out.push s
  return out

/-- Pre-rounding transform output, flattened. -/
def preOf (n : Nat) (a : Array Int) : Array Int :=
  match n with
  | 4 => (Array.range 16).map fun p => cols bf4 (rows bf4 (mk 4 a)) (Fin.ofNat 4 (p / 4)) (Fin.ofNat 4 (p % 4))
  | _ => (Array.range 64).map fun p => cols bf8 (rows bf8 (mk 8 a)) (Fin.ofNat 8 (p / 8)) (Fin.ofNat 8 (p % 8))

def resOf (n : Nat) (a : Array Int) : Array Int :=
  match n with
  | 4 => (Array.range 16).map fun p => idct4 (mk 4 a) (Fin.ofNat 4 (p / 4)) (Fin.ofNat 4 (p % 4))
  | _ => (Array.range 64).map fun p => idct8 (mk 8 a) (Fin.ofNat 8 (p / 8)) (Fin.ofNat 8 (p % 8))

def joinS (xs : Array Int) : String := " ".intercalate (xs.map toString).toList

def emit (n : Nat) (name : String) (a : Array Int) : IO Unit := do
  if a.size != n * n || a.any (fun v => v < LO || v > HI) then
    throw (IO.userError s!"input out of range: {name}")
  IO.println (",".intercalate [toString n, name, joinS a, joinS (preOf n a), joinS (resOf n a)])

/-- Coordinate ascent maximising `sgn * pre[p]`: start from the sign pattern at the zero matrix. -/
def climb (n : Nat) (p : Nat) (sgn : Int) : Array Int × Int := Id.run do
  let m := n * n
  let f := fun (a : Array Int) => sgn * (preOf n a)[p]!
  let mut a : Array Int := Array.replicate m 0
  for k in [0:m] do
    a := a.set! k (if f (a.set! k HI) ≥ f (a.set! k LO) then HI else LO)
  let mut best := f a
  for _ in [0:4] do
    for k in [0:m] do
      let b := a.set! k (if a[k]! == HI then LO else HI)
      if f b > best then
        best := f b
        a := b
  return (a, best)

/-- Exhaustive best corner for 4x4: returns (maxPatterns, maxVals, minPatterns, minVals). -/
def exhaustive4 : Array (Array Int) × Array Int × Array (Array Int) × Array Int := Id.run do
  let mut mxA : Array (Array Int) := Array.replicate 16 #[]
  let mut mnA : Array (Array Int) := Array.replicate 16 #[]
  let mut mx : Array Int := Array.replicate 16 (-1000000000)
  let mut mn : Array Int := Array.replicate 16 1000000000
  for c in [0:65536] do
    let a : Array Int := (Array.range 16).map fun p => if (c >>> p) % 2 == 1 then HI else LO
    let o := preOf 4 a
    for p in [0:16] do
      if o[p]! > mx[p]! then
        mx := mx.set! p o[p]!
        mxA := mxA.set! p a
      if o[p]! < mn[p]! then
        mn := mn.set! p o[p]!
        mnA := mnA.set! p a
  return (mxA, mx, mnA, mn)

def generate : IO Unit := do
  IO.println "n,case,coeffs,pre,res"
  for n in [4, 8] do
    let m := n * n
    emit n "zero" (Array.replicate m 0)
    emit n "dc_max" ((Array.replicate m (0 : Int)).set! 0 HI)
    emit n "dc_min" ((Array.replicate m (0 : Int)).set! 0 LO)
    emit n "all_max" (Array.replicate m HI)
    emit n "all_min" (Array.replicate m LO)
    emit n "checker" ((Array.range m).map fun p => if (p / n + p % n) % 2 == 0 then HI else LO)
    emit n "odd_max" ((Array.range m).map fun p => if p % 2 == 1 then HI else 0)
    if n == 4 then
      let (mxA, mx, mnA, mn) := exhaustive4
      for p in [0:16] do
        let (ca, cv) := climb 4 p 1
        let (cb, cw) := climb 4 p (-1)
        if cv != mx[p]! || cw != -mn[p]! then
          throw (IO.userError s!"coordinate ascent differs from exhaustive search at p={p}: {cv} vs {mx[p]!}, {cw} vs {-mn[p]!}")
        emit 4 s!"maxpre_{p}" mxA[p]!
        emit 4 s!"minpre_{p}" mnA[p]!
        let _ := ca; let _ := cb
    else
      for p in [0:64] do
        emit 8 s!"maxpre_{p}" (climb 8 p 1).1
        emit 8 s!"minpre_{p}" (climb 8 p (-1)).1
    for r in [0:48] do
      let s := lcg (100 * n + r) m
      emit n s!"random_{r}" (s.map fun v => (((v >>> 8) % 65536 : Nat) : Int) - 32768)
    for r in [0:32] do
      let s := lcg (7000 * n + r) m
      emit n s!"corner_{r}" (s.map fun v => if (v >>> 16) % 2 == 1 then HI else LO)
    for r in [0:16] do
      let s := lcg (90000 * n + r) m
      -- sparse: a few large coefficients, the rest zero
      emit n s!"sparse_{r}" (s.map fun v => if (v >>> 12) % 5 == 0 then (((v >>> 4) % 65536 : Nat) : Int) - 32768 else 0)

#eval generate
