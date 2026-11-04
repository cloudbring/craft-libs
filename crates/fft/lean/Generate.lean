import CraftFFT

open CraftFFT

/-! Exact (NTT) and high-precision (complex) FFT test vectors.  See ../TRUST.md.

CSV columns: `ring,case,n,x_re,x_im,X_re,X_im`, element lists space-separated.
* `ntt`: integers in `[0, 998244353)`, `x_im`/`X_im` empty.  `X` is `CraftFFT.fft` over `ZMod P`
  (proved equal to `dft`, `ntt_eq_dft`), forward sign convention `X_k = Σ x_j ω^(jk)`.
* `complex`: `x` are exact dyadic decimals (multiples of 1/8).  `X` is the defining DFT sum
  `Σ x_j exp(-2πi jk/N)` evaluated in 60-digit fixed point, rounded to 20 decimals. -/

def S : Nat := 10 ^ 60

def rdiv (a : Int) (b : Int) : Int := (2 * a + b) / (2 * b)

/-- Newton integer square root (floor). -/
partial def isqrt (n : Nat) : Nat :=
  if n < 2 then n else
  let rec go (x : Nat) : Nat :=
    let y := (x + n / x) / 2
    if y ≥ x then x else go y
  go (1 <<< (n.log2 / 2 + 1))

/-- `exp(-2πi/2^m)` at scale `S`; `ω_{2N} = (ω_N + 1)/|ω_N + 1|` (principal square root). -/
def omegaFix : Nat → Int × Int
  | 0 => (S, 0)
  | 1 => (-(S : Int), 0)
  | 2 => (0, -(S : Int))
  | m + 3 =>
    let (c, s) := omegaFix (m + 2)
    let a : Int := c + S
    let nrm : Int := (isqrt (a * a + s * s).toNat : Nat)
    (rdiv (a * S) nrm, rdiv (s * S) nrm)

/-- `ω^t` for `t < 2^m` at scale `S`. -/
def wTable (m : Nat) : Array (Int × Int) := Id.run do
  let w := omegaFix m
  let mut acc : Array (Int × Int) := #[((S : Int), 0)]
  for _ in [1:2 ^ m] do
    let (c, s) := acc.back!
    acc := acc.push (rdiv (c * w.1 - s * w.2) S, rdiv (c * w.2 + s * w.1) S)
  return acc

def dftFix (m : Nat) (xr xi : Array Int) : Array (Int × Int) := Id.run do
  let n := 2 ^ m
  let w := wTable m
  let mut out : Array (Int × Int) := Array.mkEmpty n
  for k in [0:n] do
    let mut re : Int := 0
    let mut im : Int := 0
    for j in [0:n] do
      let a := xr[j]!
      let b := xi[j]!
      if a != 0 || b != 0 then
        let (c, s) := w[(j * k) % n]!
        re := re + a * c - b * s
        im := im + a * s + b * c
    out := out.push (re, im)
  return out

def decStr (q : Int) (d : Nat) : String :=
  let a := q.natAbs
  let fp := toString (a % 10 ^ d)
  (if q < 0 then "-" else "") ++ toString (a / 10 ^ d) ++ "." ++
    String.mk (List.replicate (d - fp.length) '0') ++ fp

def lcgNext (s : Nat) : Nat := (s * 1103515245 + 12345) % 2147483648

/-- Deterministic stream of `len` states starting from `seed`. -/
def lcg (seed len : Nat) : Array Nat := Id.run do
  let mut s := seed
  let mut out := #[]
  for _ in [0:len] do
    s := lcgNext s
    out := out.push s
  return out

/-- Dyadic input in `{-64..64}/8`. -/
def dyad (s : Nat) : Int := ((s >>> 16) % 129 : Nat) - 64

def joinS (xs : Array String) : String := " ".intercalate xs.toList

def emitComplex (name : String) (m : Nat) (xr xi : Array Int) : IO Unit := do
  let out := dftFix m xr xi
  let fx := fun (a : Int) => decStr (a * 125) 3
  let fo := fun (v : Int) => decStr (rdiv (v * 10 ^ 20) (8 * S)) 20
  IO.println (",".intercalate ["complex", name, toString (2 ^ m),
    joinS (xr.map fx), joinS (xi.map fx), joinS (out.map (fo ·.1)), joinS (out.map (fo ·.2))])

def emitNtt (name : String) (m : Nat) (x : Array Nat) : IO Unit := do
  let xs : Array (ZMod P) := x.map fun v => (Nat.cast v : ZMod P)
  let sig : ℕ → ZMod P := fun j => xs.getD j 0
  let out := (Array.range (2 ^ m)).map fun k => (fft m (nttRoot m) sig k).val
  if m ≤ 5 then
    for k in [0:2 ^ m] do
      if fft m (nttRoot m) sig k != dft (nttRoot m) (2 ^ m) sig k then
        throw (IO.userError s!"fft/dft mismatch ntt m={m} k={k}")
  IO.println (",".intercalate ["ntt", name, toString (2 ^ m),
    joinS (xs.map (toString ·.val)), "", joinS (out.map toString), ""])

def generate : IO Unit := do
  IO.println "ring,case,n,x_re,x_im,X_re,X_im"
  -- NTT over ZMod 998244353, N = 2 .. 256 (the closure-based proved `fft` is evaluated by the
  -- interpreter with a naive `ZMod` power, so larger N is impractical).
  for m in [1:9] do
    let n := 2 ^ m
    emitNtt "impulse1" m ((Array.range n).map fun j => if j == 1 then 1 else 0)
    emitNtt "random" m ((lcg (1000 + m) n).map fun s => s)
  -- Complex, N = 2 .. 4096.  Dense inputs up to N = 1024; from N = 16 also 8-sparse inputs
  -- (the DFT sum skips zero samples, so these stay O(8N) for N up to 4096).
  for m in [1:13] do
    let n := 2 ^ m
    emitComplex "impulse1" m ((Array.range n).map fun j => if j == 1 then 8 else 0) (Array.replicate n 0)
    if m ≤ 9 then
      emitComplex "real" m ((lcg (2000 + m) n).map dyad) (Array.replicate n 0)
    if m ≤ 10 then
      emitComplex "complex" m ((lcg (3000 + m) n).map dyad) ((lcg (4000 + m) n).map dyad)
    if m ≥ 4 then
      let pos := (lcg (5000 + m) 8).map fun s => (s >>> 8) % n
      let sparse := fun (seed : Nat) => Id.run do
        let vals := lcg seed 8
        let mut acc : Array Int := Array.replicate n 0
        for i in [0:8] do
          acc := acc.modify pos[i]! (· + dyad vals[i]!)
        return acc
      emitComplex "sparse8" m (sparse (6000 + m)) (sparse (7000 + m))

#eval generate
