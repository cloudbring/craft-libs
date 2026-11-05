import CraftResample

open CraftResample

/-- `n/d` in lowest terms. -/
def fraction (value : ℚ) : String := s!"{value.num}/{value.den}"

def fractions (xs : List ℚ) : String := " ".intercalate (xs.map fraction)

/-- 64-bit LCG (Knuth MMIX constants); deterministic, no library randomness. -/
def lcg (s : Nat) : Nat := (s * 6364136223846793005 + 1442695040888963407) % 18446744073709551616

/-- `count` random small rationals `a/b`, `a ∈ [-16, 16]`, `b ∈ {1,2,3,4,5,7,8}`; returns state. -/
def randRats (seed count : Nat) (positive : Bool := false) : List ℚ × Nat := Id.run do
  let dens : Array Nat := #[1, 2, 3, 4, 5, 7, 8]
  let mut s := seed
  let mut out : List ℚ := []
  for _ in [0:count] do
    s := lcg s
    let a : Int := ((s / 65536) % 33 : Nat) - 16
    s := lcg s
    let b := dens[(s / 65536) % 7]!
    let a' : Int := if positive then a.natAbs + 1 else a
    out := out ++ [(a' : ℚ) / (b : ℚ)]
  return (out, s)

def seqOf (xs : List ℚ) : ℤ → ℚ := fun j => if 0 ≤ j then xs.getD j.toNat 0 else 0

def tapsOf (hs : List ℚ) : ℕ → ℚ := fun k => hs.getD k 0

/-- Direct form, polyphase-by-formula, and the streamed accumulator (started at 0 and at 3). -/
def outputs (hs xs : List ℚ) (L M : Nat) : IO (List ℚ) := do
  let N := hs.length
  let T := tapsPerPhase N L
  let len := xs.length
  let cnt := outLen len N L M
  let x := seqOf xs
  let h := tapsOf hs
  let mut ys : List ℚ := []
  for n in [0:cnt] do
    let d := direct h N L M x n
    let p := polyOut h N L T x ((n * M / L : Nat) : ℤ) (n * M % L)
    let s := streamOut h N L M T x 0 n
    let s3 := if n ≥ 3 then streamOut h N L M T x 3 (n - 3) else d
    if d != p || d != s || d != s3 then
      throw (IO.userError s!"polyphase mismatch L={L} M={M} n={n}")
    ys := ys ++ [d]
  -- `direct_zero_after`, checked on the data
  for n in [cnt:cnt + 3] do
    if direct h N L M x n != 0 then throw (IO.userError s!"output past out_len L={L} M={M}")
  return ys

def row (case : String) (L M : Nat) (hs xs ys : List ℚ) : IO Unit :=
  IO.println (String.intercalate ","
    [case, toString L, toString M, toString hs.length, toString xs.length, toString ys.length,
     fractions xs, fractions hs, fractions ys])

/-- Random taps and inputs. -/
def randomCase (name : String) (L M T cut len seed : Nat) : IO Unit := do
  let N := L * T - cut
  let (hs, s1) := randRats seed N
  let (xs, _) := randRats s1 len
  let ys ← outputs hs xs L M
  row name L M hs xs ys

/-- Taps with every phase normalised to sum 1 (positive random weights), constant input `3/7`:
interior outputs must equal the constant exactly. -/
def dcCase (name : String) (L M T len seed : Nat) : IO Unit := do
  let N := L * T
  let (ws, s1) := randRats seed N (positive := true)
  let sums : List ℚ := (List.range L).map fun p => ((List.range T).map fun q => ws.getD (p + q * L) 0).sum
  let hs : List ℚ := (List.range N).map fun k => ws.getD k 0 / sums.getD (k % L) 1
  let _ := s1
  let c : ℚ := 3 / 7
  let xs : List ℚ := List.replicate len c
  let ys ← outputs hs xs L M
  -- interior: the whole window `i-q`, `q<T` inside `[0, len)`
  for n in [0:ys.length] do
    let i := n * M / L
    if i + 1 ≥ T && i < len && ys.getD n 0 != c then
      throw (IO.userError s!"DC gain not 1 at L={L} M={M} n={n}")
  -- and the sum of all taps is `L` (zero-stuffing gain)
  if hs.sum != (L : ℚ) then throw (IO.userError "prototype sum is not L")
  row name L M hs xs ys

def generate : IO Unit := do
  IO.println "case,L,M,taps,len,out_len,x,h,y"
  randomCase "rand" 1 1 5 0 12 11
  randomCase "rand" 2 1 4 0 10 12
  randomCase "rand" 3 2 4 0 12 13
  randomCase "rand" 3 2 4 1 9 14
  randomCase "rand" 2 3 5 0 15 15
  randomCase "rand" 4 3 3 0 11 16
  randomCase "rand" 4 3 3 2 8 17
  randomCase "rand" 3 4 4 0 14 18
  randomCase "rand" 1 2 6 0 13 19
  randomCase "rand" 4 2 3 0 9 20
  randomCase "rand" 6 4 3 1 10 21
  randomCase "rand" 5 3 2 0 7 22
  randomCase "rand" 160 147 2 0 20 23
  randomCase "rand" 160 147 2 37 14 24
  randomCase "rand" 147 160 2 0 12 25
  randomCase "rand" 147 160 2 11 9 26
  randomCase "rand" 80 49 3 0 10 27
  dcCase "dc" 3 2 4 14 31
  dcCase "dc" 2 3 5 18 32
  dcCase "dc" 4 3 3 14 33
  dcCase "dc" 160 147 2 20 34
  dcCase "dc" 147 160 2 16 35

#eval generate
