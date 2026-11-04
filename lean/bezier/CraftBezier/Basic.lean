import Mathlib.RingTheory.Polynomial.Bernstein
import Mathlib.Algebra.Module.Basic
import Mathlib.Tactic.Module

/-!
# Bézier curves in Bernstein form and de Casteljau evaluation

A degree-`n` Bézier curve with control points `P 0, …, P n` (a function `ℕ → E`, only the first
`n + 1` values are used) is `bez n P t = ∑ ν, bern n ν t • P ν`, where `bern n ν t` is the
evaluation of Mathlib's `bernsteinPolynomial` (`eval_bernstein`).  Everything in this file is
generic over a commutative ring `R` and an `R`-module `E`, so the very same definitions are run
over `ℚ × ℚ` by `Generate.lean` (exact test vectors) and proved about over `ℝ`.
-/

namespace CraftBezier

open Finset

section Generic

variable {R : Type*} [CommRing R] {E : Type*} [AddCommGroup E] [Module R E]

/-- Computable Bernstein basis value `C(n,ν) t^ν (1-t)^(n-ν)`. -/
def bern (n ν : ℕ) (t : R) : R := (n.choose ν : R) * t ^ ν * (1 - t) ^ (n - ν)

/-- `bern` is the evaluation of Mathlib's `bernsteinPolynomial`. -/
theorem eval_bernstein (n ν : ℕ) (t : R) :
    (bernsteinPolynomial R n ν).eval t = bern n ν t := by
  simp [bernsteinPolynomial, bern]

/-- Bézier curve of degree `n` in Bernstein form. -/
def bez (n : ℕ) (P : ℕ → E) (t : R) : E := ∑ ν ∈ range (n + 1), bern n ν t • P ν

/-- One de Casteljau step: the `i`-th point is `(1-t) P i + t P (i+1)`. -/
def step (t : R) (P : ℕ → E) : ℕ → E := fun i => (1 - t) • P i + t • P (i + 1)

/-- Drop the first control point. -/
def shift (P : ℕ → E) : ℕ → E := fun i => P (i + 1)

/-- `k` rounds of de Casteljau at parameter `t`. -/
def lvl : ℕ → R → (ℕ → E) → (ℕ → E)
  | 0, _, P => P
  | k + 1, t, P => lvl k t (step t P)

/-- Left sub-curve control points of the subdivision at `s`: `L k = lvl k s P 0`. -/
def leftPt (s : R) (P : ℕ → E) : ℕ → E := fun k => lvl k s P 0

/-- Right sub-curve control points of the degree-`n` subdivision at `s`: `R k = lvl (n-k) s P k`. -/
def rightPt (n : ℕ) (s : R) (P : ℕ → E) : ℕ → E := fun k => lvl (n - k) s P k

theorem bern_zero_of_lt {n ν : ℕ} (h : n < ν) (t : R) : bern n ν t = 0 := by
  simp [bern, Nat.choose_eq_zero_of_lt h]

theorem bern_succ_zero (n : ℕ) (t : R) : bern (n + 1) 0 t = (1 - t) * bern n 0 t := by
  simp [bern, pow_succ]; ring

/-- The Bernstein recurrence. -/
theorem bern_succ_succ (n ν : ℕ) (t : R) :
    bern (n + 1) (ν + 1) t = (1 - t) * bern n (ν + 1) t + t * bern n ν t := by
  rcases lt_trichotomy n ν with h | h | h
  · rw [bern_zero_of_lt (by omega), bern_zero_of_lt (by omega), bern_zero_of_lt h]; simp
  · subst h
    rw [bern_zero_of_lt (by omega : n < n + 1)]
    simp [bern, pow_succ]; ring
  · obtain ⟨k, rfl⟩ : ∃ k, n = ν + 1 + k := ⟨n - (ν + 1), by omega⟩
    have e1 : ν + 1 + k + 1 - (ν + 1) = k + 1 := by omega
    have e2 : ν + 1 + k - (ν + 1) = k := by omega
    have e3 : ν + 1 + k - ν = k + 1 := by omega
    unfold bern
    rw [e1, e2, e3, Nat.choose_succ_succ]
    push_cast
    ring

