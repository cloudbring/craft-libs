# craft-diff

The anchored, budgeted sequence diff behind both Craft Apps' document compare:
`diff(a, b, budget)` turns two token sequences into range `Op`s — equal, inserted,
deleted, replaced — generic over any `Hash + Eq` token. The pipeline: common edge
trim, bounded Myers (saved frontiers) for segments within the budget, patience
anchors (tokens unique on both sides), longest-common-run anchors where repeated
vocabulary leaves nothing unique (rolling hash, verified by comparison),
recursion per gap under depth and work budgets, and a deterministic replace
fallback. No dependencies, no `unsafe`; complexity and the determinism guarantee
are documented on the crate.

Extracted from two implementations that had converged on one algorithm: pdfcraft
`crates/compare/src/lib.rs` (974f4be4b, #54) and wordcraft
`crates/engine/src/cmd/tools/compare.rs` (4912b24b, #53). Differences found and
settled: pdfcraft diffed pre-hashed `u64` words with no work budget and Myers
frontiers as `usize`; wordcraft diffed `DiffKey` tokens, verified anchors by
equality, kept `u32` frontiers and charged each middle the old quadratic table's
area (`MAX_LCS²` upfront, one budget shared across a document's pairs, so pairs
past it were replaced whole). craft-diff takes the wordcraft shape — generic
tokens, anchors verified by equality — but charges what actually allocates:
every frontier cell Myers saves plus the anchor search's element visits, against
`Budget::work_units`, per call. An app that wants wordcraft's shared
document-level budget passes smaller per-call budgets. Neither app detects
moved blocks (wordcraft's `moves` option is accepted but not honoured), so there
is no `moved_blocks`; tokenization, change rendering and wordcraft's
character-level noise folding (`collapse_noise_matches`) stay in the apps. The
differential tests run both apps' implementations verbatim (under
`tests/oracles/`, cited) against this crate on their fixtures.
