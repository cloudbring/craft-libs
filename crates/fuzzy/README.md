# craft-fuzzy

Dependency-free, safe Rust command-palette subsequence scoring. Construct a `Matcher` per item and reuse it as the query changes; `match_query` returns a score and original Unicode scalar indices. `rank` orders by descending score, shorter original character length, then input order; an empty query retains input order. Scores across different queries are not comparable. Contiguous normalized substrings always outrank scattered subsequences: a word-start substring wins over an interior substring, earlier words win, and shorter candidates win when the word position is the same. For scattered matches, the dynamic program favors word starts, path separators, camelCase, adjacency and exact case, choosing an optimal alignment rather than the first one. ASCII queries use a cheap subsequence pre-filter and cached normalized candidates.

`Normalization::CaseInsensitive` applies Unicode lowercase. `Normalization::NfkdFolded` additionally uses a generated Unicode 13.0.0 NFKD decomposition table and removes marks, making `é` and `e`, or `ﬁ` and `fi`, match. Expanded characters highlight once, at their original character index. This is accent-insensitive search, not locale-sensitive case folding. The Unicode table was generated from Python's `unicodedata` 13.0.0 by applying NFKD and removing characters in Unicode categories M*. Newer Unicode characters receive lowercase matching without decomposition.

```rust
use craft_fuzzy::{Matcher, Normalization};
let item = Matcher::new("Paste Special", Normalization::CaseInsensitive);
assert_eq!(item.match_query("ps").map(|m| m.positions), Some(vec![0, 6]));
```

## Migration differences

The differential tests include scorer copies from `photocraft/crates/ui-egui/src/dialogs/palette.rs:9`, `soundcraft/crates/ui-egui/src/palette.rs:34`, and `craft-anything/crates/ui-egui/src/palette.rs:40`. All agree that `Open` outranks `Copy` for `op`. PhotoCraft and Craft Anything greedily choose the first `s` in `Paste Special` for `ps`; this matcher selects the second word's `S`. Contiguous substrings, even interior ones, beat scattered word initials: `Filter Gallery` correctly outranks a `Blur` command whose match is scattered through its menu path for `filter`. Craft Anything discards query whitespace; this matcher treats it literally. PDFCraft's reordered-word fallback and flat subsequence score, VectorCraft's unordered word-substring filtering, and FigCraft's substring-only filtering are replaced by ordered subsequence matching.

Run the manual benchmark with `cargo test -p craft-fuzzy --release --test matcher benchmark_palette -- --ignored --nocapture` (10,000 commands × 100 queries).
Run `cargo test -p craft-fuzzy --release --test matcher benchmark_photocraft_shape -- --ignored --nocapture` for a synthetic 766-item comparison; it is not the PhotoCraft catalogue benchmark.
