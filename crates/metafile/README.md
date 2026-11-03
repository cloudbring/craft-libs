# craft-metafile

A single pure-Rust EMF, WMF (plain/placeable), and EMF+ reader, consolidated from
the first-party DeckCraft reader, WordCraft fixes/tests, and VectorCraft import
coverage. No writers, app dependencies, filesystem access, font resolution, or
rendering engines. Safe Rust; native and `wasm32-unknown-unknown`.

## API and scene

```rust
use craft_metafile::{Error, Kind, Limits, Picture};

fn read(bytes: &[u8]) -> Result<Picture, Error> {
    let _container: Option<Kind> = craft_metafile::sniff(bytes);
    craft_metafile::parse_with_limits(bytes, Limits::default())
}
```

- `parse(&[u8]) -> Result<Picture, Error>` uses the default limits.
- `parse_with_limits(&[u8], Limits) -> Result<Picture, Error>` sets ceilings.
- `is_metafile(&[u8]) -> bool` and `sniff(&[u8]) -> Option<Kind>` inspect
  signatures, not record validity. `Kind::{Emf, Wmf, PlaceableWmf}`; EMF+ is
  inside an EMF comment, not a fourth container.
- `Picture`, `Item::{Path, Pattern, Bitmap, Text}`, `Seg::{Move, Line, Cubic,
  Close}`, `Tile`, `Text`, `Font`, `HAlign`, `VAlign` are app-neutral. Coordinates
  are y-down. Clip regions attach to individual items; text has an additional
  ETO clip rectangle. Pattern fills share `Arc<Tile>` pixels. Cubics stay curves
  except where EMF+ clipping requires flattening.
- `Picture::{fit, place}`, `PlacedItem`, `Text::layout`, `TextLayout`,
  `crop_window`, `MAX_BITMAP_SIDE`, and `SYMBOL_CHARSET` support renderers.
  WordCraft's text fields are represented by DeckCraft's richer affine text
  transform and `Font`; its metric layout helper is retained, not source-compatible.

## Resource contract

| Limit | Default | Provenance |
|---|---:|---|
| `max_records` | 1,000,000 | New finite traversal cap; apps had none |
| `max_items` | 200,000 | WordCraft/DeckCraft, stricter than VectorCraft |
| `max_bitmap_pixels` | 40,000,000 | Strictest app decode cap, now aggregate |
| `max_bytes` | 268,435,456 | WordCraft/DeckCraft retained-data cap, now conservative cumulative charges |

The input length and allocation charges each must fit `max_bytes`. Records
reserve a conservative temporary-data allowance; items reserve raw and final
scene storage. Pixels, generated shading tiles, saved states, continuations and
large pattern copies are charged before allocation. Counters are shared across
EMF+ objects and nested metafiles, including discarded objects. Nesting is
limited to three child levels. Fixed parser state and allocator bookkeeping
are excluded; this is not an exact heap profiler. Lower limits are useful for
untrusted previews. Set the input limit before buffering a stream at the caller.

Exhausting a configurable limit returns `Error::LimitExceeded`, not a partial
success. Inherited fixed structural caps (handles, path points, clip complexity,
save-stack depth) still skip unsupported complexity or stop playback. Damaged
or unknown records may yield partial pictures; this is a reader, not a validator.
No panic handlers or unsafe code hide decoding failures.

## Provenance and layering

Source snapshots (first-party code only, no third-party reader implementations):

- DeckCraft `3e178d8f5a59c787a6a53a7ed0f052b3e4a1489f`,
  `crates/metafile/src/{lib,bytes,canvas,clip,cliprect,dib,emf,emfplus,geom,place,text,wmf,tests}.rs`:
  base reader, split into `scene.rs` and reexports. EMF+ was added by DeckCraft,
  not present in WordCraft. Each moved module names its source.
- WordCraft `b47079fc791595fd84bb499d64dcd3f0624ef35a`,
  `crates/metafile/src/{dib,geom,canvas,place}.rs`: crop-before-decode DIBs,
  exact empty-poly-group allocation, free-handle table, narrow crop placement
  and text metric layout. All crate tests are retained (duplicate overlapping
  cases intentionally remain in `wordcraft_tests.rs`). Limit assertions now
  expect explicit errors instead of successful truncation.
- VectorCraft `11862f61198c8f62c0b22b6c9a331cc5950ac98d`,
  `crates/metafile/src/import/{emf,wmf}.rs`: import-only comparison; added EMF
  extent scaling, arc direction/save-restore, abort-path and placeholder slots.
  Its writer/export is neither copied nor depended on.

Layering: **format/algorithm library**, below app document/import adapters and
renderers. Dependencies: `kurbo` (geometry), `thiserror`, `image` (bounded EMF+
compressed images, same features as DeckCraft). No internal app dependency
needed replacing. App migrations are intentionally a later ticket.

See [record coverage and gaps](COVERAGE.md), [fixture/oracle provenance](tests/fixtures/README.md),
and `.github/workflows/metafile.yml` for native and wasm gates.