/-- Bézier recurrence: `B_{P0..Pn+1} = (1-t) B_{P0..Pn} + t B_{P1..Pn+1}`. -/
theorem bez_succ (n : ℕ) (P : ℕ → E) (t : R) :
    bez (n + 1) P t = (1 - t) • bez n P t + t • bez n (shift P) t := by
  have h1 : ∀ ν ∈ range (n + 1), bern (n + 1) (ν + 1) t • P (ν + 1) =
      (1 - t) • (bern n (ν + 1) t • P (ν + 1)) + t • (bern n ν t • shift P ν) := by
    intro ν _
    rw [bern_succ_succ, add_smul, mul_smul, mul_smul]; rfl
  have hA : ∑ ν ∈ range (n + 1), bern n (ν + 1) t • P (ν + 1) =
      ∑ ν ∈ range n, bern n (ν + 1) t • P (ν + 1) := by
    rw [sum_range_succ, bern_zero_of_lt (by omega : n < n + 1), zero_smul, add_zero]
  unfold bez
  rw [sum_range_succ' _ (n + 1), sum_congr rfl h1, sum_add_distrib, ← smul_sum, ← smul_sum, hA,
    bern_succ_zero, mul_smul, sum_range_succ' (fun ν => bern n ν t • P ν) n, smul_add]
  abel

theorem bez_congr {n : ℕ} {P Q : ℕ → E} (h : ∀ k ≤ n, P k = Q k) (t : R) :
    bez n P t = bez n Q t := by
  unfold bez
  exact sum_congr rfl fun k hk => by rw [h k (by simpa [Nat.lt_succ_iff] using hk)]

theorem bez_zero (P : ℕ → E) (t : R) : bez 0 P t = P 0 := by simp [bez, bern]

/-- A de Casteljau step is a convex-combination of two curves. -/
theorem bez_step (n : ℕ) (s : R) (P : ℕ → E) (w : R) :
    bez n (step s P) w = (1 - s) • bez n P w + s • bez n (shift P) w := by
  unfold bez
  rw [smul_sum, smul_sum, ← sum_add_distrib]
  refine sum_congr rfl fun ν _ => ?_
  simp only [step, shift, smul_add]
  rw [smul_comm (bern n ν w) (1 - s), smul_comm (bern n ν w) s]

/-- (c) De Casteljau evaluation equals the Bernstein form. -/
theorem deCasteljau_eq_bez (n : ℕ) (P : ℕ → E) (t : R) : lvl n t P 0 = bez n P t := by
  induction n generalizing P with
  | zero => simp [lvl, bez_zero]
  | succ n ih => rw [lvl, ih, bez_step, bez_succ]

theorem lvl_shift (k : ℕ) (s : R) (P : ℕ → E) (i : ℕ) :
    lvl k s (shift P) i = lvl k s P (i + 1) := by
  induction k generalizing P with
  | zero => rfl
  | succ k ih => exact ih (step s P)

/-- (d) Left half of de Casteljau subdivision at `s` reparametrises the curve on `[0, s]`. -/
theorem subdiv_left (n : ℕ) (s : R) (P : ℕ → E) (u : R) :
    bez n (leftPt s P) u = bez n P (s * u) := by
  induction n generalizing P with
  | zero => simp [bez_zero, leftPt, lvl]
  | succ n ih =>
    have hs : shift (leftPt s P) = leftPt s (step s P) := rfl
    rw [bez_succ, hs, ih, ih, bez_step, bez_succ]
    module

/-- (d) Right half of de Casteljau subdivision at `s` reparametrises the curve on `[s, 1]`. -/
theorem subdiv_right (n : ℕ) (s : R) (P : ℕ → E) (u : R) :
    bez n (rightPt n s P) u = bez n P (s + (1 - s) * u) := by
  induction n generalizing P with
  | zero => simp [bez_zero, rightPt, lvl]
  | succ n ih =>
    have h1 : bez n (rightPt (n + 1) s P) u = bez n (rightPt n s (step s P)) u := by
      refine bez_congr (fun k hk => ?_) u
      have : n + 1 - k = (n - k) + 1 := by omega
      simp only [rightPt, this, lvl]
    have h2 : shift (rightPt (n + 1) s P) = rightPt n s (shift P) := by
      funext k
      simp only [shift, rightPt, Nat.add_sub_add_right, lvl_shift]
    rw [bez_succ, h1, h2, ih, ih, bez_step, bez_succ]
    module

end Generic

end CraftBezier
