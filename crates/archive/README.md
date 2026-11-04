# craft-archive

Reusable archive input guards, without app-specific extraction or format policy.

- `BoundedInflate::new(reader, Limits)` streams raw, zlib or gzip DEFLATE with hard input/output byte ceilings and an expansion-ratio ceiling. `Wrapper::Auto` sniffs the envelope; explicit wrapper variants avoid ambiguity. `read_chunk` returns typed failures, and `accounting` reports compressed bytes consumed and decoded bytes delivered. `inflate_bounded` collects a bounded slice in one call.
- `ZipGuard::new(zip::ZipArchive::new(reader)?, ZipLimits)` rejects duplicate central-directory names, traversal/absolute/Windows paths, symlinks, excessive entry counts and declared sizes. `read_entry(index)` checks actual bytes while decoding and completes the entry to validate its CRC. It returns bytes, not a filesystem path. The cumulative cap applies to bytes returned across calls, including repeated reads.
- `PngChunks::new(bytes, max_chunk_bytes)` validates the signature, chunk type, length and CRC before yielding each borrowed chunk. It does not validate PNG image semantics or enforce chunk order.

The inflate accounting is the bounded-decoder verification target in artcraft-algorithms #21/#40; `lean-zip` is a future differential oracle, not a runtime dependency. Differential tests include the bounded zlib wrapper from `vectorcraft/crates/eps/src/ps.rs:286`.

## Entry-name policy: backslashes

Some third-party writers (seen in the corpus: `xl\styles.xml` in three `.xlsx` files) store Windows separators in entry names. The ZIP specification requires `/`. craft-archive therefore has two modes, chosen by the caller:

- `NameMode::Strict` (the default for `ZipGuard::new`, `ZipReader::open` and the writer): a `\` anywhere in a name is an `UnsafePath` error. This is the safe default for archives the application writes itself or whose producers are controlled, because a `\` is a path separator on Windows and a traversal vector (`..\..\x`) once a name is turned into a path. Names are exposed unchanged.
- `NameMode::NormalizeBackslash` (opt-in, `ZipReader::open_with_names` / `ZipGuard::new_with_names`): `\` is mapped to `/` first, and then the same checks run on the normalised name (traversal, absolute, drive, control characters, empty segments) plus duplicate detection, so `a\b` and `a/b` in one archive are rejected as duplicates. The caller sees and asks for the normalised names. Use it for user-supplied documents produced by arbitrary software, where refusing to open the file is a worse outcome than reading `xl/styles.xml` from `xl\styles.xml`.

The writer never emits a backslash in either mode. Opening is stricter than the pre-swap readers, which looked entries up by exact name and so accepted `xl\styles.xml` only if asked for with the backslash (cloudbring/artcraft-algorithms#208).

| consumer | reads zip with | current choice | recommendation |
|---|---|---|---|
| craft-ooxml | `ZipGuard::new_with_names` | `NormalizeBackslash` by default (`Tolerance::normalize_backslash_names = true`, lenient reader); `Strict` when that tolerance is switched off | keep |
| craft-doc (`.pcraft`) | `ZipReader::open` | `Strict` | keep: it is the app's own container, and its writer cannot produce a backslash |
| vectorcraft (`engine/src/cmd/fileio/zip.rs`) | `ZipReader::open` | `Strict` | switch to `open_with_names(.., NormalizeBackslash)` where it ingests third-party `.docx`/`.xlsx`; the pre-swap reader opened such files, so Strict is a behaviour change |
| effectcraft (`lottie/src/zip.rs`) | `ZipReader::open` | `Strict` | keep: dotLottie is an asset bundle and the format requires `/` |
| pdfcraft (`export/src/zip.rs`) | none (`ZipWriter` only) | n/a, the writer rejects `\` | none; if a reader is added for Office input, use `NormalizeBackslash` |

Document applications that open files from the wild should normalise, like OOXML; asset bundles and app-owned containers stay strict. This table reflects each app's `algo/120-use-craft-archive-zip` branch as read on 2026-11-04.

## End-record discovery

`ZipReader::open` scans backwards from the end for end-of-central-directory signatures and uses the first whose directory is consistent (comment fits, a central-directory header sits where the size says, counts non-zero, or a ZIP64 locator precedes it), preferring an exact-fit comment and a non-empty directory. A comment that itself contains `PK\x05\x06` therefore no longer hides the real directory (the `zip` crate alone would take the bogus record). `ZipGuard::new` receives an already-built `zip::ZipArchive`, so callers constructing the archive themselves keep the `zip` crate's own discovery.

## Open cost

`ZipReader::open` validates every entry once and allocates nothing per entry (names and metadata for `names()`/`entry()` are built on first use). It still pays for the `zip` crate parsing the central directory, which is about 70% of the remaining time on very large archives (#206).
