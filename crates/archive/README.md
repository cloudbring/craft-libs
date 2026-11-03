# craft-archive

Reusable archive input guards, without app-specific extraction or format policy.

- `BoundedInflate::new(reader, Limits)` streams raw, zlib or gzip DEFLATE with hard input/output byte ceilings and an expansion-ratio ceiling. `Wrapper::Auto` sniffs the envelope; explicit wrapper variants avoid ambiguity. `read_chunk` returns typed failures, and `accounting` reports compressed bytes consumed and decoded bytes delivered. `inflate_bounded` collects a bounded slice in one call.
- `ZipGuard::new(zip::ZipArchive::new(reader)?, ZipLimits)` rejects duplicate central-directory names, traversal/absolute/Windows paths, symlinks, excessive entry counts and declared sizes. `read_entry(index)` checks actual bytes while decoding and completes the entry to validate its CRC. It returns bytes, not a filesystem path. The cumulative cap applies to bytes returned across calls, including repeated reads.
- `PngChunks::new(bytes, max_chunk_bytes)` validates the signature, chunk type, length and CRC before yielding each borrowed chunk. It does not validate PNG image semantics or enforce chunk order.

The inflate accounting is the bounded-decoder verification target in artcraft-algorithms #21/#40; `lean-zip` is a future differential oracle, not a runtime dependency. Differential tests include the bounded zlib wrapper from `vectorcraft/crates/eps/src/ps.rs:286`.
