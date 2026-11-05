import CraftResample.Defs

/-!
# Polyphase equals filter-then-decimate

For every sequence `x : ℤ → R` (no support hypothesis),
`polyOut` at `(i, p)` with `i L + p = n M` equals `(↓M (h * ↑L x))[n]`, and the integer accumulator
used by the Rust code walks exactly those `(i, p)`.
-/

namespace CraftResample

open Finset

variable {R : Type*} [CommRing R]

/-- Splitting `range (L T)` into residues mod `L`. -/
lemma sum_range_mul_split (f : ℕ → R) (L T : ℕ) :
    ∑ k ∈ range (L * T), f k = ∑ p ∈ range L, ∑ q ∈ range T, f (p + q * L) := by
  induction T with
  | zero => simp
  | succ T ih =>
    rw [Nat.mul_succ, Finset.sum_range_add, ih]
    have key : ∀ p, ∑ q ∈ range (T + 1), f (p + q * L) = ∑ q ∈ range T, f (p + q * L) + f (p + T * L) :=
      fun p => Finset.sum_range_succ _ _
    simp_rw [key]
    rw [Finset.sum_add_distrib]
    congr 1
    exact Finset.sum_congr rfl fun p _ => by congr 1; ring

lemma upsample_mul {L : ℕ} (hL : 0 < L) (x : ℤ → R) (j : ℤ) : upsample L x (j * L) = x j := by
  unfold upsample
  rw [if_pos (Dvd.intro_left _ rfl), Int.mul_ediv_cancel _ (by exact_mod_cast hL.ne')]

lemma upsample_off {L : ℕ} (x : ℤ → R) {p p' : ℕ} (hp : p < L) (hp' : p' < L) (hne : p' ≠ p) (j : ℤ) :
    upsample L x (j * L + ((p : ℤ) - p')) = 0 := by
  unfold upsample
  rw [if_neg]
  intro hd
  have hd2 : (L : ℤ) ∣ (p : ℤ) - p' := (Int.dvd_add_right (Dvd.intro_left _ rfl)).1 hd
  have hlt : |(p : ℤ) - p'| < L := by
    rw [abs_lt]; constructor <;> omega
  have := Int.eq_zero_of_abs_lt_dvd hd2 hlt
  omega

/-- The core identity at upsampled index `i L + p`. -/
theorem fir_eq_polyOut {L T N : ℕ} (hL : 0 < L) (hN : N ≤ L * T) (h : ℕ → R) (x : ℤ → R)
    (i : ℤ) (p : ℕ) (hp : p < L) :
    fir h N (upsample L x) (i * L + p) = polyOut h N L T x i p := by
  unfold fir polyOut
  have h1 : ∑ k ∈ range N, h k * upsample L x (i * L + p - k)
      = ∑ k ∈ range N, tap h N k * upsample L x (i * L + p - k) := by
    apply Finset.sum_congr rfl
    intro k hk
    simp [tap, Finset.mem_range.1 hk]
  have h2 : ∑ k ∈ range N, tap h N k * upsample L x (i * L + p - k)
      = ∑ k ∈ range (L * T), tap h N k * upsample L x (i * L + p - k) := by
    apply Finset.sum_subset (Finset.range_subset_range.2 hN)
    intro k _ hk
    have : ¬ k < N := fun hlt => hk (Finset.mem_range.2 hlt)
    simp [tap, this]
  rw [h1, h2, sum_range_mul_split, Finset.sum_eq_single p]
  · apply Finset.sum_congr rfl
    intro q _
    have : (i * L + p - ((p + q * L : ℕ) : ℤ)) = (i - q) * L := by push_cast; ring
    rw [this, upsample_mul hL]
  · intro p' hp' hne
    apply Finset.sum_eq_zero
    intro q _
    have : (i * L + p - ((p' + q * L : ℕ) : ℤ)) = (i - q) * L + ((p : ℤ) - p') := by push_cast; ring
    rw [this, upsample_off x hp (Finset.mem_range.1 hp') hne]
    simp
  · intro hnot; exact absurd (Finset.mem_range.2 hp) hnot

/-- One accumulator step moves `(a div L, a mod L)` to `((a+M) div L, (a+M) mod L)`. -/
lemma step_div_mod {L : ℕ} (hL : 0 < L) (a M : ℕ) :
    step L M (((a / L : ℕ) : ℤ), a % L) = ((((a + M) / L : ℕ) : ℤ), (a + M) % L) := by
  unfold step
  have h1 : a % L < L := Nat.mod_lt _ hL
  have h2 : M % L < L := Nat.mod_lt _ hL
  have ea := Nat.div_add_mod a L
  have eM := Nat.div_add_mod M L
  by_cases hc : a % L + M % L ≥ L
  · have e : a + M = (a % L + M % L - L) + L * (a / L + M / L + 1) := by
      rw [mul_add, mul_add]; omega
    have hlt : a % L + M % L - L < L := by omega
    have hd : (a + M) / L = a / L + M / L + 1 := by
      rw [e, Nat.add_mul_div_left _ _ hL, Nat.div_eq_of_lt hlt]; simp
    have hm : (a + M) % L = a % L + M % L - L := by
      rw [e, Nat.add_mul_mod_self_left, Nat.mod_eq_of_lt hlt]
    rw [if_pos hc, hd, hm]
    ext
    · push_cast; ring
    · rfl
  · have hc' : a % L + M % L < L := by omega
    have e : a + M = (a % L + M % L) + L * (a / L + M / L) := by
      rw [mul_add]; omega
    have hd : (a + M) / L = a / L + M / L := by
      rw [e, Nat.add_mul_div_left _ _ hL, Nat.div_eq_of_lt hc']; simp
    have hm : (a + M) % L = a % L + M % L := by
      rw [e, Nat.add_mul_mod_self_left, Nat.mod_eq_of_lt hc']
    rw [if_neg hc, hd, hm]
    ext
    · push_cast; rfl
    · rfl

/-- The accumulator never drifts: after `k` steps it is exactly `((first+k) M) divmod L`. -/
theorem accState_eq {L : ℕ} (hL : 0 < L) (M first k : ℕ) :
    accState L M first k = ((((first + k) * M / L : ℕ) : ℤ), (first + k) * M % L) := by
  induction k with
  | zero => simp [accState]
  | succ k ih =>
    have : accState L M first (k + 1) = step L M (accState L M first k) := by
      unfold accState; rw [Function.iterate_succ_apply']
    rw [this, ih, step_div_mod hL]
    have e : (first + (k + 1)) * M = (first + k) * M + M := by ring
    rw [e]

/-- Main theorem: the streamed polyphase output (accumulator and all) is exactly
`↓M (h * ↑L x)` at output index `first + k`, for every input `x`. -/
theorem streamOut_eq_direct {L T N : ℕ} (hL : 0 < L) (hN : N ≤ L * T) (M : ℕ) (h : ℕ → R)
    (x : ℤ → R) (first k : ℕ) :
    streamOut h N L M T x first k = direct h N L M x (first + k) := by
  unfold streamOut direct downsample
  rw [accState_eq hL]
  simp only
  have hp : (first + k) * M % L < L := Nat.mod_lt _ hL
  have e : (((first + k) * M : ℕ) : ℤ)
      = ((((first + k) * M / L : ℕ) : ℤ)) * L + (((first + k) * M % L : ℕ) : ℤ) := by
    have e0 := Nat.div_add_mod' ((first + k) * M) L
    generalize (first + k) * M = X at e0 ⊢
    exact_mod_cast e0.symm
  rw [e]
  exact (fir_eq_polyOut hL hN h x _ _ hp).symm

/-- Without the accumulator: output `n` is `polyOut` at `(n M div L, n M mod L)`. -/
theorem polyOut_eq_direct {L T N : ℕ} (hL : 0 < L) (hN : N ≤ L * T) (M : ℕ) (h : ℕ → R)
    (x : ℤ → R) (n : ℕ) :
    polyOut h N L T x ((n * M / L : ℕ) : ℤ) (n * M % L) = direct h N L M x n := by
  have := streamOut_eq_direct hL hN M h x n 0
  simpa [streamOut, accState_eq hL] using this

/-- Rust's `taps_per_phase = ⌈N / L⌉` satisfies the hypothesis `N ≤ L T`. -/
lemma le_mul_tapsPerPhase {L : ℕ} (hL : 0 < L) (N : ℕ) : N ≤ L * tapsPerPhase N L := by
  unfold tapsPerPhase
  have := Nat.div_add_mod (N + L - 1) L
  have h2 := Nat.mod_lt (N + L - 1) hL
  omega

/-- The theorem at exactly the Rust table size. -/
theorem rust_polyphase_eq_direct {L N : ℕ} (hL : 0 < L) (M : ℕ) (h : ℕ → R) (x : ℤ → R)
    (first k : ℕ) :
    streamOut h N L M (tapsPerPhase N L) x first k = direct h N L M x (first + k) :=
  streamOut_eq_direct hL (le_mul_tapsPerPhase hL N) M h x first k

/-! ## DC gain / normalisation -/

/-- Constant input window `c` gives `phaseSum * c`. -/
theorem polyOut_const (h : ℕ → R) (N L T : ℕ) (x : ℤ → R) (i : ℤ) (p : ℕ) (c : R)
    (hx : ∀ q < T, x (i - q) = c) :
    polyOut h N L T x i p = phaseSum h N L T p * c := by
  unfold polyOut phaseSum
  rw [Finset.sum_mul]
  exact Finset.sum_congr rfl fun q hq => by rw [hx q (Finset.mem_range.1 hq)]

/-- If every phase sums to `1`, a constant window passes unchanged. -/
theorem polyOut_dc {h : ℕ → R} {N L T : ℕ} (hs : ∀ p < L, phaseSum h N L T p = 1) (x : ℤ → R)
    (i : ℤ) {p : ℕ} (hp : p < L) (c : R) (hx : ∀ q < T, x (i - q) = c) :
    polyOut h N L T x i p = c := by
  rw [polyOut_const h N L T x i p c hx, hs p hp, one_mul]

/-- The whole prototype's sum is the sum of the phase sums. -/
theorem proto_sum_eq {L T N : ℕ} (hN : N ≤ L * T) (h : ℕ → R) :
    ∑ k ∈ range N, h k = ∑ p ∈ range L, phaseSum h N L T p := by
  unfold phaseSum
  rw [← sum_range_mul_split (tap h N)]
  have h1 : ∑ k ∈ range N, h k = ∑ k ∈ range N, tap h N k :=
    Finset.sum_congr rfl fun k hk => by simp [tap, Finset.mem_range.1 hk]
  rw [h1]
  apply Finset.sum_subset (Finset.range_subset_range.2 hN)
  intro k _ hk
  have : ¬ k < N := fun hlt => hk (Finset.mem_range.2 hlt)
  simp [tap, this]

/-- Phases each summing to `s` give a prototype summing to `L * s`
(`s = 1`: the zero-stuffing gain `L`). -/
theorem proto_sum_of_phase_sum {L T N : ℕ} (hN : N ≤ L * T) (h : ℕ → R) (s : R)
    (hs : ∀ p < L, phaseSum h N L T p = s) :
    ∑ k ∈ range N, h k = (L : R) * s := by
  rw [proto_sum_eq hN]
  rw [Finset.sum_congr rfl fun p hp => hs p (Finset.mem_range.1 hp)]
  simp

/-- DC through the whole streamed polyphase resampler: if the input is constant `c` on the
`T` samples the output reads, the output is `c` (every phase normalised to 1). -/
theorem streamOut_dc {L T N : ℕ} (hL : 0 < L) (M : ℕ) {h : ℕ → R}
    (hs : ∀ p < L, phaseSum h N L T p = 1) (x : ℤ → R) (first k : ℕ) (c : R)
    (hx : ∀ q < T, x ((accState L M first k).1 - q) = c) :
    streamOut h N L M T x first k = c := by
  unfold streamOut
  refine polyOut_dc hs x _ ?_ c hx
  rw [accState_eq hL]
  exact Nat.mod_lt _ hL

/-- DC gain of the direct form, same hypothesis, through the main theorem. -/
theorem direct_dc {L T N : ℕ} (hL : 0 < L) (hN : N ≤ L * T) (M : ℕ) {h : ℕ → R}
    (hs : ∀ p < L, phaseSum h N L T p = 1) (x : ℤ → R) (n : ℕ) (c : R)
    (hx : ∀ q < T, x (((n * M / L : ℕ) : ℤ) - q) = c) :
    direct h N L M x n = c := by
  rw [← polyOut_eq_direct hL hN]
  exact polyOut_dc hs x _ (Nat.mod_lt _ hL) c hx

/-! ## Output length -/

/-- Beyond `outLen len N L M` the direct-form output of an input supported on `[0, len)` is zero,
so Rust's `out_len` loses nothing. -/
theorem direct_zero_after {L M N len : ℕ} (hL : 0 < L) (hM : 0 < M) (hlen : 0 < len)
    (h : ℕ → R) (x : ℤ → R) (hx : ∀ j : ℤ, j < 0 ∨ (len : ℤ) ≤ j → x j = 0)
    (n : ℕ) (hn : outLen len N L M ≤ n) : direct h N L M x n = 0 := by
  have hge : (len - 1) * L + N ≤ n * M := by
    unfold outLen at hn
    have := Nat.lt_div_mul_add (a := (len - 1) * L + N + M - 1) hM
    have h3 : (len - 1) * L + N + M - 1 < ((len - 1) * L + N + M - 1) / M * M + M := this
    have h4 : ((len - 1) * L + N + M - 1) / M * M ≤ n * M := Nat.mul_le_mul_right _ hn
    omega
  unfold direct downsample fir
  apply Finset.sum_eq_zero
  intro k hk
  have hk' := Finset.mem_range.1 hk
  have : upsample L x (((n * M : ℕ) : ℤ) - k) = 0 := by
    unfold upsample
    split_ifs with hd
    · obtain ⟨j, hj⟩ := hd
      have hjL : ((n * M : ℕ) : ℤ) - k = L * j := hj
      rw [hj, Int.mul_ediv_cancel_left _ (by exact_mod_cast hL.ne')]
      apply hx
      right
      by_contra hlt
      push_neg at hlt
      have h5 : (j : ℤ) ≤ len - 1 := by omega
      have h6 : (L : ℤ) * j ≤ L * (len - 1) := Int.mul_le_mul_of_nonneg_left h5 (by exact_mod_cast hL.le)
      have h7 : (((len - 1) * L + N : ℕ) : ℤ) ≤ ((n * M : ℕ) : ℤ) := by exact_mod_cast hge
      have hk2 : (k : ℤ) < N := by exact_mod_cast hk'
      push_cast [Nat.cast_sub (Nat.succ_le_of_lt hlen)] at h7 h6 hjL
      nlinarith
    · rfl
  rw [this, mul_zero]

end CraftResample
