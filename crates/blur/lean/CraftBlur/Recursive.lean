import Mathlib

/-!
# Young–van Vliet recursive Gaussian (`craft_blur::Recursive::new`)

Exact rational model of the coefficient computation: the decimal constants, the starting
polynomial `q₀(σ)` and five Newton steps, in `ℚ` (the Rust code does the same in `f64`).

For a causal recurrence `y[n] = g·x[n] + f₁y[n-1] + f₂y[n-2] + f₃y[n-3]` the generating function
of the impulse response is `G(x) = g / D(x)`, `D(x) = 1 - f₁x - f₂x² - f₃x³`, and with
`D₀ = D(1)`, `D₁ = D'(1)`, `D₂ = D''(1)` its variance is `-D₂/D₀ + D₁²/D₀² - D₁/D₀`.
`craft-blur` runs the mirror-image recurrence backwards, so the effective variance is twice that
(`CraftBlur.var_conv_reflect` proves the doubling for any finite kernel).

NOT proved: that the infinite impulse response has exactly this variance (a power-series
convergence argument).  The Rust test measures the real impulse response against it.
-/

namespace CraftBlur

def yvvReal : ℚ := 116680 / 100000
def yvvPair : ℚ := 110783 / 100000
def yvvImag : ℚ := 140586 / 100000

def yvvNorm : ℚ := yvvPair ^ 2 + yvvImag ^ 2
def yvvLinear : ℚ := 2 * (1 / yvvReal + 2 * yvvPair / yvvNorm)
def yvvQuadratic : ℚ :=
  2 * (1 / yvvReal ^ 2 + 2 * (yvvPair ^ 2 - yvvImag ^ 2) / yvvNorm ^ 2)

/-- Starting guess of the Rust code. -/
def q0 (σ : ℚ) : ℚ :=
  if σ < 3556 / 1000 then -2568 / 10000 + 5784 / 10000 * σ + 561 / 10000 * σ ^ 2
  else 25091 / 10000 + 9804 / 10000 * (σ - 3556 / 1000)

def newton (σ q : ℚ) : ℚ :=
  q - (q * (yvvQuadratic * q + yvvLinear) - σ ^ 2) / (2 * yvvQuadratic * q + yvvLinear)

/-- `q` after the five Newton iterations of the Rust code. -/
def qOf (σ : ℚ) : ℚ := (newton σ)^[5] (q0 σ)

def yvvDen (q : ℚ) : ℚ := (q + yvvReal) * ((q + yvvPair) ^ 2 + yvvImag ^ 2)

def yvvFeedback (q : ℚ) : ℚ × ℚ × ℚ :=
  (q * ((q + yvvPair) ^ 2 + yvvImag ^ 2 + 2 * (q + yvvReal) * (q + yvvPair)) / yvvDen q,
   -q ^ 2 * (3 * q + yvvReal + 2 * yvvPair) / yvvDen q,
   q ^ 3 / yvvDen q)

def yvvGain (q : ℚ) : ℚ := yvvReal * yvvNorm / yvvDen q

/-- Variance of one causal pass with feedback `(f₁,f₂,f₃)`. -/
def causalVar (f : ℚ × ℚ × ℚ) : ℚ :=
  let D0 := 1 - f.1 - f.2.1 - f.2.2
  let D1 := -(f.1 + 2 * f.2.1 + 3 * f.2.2)
  let D2 := -(2 * f.2.1 + 6 * f.2.2)
  (-D2 / D0 + D1 ^ 2 / D0 ^ 2 - D1 / D0)

/-- DC gain of one causal pass. -/
def causalDC (q : ℚ) : ℚ :=
  let f := yvvFeedback q
  yvvGain q / (1 - f.1 - f.2.1 - f.2.2)

/-- Effective variance of the forward + backward recursive filter at `σ`. -/
def yvvVariance (σ : ℚ) : ℚ := 2 * causalVar (yvvFeedback (qOf σ))

/-- Two-pass DC gain. -/
def yvvDC (σ : ℚ) : ℚ := causalDC (qOf σ) ^ 2

end CraftBlur
