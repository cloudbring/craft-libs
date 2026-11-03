# craft-tiledir

Persistent (structurally shared, `O(1)`-clone) 2D tile directories for undo history and
copy-on-write tiled canvases.

- `TileDirectory<V>`: common trait over `(tx, ty): (i32, i32)` coordinates providing `get`,
  `contains_key`, `get_mut`, `insert`, `remove`, `len`, `is_empty`, `iter` (in ascending
  lexicographic `(tx, ty)` order), `ptr_eq`, `shared_with`, `retain`, and `for_each_mut`.
- `SparseTileDir<V>`: persistent 16-way bitmap-indexed radix tree over unbounded signed `(tx, ty)`
  coordinates, generalized over `V` from PhotoCraft `crates/raster/src/tile_dir.rs`
  (`934bdc745033786975366063d0a55f8e6d552f51`).
- `DenseTileDir<V, const CHUNK: usize = 64>`: persistent chunked vector (`Arc<Vec<Arc<[Option<V>]>>>`)
  over a bounded `tiles_x × tiles_y` grid, generalized over `V` from TextureCraft
  `crates/texel/src/tile.rs` (`6aa37518d3c031e1f64869d74642871f36104ddd`).

## Semantic notes

- Both structures store `V` directly (applications pass `Arc<Tile>` so pixel buffers stay
  separately copy-on-write) and require `V: Clone` only on path-copying mutations.
- `TileDirectory::iter`, `SparseTileDir::iter`, and `DenseTileDir::iter` yield `((tx, ty), &V)` in
  ascending `(tx, ty)` order (`tx` first, then `ty`), matching PhotoCraft's `TileCoord::Ord` and
  `BTreeMap<(i32, i32), V>`. `DenseTileDir::iter_row_major` and `DenseTileDir::slots` preserve
  TextureCraft's linear `(ty, tx)` chunk scan.
- Neither PhotoCraft nor TextureCraft serializes the tile directory directly with `serde`;
  serialization remains in each application's container format.

## Benchmarks

Run with:

```sh
cargo test -p craft-tiledir --release --test bench -- --ignored --nocapture --test-threads=1
```

Measured locally in `craft-tiledir` (release profile):

| Scenario | Baseline | `SparseTileDir` | `DenseTileDir` |
| --- | ---: | ---: | ---: |
| PhotoCraft: 4096 tiles / 200 one-tile snapshots | `BTreeMap` 2.026 ms | 0.214 ms | 0.249 ms |
| PhotoCraft: 1024 tiles / full-canvas edit | `BTreeMap` 1.085 ms | 1.254 ms | 1.110 ms |
| TextureCraft: 4096 tiles / 200 snapshots | `Vec` 0.7589 ms | 0.0004 ms | 0.0005 ms |
| TextureCraft: 4096 tiles / 200 one-tile history edits | `Vec` 1.0006 ms | 0.3934 ms | 0.3003 ms |
| TextureCraft: 4096 tiles / 1 full-image history edit | `Vec` 4.0610 ms | 5.2871 ms | 4.1455 ms |
