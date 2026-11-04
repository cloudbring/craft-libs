import CraftSRGB

open CraftSRGB

/-! Reference vector generator. Output is `tests/vectors/srgb.csv`.

`Real.rpow` is not computable. Candidate bounds come from an untrusted integer k-th root; every
value is then accepted only if the decidable certificate `DecodeOK` / `RootOK` holds, which
`decodeOK_sound` / `rootOK_sound` (CraftSRGB/Cert.lean) turn into a proof that the true real value
lies in `[lo, lo + 10^-30)`. A failed certificate throws, so a wrong root cannot reach the CSV. -/

def S : Nat := 10 ^ 30

/-- Floor k-th root by Newton's method (untrusted; results are certified afterwards). -/
partial def iroot (k n : Nat) : Nat :=
  if n == 0 then 0 else
  let x0 := 2 ^ ((Nat.log2 n) / k + 1)
  let rec go (x : Nat) : Nat :=
    let y := ((k - 1) * x + n / x ^ (k - 1)) / k
    if y >= x then x else go y
  go x0

/-- Exact decimal expansion of `⌊q·10^33⌋ / 10^33` for `q ≥ 0`. -/
def dec (q : ℚ) : String :=
  let v := (q * 10 ^ 33).floor
  if v < 0 then panic! "negative value" else
  let n := v.toNat
  let s := toString n
  let s := if s.length ≤ 33 then String.mk (List.replicate (34 - s.length) (Char.ofNat 48)) ++ s else s
  let k := s.length - 33
  (s.take k) ++ "." ++ (s.drop k)

def frac (q : ℚ) : String := s!"{q.num}/{q.den}"

/-- Certified lower end of `dr x` (`x ≥ 0`); width `10^-30`. -/
def drLo (x : ℚ) : IO ℚ := do
  let t := (x * 200 + 11) / 211
  let n := (S ^ 5 * t.num.toNat ^ 12) / t.den ^ 12
  let r : ℚ := (iroot 5 n : Nat) / (S : ℚ)
  if DecodeOK x r (1 / (S : ℚ)) then pure r else throw (IO.userError s!"decode certificate failed at {frac x}")

/-- Certified lower end of `er y` (`y > 0.0031308`); width `1.055·10^-30`. -/
def erLo (y : ℚ) : IO ℚ := do
  let n := (S ^ 12 * y.num.toNat ^ 5) / y.den ^ 5
  let w : ℚ := (iroot 12 n : Nat) / (S : ℚ)
  if RootOK y w (1 / (S : ℚ)) then pure (211 / 200 * w - 11 / 200)
  else throw (IO.userError s!"encode certificate failed at {frac y}")

def kneeAq : ℚ := 809 / 20000
def kneeBq : ℚ := 7827 / 2500000

def decodeLo (x : ℚ) : IO ℚ := if x ≤ kneeAq then pure (x / (323 / 25)) else drLo x
def encodeLo (y : ℚ) : IO ℚ := if y ≤ kneeBq then pure (323 / 25 * y) else erLo y

def row (kind idx input output : String) : IO Unit :=
  IO.println s!"{kind},{idx},{input},{output}"

def generate : IO Unit := do
  IO.println "kind,index,input,lower_bound"
  -- decode of all 256 8-bit codes
  for c in [0:256] do
    let x : ℚ := (c : ℚ) / 255
    row "decode8" (toString c) s!"{c}/255" (dec (← decodeLo x))
  -- 4096-point grids over [0,1]
  for i in [0:4096] do
    let x : ℚ := (i : ℚ) / 4095
    row "decode_grid" (toString i) s!"{i}/4095" (dec (← decodeLo x))
  for i in [0:4096] do
    let y : ℚ := (i : ℚ) / 4095
    row "encode_grid" (toString i) s!"{i}/4095" (dec (← encodeLo y))
  -- knee quantities (Lval, Rval, 12.92*0.0031308, er(0.0031308), encode(decode 0.04045))
  row "knee" "L" (frac kneeAq) (dec (kneeAq / (323 / 25)))
  row "knee" "R" (frac kneeAq) (dec (← drLo kneeAq))
  row "knee" "k" (frac kneeBq) (dec (323 / 25 * kneeBq))
  row "knee" "Eb" (frac kneeBq) (dec (← erLo kneeBq))
  row "knee" "EDa" (frac kneeAq) (dec (← erLo (kneeAq / (323 / 25))))

#eval generate
