import CraftBlur

open CraftBlur

/-- `n/d` in lowest terms. -/
def fraction (value : ℚ) : String := s!"{value.num}/{value.den}"

/-- Decimal expansion rounded half up to `places` digits (exact integer arithmetic). -/
def decimal (value : ℚ) (places : Nat := 30) : String :=
  let scaled : Nat := ⌊|value| * (10 : ℚ) ^ places + 1 / 2⌋₊
  let digits := toString scaled
  let padded := String.mk (List.replicate (places + 1 - digits.length) '0') ++ digits
  let cut := padded.length - places
  (if value < 0 then "-" else "") ++ padded.take cut ++ "." ++ padded.drop cut

/-- Grid of requested sigmas, all exactly representable in `f32`. -/
def grid : List ℚ :=
  [1 / 2, 3 / 4, 1, 3 / 2, 2, 5 / 2, 3, 4, 6, 8, 12, 16, 24, 32, 48, 64]

def row (kernel : String) (sigma : ℚ) (widths : String) (accepted : Bool)
    (rat : Option ℚ) (variance dc : ℚ) : IO Unit :=
  IO.println (String.intercalate ","
    [kernel, fraction sigma, widths, toString accepted,
     (rat.map fraction).getD "", decimal variance, decimal dc])

def generate : IO Unit := do
  IO.println "kernel,sigma,widths,accepted,var_rat,var_dec,dc_dec"
  for sigma in grid do
    -- Sampled Gaussian, variance-calibrated by craft-blur: effective variance is the target.
    row "sampled" sigma "" true (some (sigma ^ 2)) (sigma ^ 2) 1
    -- Three-box cascade: exact widths and exact variance.
    let ws := widths sigma
    let variance := widthVar ws
    -- Interpreter cross-checks of the proved statements (a failure aborts generation).
    -- (the explicit convolution has `∏ w` entries; keep it to sigma <= 8 for the interpreter's stack)
    let direct := CraftBlur.var (cascadeW ws)
    if sigma ≤ 8 && direct != variance then throw (IO.userError s!"cascade variance mismatch at {fraction sigma}")
    if |variance - sigma ^ 2| > ((lowerWidth sigma : ℚ) + 1) / 6 then
      throw (IO.userError s!"width-selection bound violated at {fraction sigma}")
    let accepted := decide (|variance / sigma ^ 2 - 1| ≤ 1 / (3 * sigma))
    row "box3" sigma (" ".intercalate (ws.map toString)) accepted (some variance) variance 1
    -- Young–van Vliet recursive filter (craft-blur uses it from sigma 2.5).
    if sigma ≥ 5 / 2 then
      row "yvv" sigma "" true none (yvvVariance sigma) (yvvDC sigma)

#eval generate
