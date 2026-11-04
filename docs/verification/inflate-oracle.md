# Inflate oracle: craft-archive against lean-zip's verified decoder

Ticket: cloudbring/artcraft-algorithms#213 (V4.6). Evidence standard: `research/VERIFICATION.md` section 1.1.
Harness: `tools/inflate-oracle/` (its own `[workspace]`), registry: `tools/inflate-oracle/expected.toml`,
Lean wrapper source: `tools/inflate-oracle/lean/`. Run on 2026-11-04 (EST).

## Result in one paragraph

All **92,551** deflate-compressed zip entries (no sampling) of the three Office corpora, plus **3,116** generated
hostile streams (116 hand-built cases and 3,000 seeded mutations), give the same answer in craft-archive's
`inflate_bounded` (raw DEFLATE) and in lean-zip's verified pure-Lean decoder: **0 regressions, 0 panics,
0 timeouts, 0 Lean faults, 0 stale registry entries.** On the corpus, 92,521 streams decode to byte-identical output and
30 are rejected by both (LibreOffice/OSS-Fuzz `fail/` files, truncated archives, 5 empty directory
entries). On hostile inputs 1,420 are byte-identical, 1,653 are rejected by both, and **43 are one registered
difference**: an incomplete literal/length Huffman code, which zlib-family decoders (craft-archive is flate2 on
miniz_oxide) reject at table build and lean-zip accepts. No fix to craft-archive was needed, so there is no
red-then-green test. The raised level for craft-archive inflate is **L4** on the corpus and the hostile classes below,
with the caveats in "What the Lean proof does and does not say".

## Pin and build

