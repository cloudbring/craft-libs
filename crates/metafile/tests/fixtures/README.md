# Metafile fixture and oracle provenance

No checked-in `.emf` or `.wmf` fixtures were found in any of the three app
repositories at the snapshots named in the crate README. These eight tiny
synthetic files derive from DeckCraft/WordCraft `crates/metafile/src/tests.rs`
builders (`wmf`, `emf`, `wrec`, `erec`) and DeckCraft's `plus_comment`,
`plus_header`, `emfplus_only_fill_rects_with_argb_colour`. First-party,
MIT OR Apache-2.0; no external artwork. Hex text is the entire byte stream,
whitespace ignored, keeping provenance and mutations reviewable. Every decoded
fixture is under 300 bytes, far below the 200 KB fixture limit.

On November 3, 2026, the unmodified WordCraft reader was compiled via a temporary
standalone Cargo manifest: library path pointing at WordCraft's original
`crates/metafile/src/lib.rs`, edition 2024, dependencies kurbo 0.13 and thiserror 2.
The oracle source is retained in `../oracles/wordcraft_bounds.rs`. It parses
each hex fixture, builds `kurbo::BezPath`s from path items, computes exact cubic
extrema with `Shape::bounding_box`, and unions the results. These are geometric
path bounds (without stroke expansion), in the normalized picture coordinates.
The actual output was recorded as constants in `../fixtures.rs`:

| Fixture | WordCraft `[x0, y0, x1, y1]` |
|---|---|
| rectangle.wmf | `[0, 0, 100, 50]` |
| placeable.wmf | `[0.5, 1, 5.5, 3.5]` |
| mapped.wmf | `[10, 20, 110, 70]` |
| rectangle.emf | `[7.08661413192749, 14.17322826385498, 77.9527587890625, 49.606300354003906]` |
| bezier.emf | `[7.08661413192749, 7.08661413192749, 134.6456756591797, 49.606297850608826]` |
| ellipse.emf | `[14.17322826385498, 7.08661413192749, 127.55905151367188, 63.77952575683594]` |
| polypolygon.emf | `[7.08661413192749, 7.08661413192749, 127.55905151367188, 56.69291305541992]` |
| plus.emf | No paths: WordCraft does not play EMF+. Shared expected bounds `[0,0,30,40]`. |

The differential test checks every comparable fixture to 0.0001 units. EMF+
absence is recorded explicitly, not silently omitted; its shared result is
checked separately. The fixture set is nonempty, so the harness is not ignored.
There is no baseline fallback that generates numbers using the shared reader.

Reproduction manifest (outside either source workspace):

```toml
[package]
name = "wordcraft-metafile-oracle"
version = "0.0.0"
edition = "2024"
[workspace]
[lib]
path = "/absolute/path/to/wordcraft/crates/metafile/src/lib.rs"
[[bin]]
name = "bounds"
path = "/absolute/path/to/craft-libs/crates/metafile/tests/oracles/wordcraft_bounds.rs"
[dependencies]
kurbo = "0.13"
thiserror = "2"
```

Run that manifest's `bounds` binary with `tests/fixtures/*.hex` as arguments,
setting `CARGO_TARGET_DIR` to a disposable build directory. Nothing in the
WordCraft repository (including its lockfile) is changed.

The seeded xorshift mutation test performs **2,000 iterations per fixture**
(16,000 cases): alternating truncation and bit flips, with periodic four-byte
all-ones fields. Every mutation runs default and tight-limit parsing under
`catch_unwind`. Existing app hostile-input suites remain, too. This is a
deterministic regression harness, not a claim of exhaustive fuzz coverage.
