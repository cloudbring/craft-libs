# Lean-generated conformance vectors

`lean.csv` is **generated, never hand-edited**. All eight numeric columns are
reduced exact fractions `numerator/positive_denominator` computed by the proved
Lean definitions. `premul` is `Co`; `color` is `Co/αo` with zero for zero alpha.
There are 4,075 data rows plus one header. All twelve modes are represented.

From the repository root, after installing elan without modifying the shell PATH:

```sh
export PATH="$HOME/.elan/bin:$PATH"
cd crates/blend/lean
lake exe cache get Mathlib/Tactic/NormNum.lean Mathlib/Tactic/Linarith.lean \
  Mathlib/Tactic/Ring.lean Mathlib/Tactic/SplitIfs.lean \
  Mathlib/Data/Real/Sqrt.lean Mathlib/Data/Rat/Sqrt.lean Mathlib/Data/Rat/Cast/Order.lean
lake build
lake env lean Generate.lean > ../tests/vectors/lean.csv.new
cmp ../tests/vectors/lean.csv ../tests/vectors/lean.csv.new
rm ../tests/vectors/lean.csv.new
```

To intentionally regenerate after a reviewed change, replace the `cmp` and `rm`
with `mv ../tests/vectors/lean.csv.new ../tests/vectors/lean.csv`, then run
`cargo test -p craft-blend -- --nocapture` from the repository root. A new checkout
uses the committed `lean-toolchain` and `lake-manifest.json`; do not run `lake update`
unless intentionally reviewing a dependency change. The cache command fetches
only the specified modules and their transitive dependencies.

`Generate.lean` uses `#eval generate`, not precomputed Rust/Python results.
The finite table excludes only cases whose active SoftLight root is irrational.
That exclusion is a mathematical requirement, not an error tolerance; see
`../../TRUST.md` and theorem `no_rational_sqrt_half`. Separate Rust tests ensure
nonsquare SoftLight returns an error instead of an approximation.

CI runs Cargo against the committed CSV, and the `Blend Lean` workflow runs
`lean/check.sh` (build, axiom audit, regenerate and `cmp`). Run it locally the same way.