| item | value |
|---|---|
| lean-zip | `https://github.com/kim-em/lean-zip` commit `2f7a63f38195bc667a926881b55d10c9f8a88eeb` (2026-09-23) |
| its dependency | `lean-zip-common` `a9ffa91f53ff542c3ce96619d7b455d676e8ebc9` (from its `lake-manifest.json`) |
| toolchain | `leanprover/lean4:v4.35.0-rc2` via elan (`~/.elan/bin`) |
| lean-archive | not used: the ticket's comparison is inflate; zip framing is read by the Rust `zip` crate |
| decoder under test | `Zip.Native.Inflate.inflate`, "the production decoder", pure Lean, no FFI zlib (nothing in lean-zip's default package links zlib; `useNative` does not exist in this revision, the zlib FFI lives only in lean-zlib and the dev-only `conformance/` sub-package, neither of which is built) |

```text
SCRATCH=<scratch dir>
git clone https://github.com/kim-em/lean-zip $SCRATCH/lean-zip && git -C $SCRATCH/lean-zip checkout 2f7a63f38195bc667a926881b55d10c9f8a88eeb
cp -R tools/inflate-oracle/lean $SCRATCH/lzexe          # Main.lean, Axioms.lean, lakefile.lean (path-requires ../lean-zip)
export PATH=$HOME/.elan/bin:$PATH
(cd $SCRATCH/lzexe && lake build lzinflate)               # builds lean-zip including every Zip/Spec proof
(cd $SCRATCH/lzexe && lake env lean Axioms.lean)          # kernel axioms of the two round-trip theorems
export LEAN_ZIP_INFLATE=$SCRATCH/lzexe/.lake/build/bin/lzinflate
CARGO_TARGET_DIR=<worktree>/target-o ARTCRAFT_CORPUS=/Users/emm/ArtCraft/corpus \
  cargo test --release --manifest-path tools/inflate-oracle/Cargo.toml -- --ignored --nocapture
```

`lzinflate [file]` reads raw DEFLATE from the file or stdin, writes the decompressed bytes to stdout and exits 0, or
exits 2 with `reject: <reason>` on stderr. The Rust `LeanZip` side treats any other exit code, a signal, or a
60-second timeout as a `LEAN-FAULT` and the tests assert there are none.

Cost of the build (Apple M-series, macOS 27.0.1): toolchain download 571 MB (2.8 GB installed in `~/.elan`), `lake build`
218 jobs in 17 min 22 s wall (8 min 8 s user), `lean-zip` checkout plus `.lake` 382 MB, `lzexe` 8.1 MB, executable 3.6 MB.
The Lean build tree stays in the scratch directory, not in the repository.

## What the Lean proof does and does not say

`#print axioms` (`tools/inflate-oracle/lean/Axioms.lean`, no `sorryAx` in either):

| theorem | axioms |
|---|---|
| `Zip.Native.Deflate.inflate_deflateRaw` (`inflate (deflateRaw data level) = .ok data`, stated for the production decoder `Inflate.inflate`) | `propext`, `Classical.choice`, `Quot.sound`, plus 96 `*._native.bv_decide.ax_*` axioms |
| `Zip.Native.zlib_decompressSingle_compress` | the same three, plus 97 `bv_decide` axioms |

The `bv_decide` axioms are the SAT-certificate axioms `bv_decide` introduces for bit-vector lemmas (word-sized load/store
recombination, code packing); they rest on the SAT solver's LRAT proof, checked by Lean's native checker, not on the
kernel alone. `grep -w sorry` finds only a docstring saying a file is sorry-free. `Zip/Spec` is 51,722 lines with 1,631
`theorem`/`lemma` declarations in this revision. The trusted base beyond that is the Lean runtime plus four small C
primitives (`c/*.c`: word reads, in-place copies) that have pure-Lean reference bodies the proofs are about.

Scope of the statement: it is a *round trip* (the decoder inverts the encoder) and an accept-set equality with the
reference decoder `inflateReference`, not a theorem that the decoder accepts exactly the RFC 1951 language, and the
specification calls any prefix-free Huffman code valid (see the registered difference). The oracle therefore uses the
decoder as an executable reference for valid streams and as a second opinion on rejection, not as a rejection spec.

## Inputs

* **Corpus**: `office-poi`, `office-openxml-sdk`, `office-libreoffice` under `/Users/emm/ArtCraft/corpus`: 6,106 files,
  fingerprint `sha256(sorted "path size" lines) = ee008743b369139010e69f3e12530b3676a0685a46cf1986cd589864cf25dc58`.
  4,712 files start with `PK\x03\x04` (4 of them the `zip` crate cannot open); they hold 92,551 deflate entries
  (107,418,017 compressed bytes) and 4,255 stored entries; no other method occurs. Raw payloads come from
  `ZipArchive::by_index_raw` (the central directory locates the data, so entries whose local header defers sizes to a
  data descriptor are included). **The ticket and `container-oracle.md` say 4,446 zip packages out of 5,831 files; today
  the same directories hold 6,106 files and 4,712 packages** (`office-poi` and `office-openxml-sdk` were modified at
  16:05-16:07 EST today, after that run), so counts here are not comparable with #186's. The fingerprint above pins this run.
* **Hostile** (`src/hostile.rs`, generated in code, nothing stored): 116 cases: valid controls (levels 0/1/6/9,
  empty fixed block, length 258 at distance 1, stored block, trailing bytes after the final block, empty input,
  one zero byte); truncations (every cut of the first 64 bytes, 15 fractional cuts, minus one byte, inside a stored header and payload);
  block type 11 (final, non-final, after a valid block); stored LEN/NLEN mismatch (complement wrong, zero, LEN beyond input, LEN 0xFFFF with no payload);
  oversubscribed codes (literal/length, with distance, the code-length code itself); incomplete codes (two codes, EOB only, no codes);
  dynamic block with no end-of-block code; distance before the start of output (empty window, five after one literal, 32768 after ten literals) and reserved distance symbols 30 and 31;
  length symbols 286 and 287, symbol 284 with all extra bits set (258), stray extra bits after 285; code-length repeat 16 with no previous length.
  Plus 3,000 seeded (xorshift, seed `0xD3F1A7`) single-bit, single-byte, header-region and double-bit mutations of six valid streams (800-4,300 bytes of text/XML-like
  data at levels 1/2/3/6/9).
  Streams are never written to disk; `cargo run --example dump -- hostile/<case>` reproduces one.

## Sides and judge

`craft_oracle::Differential` with `BytesJudge` (exact bytes). Old = lean-zip subprocess, new = craft-archive
`inflate_bounded(Wrapper::Raw, 1 GiB output, no ratio cap)`. On errors both must reject; the error class may differ
(craft-archive reports `truncated`/`corrupt`, lean-zip names the exact fault). Anything but identical, both-reject or
registered fails the run. Two extra sides run craft-archive against `flate2::read::DeflateDecoder` (used directly) and
`miniz_oxide::inflate::decompress_to_vec_with_limit`; craft-archive *is* flate2 on the miniz_oxide backend, so those
two only confirm the craft-archive wrapper (limits, accounting, input chaining) adds nothing: **0 differences** on the corpus
and on the hostile set (`rust_sides_agree_on_hostile` runs without Lean in the default `cargo test`).

## Results

| comparison | inputs | identical | both reject | registered | other |
|---|---|---|---|---|---|
| lean-zip vs craft-archive, corpus | 92,551 | 92,521 | 30 | 0 | 0 |
| lean-zip vs craft-archive, hostile | 3,116 | 1,420 | 1,653 | 43 | 0 |
| craft-archive vs flate2 direct, corpus | 92,551 | 92,521 | 30 | 0 | 0 |
| craft-archive vs miniz_oxide direct, corpus | 92,551 | 92,521 | 30 | 0 | 0 |
| craft-archive vs flate2 / miniz_oxide, hostile | 3,116 each | 1,420 | 1,696 | 0 | 0 |

(The Rust-only hostile rows have no registered class because craft-archive and the Rust sides share the zlib rule.)

Corpus both-reject (30): 5 zero-length payloads on directory entries (`tdf124525.xlsx!_rels` and four more; both
report end of input), 1 truncated `thumbnail.wmf`, and 24 entries of OSS-Fuzz `fail/` files (`ofz*`, `LIBREOFFICE-DBDAZAEC.docx`,
`forcepoint107.xlsx`) with distance-too-far, oversubscribed code, reserved block type or truncation. Both decoders reject all
of them. Hostile both-reject (1,653): every truncation, block type 11, stored mismatch, oversubscribed, no-codes,
no-EOB, distance-too-far, reserved symbol, and repeat-without-previous case among the 116 (104 cases), and 1,549 of the 3,000 mutations.
Accepted by both with identical bytes among the 116: every valid control, trailing bytes after the final block (ignored by both),
the one-bit-code EOB-only and two-code incomplete blocks, symbol 284 with extra bits 31 (258, the zlib quirk, accepted by lean-zip too),
and stray bits after 285.

## Registered difference (`expected.toml`: `hostile-incomplete-litlen`, 43 inputs)

Root cause. In a dynamic block whose literal/length code lengths leave the Kraft sum below 1 with a longest code over
one bit, zlib's `inflate_table` and miniz_oxide fail with "invalid literal/lengths set" at table construction. lean-zip's
specification (`Zip/Spec/Huffman`) defines a valid code as prefix-free (Kraft sum at most 1), so it builds the table and
fails with `invalid Huffman code` only if decoding reaches an unassigned code; the 43 mutations never do. An independent header parser in the harness
(`hostile::first_block_incomplete_litlen`) classifies the inputs *before* the run, so the glob `mutation/incomplete-litlen/**`
covers exactly that class; the same 43 are what Python's zlib reports as `invalid literal/lengths set` (40 checked one by one before the classifier existed, all 40 the same error).
craft-archive is the stricter side, in agreement with the zlib every producer is tested against. No conforming compressor emits such a code
(the corpus, 92,521 streams, never shows it). Not a defect; if a caller ever needs the lenient reading it must not use flate2.

## Lean decoder throughput

`cargo test --release lean_throughput -- --ignored --nocapture`, 25 largest corpus streams (13.5 MB compressed, 77.0 MB out), median
of 3 runs each, one process per stream. Process startup (empty stream) is 4.0 ms; the Lean figure subtracts it and includes
piping the output to the harness.

| decoder | MB/s of output |
|---|---|
| lean-zip (verified, subprocess, startup removed) | 677 |
| craft-archive (flate2, miniz_oxide) in-process | 1,369 |

On text-like XML (highest ratio) lean-zip reaches 0.7-1.4 GB/s, on already-compressed media 0.15-0.3 GB/s (the figure is dominated by
stdout transfer there). The full 92,551-stream corpus pass takes 70.3 s wall on 8 worker threads (subprocess per stream, about
4 ms each), so no sampling was needed; the Rust-only passes over the same streams take 3.1 s.

## What this does and does not establish

* craft-archive inflate agrees with an independently implemented, kernel-checked decoder on every corpus stream byte for byte, and on rejection for every
  hostile class listed. For *valid* streams the Lean decoder's output is that of a decoder proved to invert a proved encoder.
* It is a differential, not a proof about craft-archive: streams outside the corpus and these classes are not covered, and zip
  framing (central directory, sizes, CRC, names) is not part of this oracle (see `container-oracle.md`).
* lean-archive was not needed and was not built.
