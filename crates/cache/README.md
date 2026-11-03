# craft-cache

`CostLru` offers expected O(1) lookup, insertion and eviction per entry, including pinned entries, a non-touching `peek`, oldest-first iteration and counters. Each insertion may evict multiple entries. `Budget` shares atomic cost reservations across caches; caches evict only their own unpinned entries and reject insertion if space remains unavailable. Replacing an entry removes the old value even when admission fails. Zero-cost entries are allowed; callers must bound their count separately. Dropping or clearing a cache releases its reservations.

`FrameSweep` marks inserts and successful gets; `end_frame` discards entries not marked since the previous call. `peek` does not mark an entry.

`#[derive(Fingerprint)]` generates an encoding of every struct field in declaration order, including newly added fields; unsupported fields fail compilation. Encodings are explicitly specified rather than using Rust's unspecified `Hash` representation. FNV-1a 64 is in-tree until craft-hash lands; fingerprints are not collision resistant. Changes to struct fields or field ordering change fingerprints. Floating-point values use their bit patterns, including signed zero and NaN payloads.

Run the ignored comparison with `cargo test -p craft-cache --release bench_insert_vs_sort -- --ignored --nocapture`; it compares one million inserts with the former sort-on-eviction design in `filmcraft/crates/media/src/cache.rs:61` (inventory #66, before filmcraft #104).
