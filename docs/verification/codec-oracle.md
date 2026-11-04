# Codec oracle: the #48 de-fork (h264, hevc, av1, vp8, vp9, opus)

Ticket: cloudbring/artcraft-algorithms#187 (V2.4). Evidence standard: `research/VERIFICATION.md` section 1.1
(old and new in one binary, over pinned inputs, every difference identical, within a named tolerance, or registered
with its evidence; anything else is a regression). Harness: `tools/codec-oracle/` (its own `[workspace]`), registry:
`tools/codec-oracle/expected.toml`. Base: craft-libs `algo/integration` b62cb42. Run on 2026-11-04.

## Result in one paragraph

The three apps' decoder forks were compared with the shared `craft-h264/hevc/av1/vp8/vp9/opus` crates on
**143,435 stream comparisons** (12 old decoder trees x ffmpeg fixtures, official conformance vectors, damaged streams,
header sweeps, value-extreme patterns, hand-built streams and avcC/hvcC records; 1,111,173 decoded pictures and
2.5 billion Opus samples on the new side). **125,645 are bit-identical** in outcome (how decoding ended, picture count, picture size/pts/key
flag, every plane byte, every PCM sample bit), **17,790 are registered differences, 0 are unexplained, 0 panics or
timeouts on the new side, 0 stale registry entries.** All 1,251 valid-video comparisons (341 ffmpeg-matrix streams and 910 libvpx/libaom conformance-vector
runs) are **bit-identical**. The differences come from damaged, hostile or hand-built input, from Opus (float rounding in
the MDCT, on the valid vectors too), and from the HEVC memory-budget accounting. No regression was found, so no craft-* code was changed. Three things need a human decision and are
listed under "Findings": the VP9 decoder is now stricter than the forks (from #131, **not** on the #48 list), the HEVC memory budget counts samples
differently from the deckcraft fork (documented in the crate), and frame-threaded H.264/HEVC decoding is not deterministic on damaged
streams in old or new code. This raises C21 to **L3** (corpus differential with zero unexplained differences); it is not L4, because no independent decoder output was compared: ffmpeg is used only as an encoder,
and the libvpx/libaom vectors' published md5 files were not checked here (the crates' own tests do that for VP8).

## What was compared

The forks are **not** byte-identical across apps, so each is tested (hashing the deleted `src/` trees after normalising crate names:
h264 7 of 20 files identical between filmcraft and deckcraft, 3 of 18 between filmcraft and soundcraft; hevc 4 of 17; av1 3 of 21; vp9 4 of 16; opus 2 of 16; the
bitstream crates differ). OLD is each app's crate at the **merge-base of its `algo/48-use-craft-*` branches with `origin/main`**
(a git dependency renamed in `Cargo.toml`, nothing vendored):

| app | OLD commit | crates | NEW branch tip(s) it was swapped to |
|---|---|---|---|
| filmcraft | storytold/filmcraft `eb59ec09daaea519c9c8fffa404a594ae50d00e0` | filmcraft-h264, -hevc, -av1, -vp8, -vp9, -opus (and -bitstream, exercised through them) | `algo/48-use-craft-h264`, `-hevc-av1`, `-vp9-opus` |
| deckcraft | storytold/deckcraft `3e178d8f5a59c787a6a53a7ed0f052b3e4a1489f` | deckcraft-h264, -hevc, -av1, -vp9, -opus (no vp8 fork) | `algo/48-use-craft-{h264,hevc,av1,vp9,opus}` |
| soundcraft | storytold/soundcraft `b35dd0f9eb0c08a198c1306021bd138f3a22d87c` | `soundcraft-video`'s embedded `h264` module | `algo/48-use-craft-h264` |

NEW is the workspace crates (path dependencies). That is 12 old decoder trees (6 + 5 + 1) against one new tree.

A third side, **PRE**, attributes every difference: the shared crates at the commit that moved each fork in *verbatim*, before the upstreamed
hardening commit (craft-libs `935b352` h264, `00fbb68` hevc, `58f8a71` av1, `9aafc3f` vp8/vp9, `2688653` opus). For a difference between
OLD and NEW: `fix` means OLD equals PRE and PRE differs from NEW (the hardening commit changed it); `drift` means OLD differs from PRE and PRE equals NEW (the fork
differed from the code the crate was moved from; the hardening commit did nothing); `both` is both. The attribution is part of the metric the registry keys on.

Build: `--release` with `overflow-checks = true`. The forks were hardened against integer overflow found by debug assertions; the unhardened
decoders only panic where they overflow if the checks are on, so a release build without them would hide the differences the fixes are about. A panic in a decoder
is caught and compared as a result (`panic` ending, with its source file), not allowed to stop the run.

## Streams

`tools/codec-oracle/src/matrix.rs` generates the ffmpeg matrix with `craft-media-testkit` (ffmpeg discovery, skip-or-fail rule, fixture directory,
atomic publish): the encoder tables of `crates/{h264,hevc,vp8,vp9}/tests/it/common/mod.rs` are extracted by `scripts/extract_matrix.py` into `src/matrix_data.rs`
(so the oracle decodes the same libx264/libx265/libvpx/libvpx-vp9 configurations the crates test against); AV1 uses the `oracle_intra`/`oracle_inter` SVT-AV1 specs; Opus uses the crates'
libopus oracle settings on lavfi signals. Nothing is committed: fixtures live in `$CODEC_ORACLE_FIXTURES`.

| set | what | count |
|---|---|---|
| matrix | ffmpeg encodes: h264 36 (x264 + VideoToolbox), hevc 41 (x265 + VideoToolbox), vp8 17, vp9 44, av1 23, opus 20 | 181 streams |
| vectors | libvpx VP8 `vp80-*` 62; libvpx VP9 `vp90/91/92/93-*` 185 (webm remuxed to IVF with `ffmpeg -c copy`); libaom AV1 `av1-1-*` 239; Opus RFC 8251 12 `.bit` x 3 decoder configurations | 486 streams (+36 Opus runs) |
| mutants | 528 per codec: 8 bases (the crate matrices' smaller streams) x 66 deterministic damages (truncations, bit flips, zeroed/0xFF runs, header-field sweeps, cut and duplicate chunks) | 528 x 5 codecs + Opus ogg 528 + 150 x 6 `.bit` |
| sweep | every single-bit flip and 0x00/0xFF substitution over the first 12 bytes of each of the first 14 NAL units / IVF frames of 6 small streams (av1: 10 bytes, 5 units, 4 streams) | 4,337 h264, 9,325 hevc, 2,377 av1, 15,995 vp8, 12,570 vp9 |
| havoc | Exp-Golomb-extreme patterns (31 leading zeros, all ones, 0x7FFFFFFF, 0x80000000, ...) laid over the first RBSP bytes of each NAL (Annex-B re-escaped) or IVF payload | 2,502 h264, 5,508 hevc, 2,280 av1, 11,088 vp8, 7,920 vp9 |
| synth | hand-built streams (builders adapted from `crates/h264/src/synth_tests.rs` and `crates/hevc/src/synth_tests.rs`): h264 870 (slice QP deltas, frame_num gaps, POC types 0/1/2 with extreme offsets, ref-list modification, MMCO operands, skip runs), hevc 144 (PPS init_qp and chroma offsets, slice_qp_delta) | 1,014 |
| avcc / hvcc | a valid record, every bit flip and 0x00/0xFF substitution, every truncation, the type-swap of the crate's regression: 244 avcC, 1,673 hvcC (two records each) | 1,917 |
| budget | `set_memory_budget` at the shared crate's acceptance threshold (binary search) +/- 1..65536 bytes and extremes, 3 streams per codec | 75 per codec (h264, hevc, av1, vp9; deckcraft only: filmcraft and soundcraft never had a budget) |

Not generated: AV1 4:2:2/4:4:4 (the local ffmpeg has no libaom encoder; 3 specs skipped), one Opus 4-channel case (libopus refuses the mapping; 1 skipped), the `bench_*` performance streams.
The libvpx, libaom and Opus sets are downloaded (`docs/verification/codec-oracle-manifest.tsv` lists name, sha256 and URL of all 499); only the Opus tarball has a published digest, and it matches
`crates/opus/test-support/vectors.sha256` (`6b26a22f...`). No digest is published for the VP8/VP9/AV1 files; the manifest records what was decoded.
One libvpx VP9 vector, `vp90-2-13-largescaling`, is rejected identically by the forks and the shared crate ("frame size 19200x108 too large", filmcraft #2930), so it counts as identical.

## How a stream is judged

Each side decodes with the same driver text (instantiated per crate by a macro): Annex-B split into access units (the crates' own splitters), IVF frames, Ogg packets, RFC `.bit` packets. The output
is a trace: how it ended (complete, error class and unit, or panic), and for each picture its size, bit depth, pts, key flag and the SHA-256 of every plane (the bytes are kept up to 96 MiB per
trace, to measure a difference), or the PCM and the final range-coder state after each packet. `craft-oracle`'s `Differential` runs both sides (timeout 120 s each), a custom judge compares
the traces exactly, and `craft-compare` supplies the verdict type. Any difference is `Exceeds` with the metric `<aspects>|<attribution>[|old=<site>|new=<site>]`, with aspects in this order:
`end_<old>_<new>`, `err_at`, `err_msg`, `frames`, `frame_meta`, `plane_bytes`, `pcm_len`, `pcm_bytes`, `range_state`. Nothing is "within tolerance": a difference is registered or it fails.
Valid streams run with the decoders' default thread counts; damaged streams run **single-threaded** (see Findings: threaded decoding of damaged streams is not repeatable).

## Results

| scenario | set | streams | pictures old/new | PCM samples | old ok/err/panic | new ok/err/panic | identical | expected | other |
|---|---|---:|---:|---:|---|---|---:|---:|---:|
| h264-filmcraft | matrix | 36 | 758/758 | 0 | 36/0/0 | 36/0/0 | 36 | 0 | 0 |
| h264-filmcraft | mutants | 528 | 5569/5569 | 0 | 282/246/0 | 282/246/0 | 525 | 3 | 0 |
| h264-filmcraft | sweep | 4337 | 46968/46812 | 0 | 2166/2171/0 | 2156/2181/0 | 4323 | 14 | 0 |
| h264-filmcraft | synth | 870 | 908/3448 | 0 | 302/293/275 | 666/204/0 | 470 | 400 | 0 |
| h264-filmcraft | havoc | 2502 | 17367/17353 | 0 | 683/1819/0 | 682/1820/0 | 2499 | 3 | 0 |
| h264-deckcraft | matrix | 36 | 758/758 | 0 | 36/0/0 | 36/0/0 | 36 | 0 | 0 |
| h264-deckcraft | mutants | 528 | 5569/5569 | 0 | 282/246/0 | 282/246/0 | 528 | 0 | 0 |
| h264-deckcraft | sweep | 4337 | 46968/46812 | 0 | 2166/2171/0 | 2156/2181/0 | 4324 | 13 | 0 |
| h264-deckcraft | synth | 870 | 1562/3448 | 0 | 472/127/271 | 666/204/0 | 514 | 356 | 0 |
| h264-deckcraft | havoc | 2502 | 17393/17353 | 0 | 685/1817/0 | 682/1820/0 | 2494 | 8 | 0 |
| h264-soundcraft | matrix | 36 | 758/758 | 0 | 36/0/0 | 36/0/0 | 36 | 0 | 0 |
| h264-soundcraft | mutants | 528 | 5569/5569 | 0 | 282/246/0 | 282/246/0 | 527 | 1 | 0 |
| h264-soundcraft | sweep | 4337 | 46812/46812 | 0 | 2156/2181/0 | 2156/2181/0 | 4326 | 11 | 0 |
| h264-soundcraft | synth | 870 | 3642/3448 | 0 | 731/139/0 | 666/204/0 | 797 | 73 | 0 |
| h264-soundcraft | havoc | 2502 | 17379/17353 | 0 | 684/1818/0 | 682/1820/0 | 2494 | 8 | 0 |
| h264-avcc-filmcraft | avcc | 244 | 0/0 | 0 | 0/244/0 | 0/244/0 | 129 | 115 | 0 |
| h264-avcc-deckcraft | avcc | 244 | 0/0 | 0 | 0/244/0 | 0/244/0 | 242 | 2 | 0 |
| h264-avcc-soundcraft | avcc | 244 | 0/0 | 0 | 0/244/0 | 0/244/0 | 129 | 115 | 0 |
| hevc-filmcraft | matrix | 41 | 556/556 | 0 | 41/0/0 | 41/0/0 | 41 | 0 | 0 |
| hevc-filmcraft | mutants | 528 | 5206/5206 | 0 | 226/302/0 | 226/302/0 | 528 | 0 | 0 |
| hevc-filmcraft | sweep | 9325 | 82246/82246 | 0 | 4573/4752/0 | 4573/4752/0 | 9279 | 46 | 0 |
| hevc-filmcraft | synth | 144 | 0/0 | 0 | 0/136/8 | 0/144/0 | 115 | 29 | 0 |
| hevc-filmcraft | havoc | 5508 | 34555/34555 | 0 | 1600/3908/0 | 1600/3908/0 | 5481 | 27 | 0 |
| hevc-deckcraft | matrix | 41 | 556/556 | 0 | 41/0/0 | 41/0/0 | 41 | 0 | 0 |
| hevc-deckcraft | mutants | 528 | 5206/5206 | 0 | 226/302/0 | 226/302/0 | 528 | 0 | 0 |
| hevc-deckcraft | sweep | 9325 | 82246/82246 | 0 | 4573/4752/0 | 4573/4752/0 | 9325 | 0 | 0 |
| hevc-deckcraft | synth | 144 | 0/0 | 0 | 0/144/0 | 0/144/0 | 144 | 0 | 0 |
| hevc-deckcraft | havoc | 5508 | 34555/34555 | 0 | 1600/3908/0 | 1600/3908/0 | 5508 | 0 | 0 |
| hevc-hvcc-filmcraft | hvcc | 1673 | 0/0 | 0 | 0/1673/0 | 0/1673/0 | 577 | 1096 | 0 |
| hevc-hvcc-deckcraft | hvcc | 1673 | 0/0 | 0 | 0/1673/0 | 0/1673/0 | 1673 | 0 | 0 |
| h264-budget-deckcraft | budget | 75 | 689/689 | 0 | 39/36/0 | 39/36/0 | 75 | 0 | 0 |
| hevc-budget-deckcraft | budget | 75 | 210/910 | 0 | 9/66/0 | 39/36/0 | 9 | 66 | 0 |
| av1-budget-deckcraft | budget | 75 | 338/338 | 0 | 39/36/0 | 39/36/0 | 75 | 0 | 0 |
| vp9-budget-deckcraft | budget | 75 | 390/390 | 0 | 39/36/0 | 39/36/0 | 75 | 0 | 0 |
| av1-filmcraft | matrix | 23 | 127/127 | 0 | 23/0/0 | 23/0/0 | 23 | 0 | 0 |
| av1-filmcraft | vectors | 239 | 569/569 | 0 | 239/0/0 | 239/0/0 | 239 | 0 | 0 |
| av1-filmcraft | mutants | 528 | 3068/3068 | 0 | 399/129/0 | 399/129/0 | 522 | 6 | 0 |
| av1-filmcraft | sweep | 2377 | 13846/13846 | 0 | 1640/737/0 | 1639/738/0 | 2372 | 5 | 0 |
| av1-filmcraft | havoc | 2280 | 14848/14824 | 0 | 1516/764/0 | 1514/766/0 | 2263 | 17 | 0 |
| av1-deckcraft | matrix | 23 | 127/127 | 0 | 23/0/0 | 23/0/0 | 23 | 0 | 0 |
| av1-deckcraft | vectors | 239 | 569/569 | 0 | 239/0/0 | 239/0/0 | 239 | 0 | 0 |
| av1-deckcraft | mutants | 528 | 3068/3068 | 0 | 399/129/0 | 399/129/0 | 528 | 0 | 0 |
| av1-deckcraft | sweep | 2377 | 13846/13846 | 0 | 1639/738/0 | 1639/738/0 | 2370 | 7 | 0 |
| av1-deckcraft | havoc | 2280 | 14824/14824 | 0 | 1514/766/0 | 1514/766/0 | 2279 | 1 | 0 |
| vp8-filmcraft | matrix | 17 | 217/217 | 0 | 17/0/0 | 17/0/0 | 17 | 0 | 0 |
| vp8-filmcraft | vectors | 62 | 1932/1932 | 0 | 62/0/0 | 62/0/0 | 62 | 0 | 0 |
| vp8-filmcraft | mutants | 528 | 6656/6656 | 0 | 454/74/0 | 454/74/0 | 528 | 0 | 0 |
| vp8-filmcraft | sweep | 15995 | 176836/176836 | 0 | 13862/2133/0 | 13862/2133/0 | 15995 | 0 | 0 |
| vp8-filmcraft | havoc | 11088 | 121701/121701 | 0 | 8546/2542/0 | 8546/2542/0 | 11088 | 0 | 0 |
| vp9-filmcraft | matrix | 44 | 452/452 | 0 | 44/0/0 | 44/0/0 | 44 | 0 | 0 |
| vp9-filmcraft | vectors | 185 | 2218/2218 | 0 | 184/1/0 | 184/1/0 | 185 | 0 | 0 |
| vp9-filmcraft | mutants | 528 | 5348/1840 | 0 | 473/55/0 | 101/427/0 | 112 | 416 | 0 |
| vp9-filmcraft | sweep | 12570 | 100062/86619 | 0 | 10419/2151/0 | 6954/5616/0 | 8385 | 4185 | 0 |
| vp9-filmcraft | havoc | 7920 | 51234/43716 | 0 | 4240/3680/0 | 2311/5609/0 | 5781 | 2139 | 0 |
| vp9-deckcraft | matrix | 44 | 452/452 | 0 | 44/0/0 | 44/0/0 | 44 | 0 | 0 |
| vp9-deckcraft | vectors | 185 | 2218/2218 | 0 | 184/1/0 | 184/1/0 | 185 | 0 | 0 |
| vp9-deckcraft | mutants | 528 | 5348/1840 | 0 | 473/55/0 | 101/427/0 | 112 | 416 | 0 |
| vp9-deckcraft | sweep | 12570 | 100094/86619 | 0 | 10424/2146/0 | 6954/5616/0 | 8380 | 4190 | 0 |
| vp9-deckcraft | havoc | 7920 | 51234/43716 | 0 | 4240/3680/0 | 2311/5609/0 | 5781 | 2139 | 0 |
| opus-ogg-filmcraft | matrix | 20 | 0/0 | 12433680 | 20/0/0 | 20/0/0 | 3 | 17 | 0 |
| opus-ogg-filmcraft | mutants | 528 | 0/0 | 265770120 | 453/75/0 | 453/75/0 | 104 | 424 | 0 |
| opus-ogg-deckcraft | matrix | 20 | 0/0 | 12433680 | 20/0/0 | 20/0/0 | 0 | 20 | 0 |
| opus-ogg-deckcraft | mutants | 528 | 0/0 | 265770120 | 453/75/0 | 453/75/0 | 51 | 477 | 0 |
| opus-bit-stereo-filmcraft | vectors | 12 | 0/0 | 30780960 | 12/0/0 | 12/0/0 | 3 | 9 | 0 |
| opus-bit-stereo-filmcraft | mutants | 150 | 0/0 | 361566240 | 148/2/0 | 148/2/0 | 3 | 147 | 0 |
| opus-bit-mono-filmcraft | vectors | 12 | 0/0 | 15390480 | 12/0/0 | 12/0/0 | 3 | 9 | 0 |
| opus-bit-mono-filmcraft | mutants | 150 | 0/0 | 180783120 | 148/2/0 | 148/2/0 | 3 | 147 | 0 |
| opus-bit-stereo-noinv-filmcraft | vectors | 12 | 0/0 | 30780960 | 12/0/0 | 12/0/0 | 3 | 9 | 0 |
| opus-bit-stereo-noinv-filmcraft | mutants | 150 | 0/0 | 361566240 | 148/2/0 | 148/2/0 | 3 | 147 | 0 |
| opus-bit-stereo-deckcraft | vectors | 12 | 0/0 | 30780960 | 12/0/0 | 12/0/0 | 0 | 12 | 0 |
| opus-bit-stereo-deckcraft | mutants | 150 | 0/0 | 361566240 | 148/2/0 | 148/2/0 | 3 | 147 | 0 |
| opus-bit-mono-deckcraft | vectors | 12 | 0/0 | 15390480 | 12/0/0 | 12/0/0 | 0 | 12 | 0 |
| opus-bit-mono-deckcraft | mutants | 150 | 0/0 | 180783120 | 148/2/0 | 148/2/0 | 3 | 147 | 0 |
| opus-bit-stereo-noinv-deckcraft | vectors | 12 | 0/0 | 30780960 | 12/0/0 | 12/0/0 | 0 | 12 | 0 |
| opus-bit-stereo-noinv-deckcraft | mutants | 150 | 0/0 | 361566240 | 148/2/0 | 148/2/0 | 3 | 147 | 0 |

`expected.toml` has 227 entries, generated by `scripts/gen_expected.py` from the reviewed run; each is keyed by scenario, set and exact metric. The script fails on a group it has no rule for.

## Differences and the fix each maps to

Counts are streams (damaged or constructed, never a valid ffmpeg or conformance stream except Opus), summed over the apps shown.

| registry id (`...:<id>:...`) | n | apps | cause | evidence |
|---|---:|---|---|---|
| h264-qp-poc-checked | 542 | fc 267, dc 267, sc 8 | `slice.rs`: checked QP addition and saturating POC. The forks panic at `slice.rs` (QP `i32::MAX`, POC type 1 offsets, and `core::iter::accum` for the POC cycle sum) where the shared crate returns "slice QP overflow" or decodes. 8 streams where all three forks said "slice QP out of range" and the new crate says "slice QP overflow" differ only in wording | craft-libs `e0f337e`; deckcraft `3e178d8` and soundcraft `b35dd0f` slice.rs; `crates/h264/tests/upstream_regressions.rs` |
| h264-avcc-validation | 232 | fc 115, dc 2, sc 115 | `decoder.rs`: `configure_avcc` rejects wrong parameter-set NAL types, empty and truncated sets, and leaves the NAL length size and parameter stores untouched on failure (the filmcraft and soundcraft code records the NAL length size of a record it then rejects). dc 2: same rejection, other wording ("forbidden_zero_bit") | `e0f337e`; deckcraft decoder.rs; `synth_tests::avcc_validates_types_and_keeps_configuration_on_failure` |
| h264-frame-num-gaps | 109 | fc 109 | `decoder.rs`: large `frame_num` gaps decode (bounded) instead of "frame number gap exceeds the supported reference limit" | `e0f337e`; deckcraft decoder.rs; `large_frame_num_gap_remains_bounded_and_decodable` |
| h264-dpb-bounds | 70 | fc 35, dc 35 | `dpb.rs`: reference-list and MMCO arithmetic bounded. The unhardened code panics at `dpb.rs` (`abs_diff_pic_num`, MMCO operands near `i32::MAX`) or accepts an out-of-range `abs_diff_pic_num_minus1` the new code rejects | `e0f337e`; soundcraft `b35dd0f` dpb.rs; `ref_list_mod_huge_value_does_not_panic` |
| h264-transform-clamp | 9 | fc 9 | `transform.rs`: coefficient and DC-shortcut bounds change the pictures of streams with hostile levels. **File-level bisect** (`tools/codec-oracle/bisect`, scripts/make-h264-variants.sh): reverting only `transform.rs` of the new crate to its pre-fix text restores the filmcraft output on all 4 sweep streams; reverting only `dpb.rs` restores it on all 10 `abs_diff_pic_num` streams; reverting any of the other four changed files does neither | `e0f337e`; deckcraft/soundcraft transform.rs |
| h264-fork-snapshot | 160 | dc 75, sc 85 | **not a hardening fix**: the deckcraft and soundcraft forks were snapshots older than the filmcraft code the crate was moved from. The shared crate (PRE already equals NEW) has filmcraft's later checks: "MMCO long-term index exceeds the configured limit" (130 streams where a fork decodes garbage), PPS field validation order ("pic_init_qp/qs out of range" before the end-of-data error), and concealment of `weighted_bipred_idc` 3 (the one pixel difference, `qp50.h264` byte 35 bit 4, verified by parsing the PPS with both crates: the fork keeps 3, the crate conceals) | filmcraft `ee871ca0` (#2894), `1ef29605` (#1441), `4328e49a` (#2915); deckcraft `docs/h264-fork-divergence.md`; soundcraft `crates/video/MIGRATION-48.md` |
| hevc-hvcc-preflight | 1,096 | fc 1,096 | F1: typed two-pass `hvcC` preflight: mismatched/AUD/VCL arrays, truncation and unsupported array types are rejected up front and a failed record no longer changes the NAL length size or parameter stores (filmcraft has the unhardened code; deckcraft's fork, which had F1, is identical on all 1,673) | craft-libs `0d907d5` README F1 |
| hevc-pps-qp / hevc-slice-qp | 25 / 4 | fc | F3: PPS `init_qp_minus26` range (the filmcraft code accepts -75 and panics at `26 + read_se()`); F4: slice QP at header parsing (panics at `slice.rs` on `i32::MAX`) | `0d907d5` F3, F4 |
| hevc-weight-denominator | 73 | fc 73 | F5: chroma weight denominator: same error class and unit, new wording `delta_chroma_log2_weight_denom` | `0d907d5` F5 |
| hevc-budget-accounting | 66 | dc 66 | F2 memory budget: the shared crate charges one byte per sample for streams of at most 8 bits where the fork charged `u16`, so the accept threshold is about half (e.g. `bframes.hevc` 1,520,640 vs more than 1,586,176 bytes) and the wording differs. Documented in the crate README; h264, av1 and vp9 thresholds are **exactly** equal to deckcraft's (75 of 75 each) | `0d907d5` README ("Unlike the fork's all-u16 storage") |
| av1-tile-group-order | 28 | fc 28 | `decoder.rs`: tile groups must be contiguous and in order: "tile group out of order" where the filmcraft code says "missing tile coverage" or "truncated" | craft-libs `2f6b4f1`; deckcraft av1 decoder.rs |
| av1-fork-snapshot | 8 | dc 8 | **not a hardening fix**: the deckcraft av1 fork predates filmcraft's hostile-input hardening (a block-size/subsampling panic becomes "frame decode panicked" in the fork; the crate returns "block size has no chroma size") | filmcraft `cb2ac66c`, `62c16fcb`; deckcraft `docs/av1-fork-divergence.md` |
| vp9-strict-bool-decoder | 13,480 | fc 6,740, dc 6,740 | VP9 boolean partitions are validated (marker, exhaustion, nonzero padding) and the error propagates; the forks decode corrupt partitions with implicit zero extension. **Not on the #48 list** | filmcraft `409f5418` (#131), carried into craft-libs `9aafc3f`; see Findings |
| vp9-fork-snapshot | 5 | dc 5 | the deckcraft vp9 fork accepts a frame with no tile data that filmcraft rejects | filmcraft `99ec34de` (#2930) |
| opus-celt-mdct | 1,883 | fc 909, dc 974 | float-level PCM differences from the MDCT now computed by craft-fft; 36 of 36 RFC 8251 vectors keep identical range states, quality >= 94.1 (threshold 0), largest sample difference 2.4e-7 | craft-libs `2688653` (#49) |

(vp8: 0 differences in 27,690 comparisons, including all 62 libvpx vectors; the shared crate is the filmcraft fork plus the unified bool decoder, which VP8 keeps lenient.)

### Opus against the RFC 8251 reference

`codec-oracle opus-quality` scores the old and new PCM against the official `.dec` files with the crate's opus_compare-style metric (pass: >= 0); range states equal for all 36:

| vector | config | quality fc | quality dc | quality new | SNR new vs ref (dB) | max abs fc-new | max abs dc-new | range states equal (fc/dc vs new) |
|---|---|---:|---:|---:|---:|---:|---:|---|
| testvector01 | stereo | 99.9 | 99.9 | 99.9 | 79.8 | 1.79e-7 | 1.79e-7 | true/true |
| testvector01 | stereo-noinv | 99.9 | 99.9 | 99.9 | 79.8 | 1.79e-7 | 1.79e-7 | true/true |
| testvector01 | mono | 100.0 | 100.0 | 100.0 | 81.5 | 1.19e-7 | 1.19e-7 | true/true |
| testvector02 | stereo | 94.1 | 94.1 | 94.1 | 15.6 | 0.00e0 | 2.98e-8 | true/true |
| testvector02 | stereo-noinv | 94.1 | 94.1 | 94.1 | 15.6 | 0.00e0 | 2.98e-8 | true/true |
| testvector02 | mono | 94.2 | 94.2 | 94.2 | 15.9 | 0.00e0 | 2.98e-8 | true/true |
| testvector03 | stereo | 99.3 | 99.3 | 99.3 | 19.1 | 0.00e0 | 4.47e-8 | true/true |
| testvector03 | stereo-noinv | 99.3 | 99.3 | 99.3 | 19.1 | 0.00e0 | 4.47e-8 | true/true |
| testvector03 | mono | 99.5 | 99.5 | 99.5 | 20.7 | 0.00e0 | 4.47e-8 | true/true |
| testvector04 | stereo | 99.0 | 99.0 | 99.0 | 18.5 | 0.00e0 | 4.47e-8 | true/true |
| testvector04 | stereo-noinv | 99.0 | 99.0 | 99.0 | 18.5 | 0.00e0 | 4.47e-8 | true/true |
| testvector04 | mono | 99.1 | 99.1 | 99.1 | 20.4 | 0.00e0 | 4.47e-8 | true/true |
| testvector05 | stereo | 97.3 | 97.3 | 97.3 | 20.5 | 5.22e-8 | 5.96e-8 | true/true |
| testvector05 | stereo-noinv | 97.3 | 97.3 | 97.3 | 20.5 | 5.22e-8 | 5.96e-8 | true/true |
| testvector05 | mono | 97.7 | 97.7 | 97.7 | 23.4 | 3.91e-8 | 5.96e-8 | true/true |
| testvector06 | stereo | 97.2 | 97.2 | 97.2 | 21.2 | 4.84e-8 | 5.96e-8 | true/true |
| testvector06 | stereo-noinv | 97.1 | 97.1 | 97.1 | 21.2 | 4.10e-8 | 5.96e-8 | true/true |
| testvector06 | mono | 98.1 | 98.1 | 98.1 | 23.5 | 3.73e-8 | 5.96e-8 | true/true |
| testvector07 | stereo | 99.9 | 99.9 | 99.9 | 66.3 | 6.71e-8 | 6.71e-8 | true/true |
| testvector07 | stereo-noinv | 99.9 | 99.9 | 99.9 | 66.3 | 5.96e-8 | 7.45e-8 | true/true |
| testvector07 | mono | 100.0 | 100.0 | 100.0 | 67.4 | 5.96e-8 | 4.84e-8 | true/true |
| testvector08 | stereo | 100.0 | 100.0 | 100.0 | 65.8 | 8.94e-8 | 6.71e-8 | true/true |
| testvector08 | stereo-noinv | 100.0 | 100.0 | 100.0 | 65.9 | 8.94e-8 | 6.71e-8 | true/true |
| testvector08 | mono | 100.0 | 100.0 | 100.0 | 67.7 | 5.96e-8 | 5.96e-8 | true/true |
| testvector09 | stereo | 99.9 | 99.9 | 99.9 | 65.6 | 9.31e-8 | 8.94e-8 | true/true |
| testvector09 | stereo-noinv | 99.9 | 99.9 | 99.9 | 65.6 | 9.31e-8 | 8.94e-8 | true/true |
| testvector09 | mono | 99.9 | 99.9 | 99.9 | 66.5 | 7.45e-8 | 6.71e-8 | true/true |
| testvector10 | stereo | 99.7 | 99.7 | 99.7 | 53.4 | 5.96e-8 | 8.94e-8 | true/true |
| testvector10 | stereo-noinv | 99.7 | 99.7 | 99.7 | 53.4 | 5.96e-8 | 8.94e-8 | true/true |
| testvector10 | mono | 99.7 | 99.7 | 99.7 | 53.6 | 5.96e-8 | 5.96e-8 | true/true |
| testvector11 | stereo | 100.0 | 100.0 | 100.0 | 82.7 | 2.38e-7 | 1.49e-7 | true/true |
| testvector11 | stereo-noinv | 100.0 | 100.0 | 100.0 | 82.7 | 2.38e-7 | 1.49e-7 | true/true |
| testvector11 | mono | 100.0 | 100.0 | 100.0 | 84.6 | 1.19e-7 | 1.19e-7 | true/true |
| testvector12 | stereo | 96.5 | 96.5 | 96.5 | 19.6 | 2.24e-8 | 2.98e-8 | true/true |
| testvector12 | stereo-noinv | 96.5 | 96.5 | 96.5 | 19.6 | 2.24e-8 | 2.98e-8 | true/true |
| testvector12 | mono | 96.5 | 96.5 | 96.5 | 19.6 | 2.24e-8 | 2.98e-8 | true/true |

36 comparisons; lowest new-decoder quality 94.1 (pass criterion: >= 0).

## Which listed #48 fixes the differential exercised

| fix (commit) | exercised by | result |
|---|---|---|
| h264 avcC validation (`e0f337e`) | avcC set | yes, 232 differences |
| h264 whole-store memory budget | budget set vs deckcraft | thresholds identical to the byte (75 of 75); filmcraft/soundcraft had no budget |
| h264 large frame-number gaps | synth | yes, 109 |
| h264 checked QP | synth | yes |
| h264 temporal direct MV arithmetic | mutants, sweep, havoc, synth | **no difference observed** (no stream reached it; covered only by the crate's red-green tests) |
| h264 bounded coefficients | sweep, havoc, mutants | yes, 9 pictures differ, bisected to transform.rs |
| h264 reference-list and MMCO bounds | synth, sweep, havoc | yes, 70 |
| h264 missing-row concealment | all | **no difference observed** |
| h264 saturating POC | synth | yes |
| h264 CAVLC skip-run bounds | synth skip runs | **no difference observed** (all forks already handle the grid) |
| hevc F1 hvcC, F3 PPS QP, F4 slice QP, F5 weights | hvcC, synth, sweep, havoc | yes (1,096, 25, 4, 73) |
| hevc F2 memory budget | budget set | exercised; documented accounting difference |
| av1 tile-group ordering | mutants, sweep, havoc | yes, 28 |
| av1 whole-store budget | budget set | thresholds identical to the byte |
| av1 output cap (33rd shown frame), explicit tile-count bounds | all | **no difference observed** |
| vp9 memory budget | budget set | thresholds identical to the byte |

The four "no difference observed" fixes are not shown to be wrong; this oracle simply did not produce a stream that reaches them.

## Findings

1. **The VP9 decoder is stricter than both forks, and #48 does not list it.** 13,480 damaged streams that the forks decode to garbage pictures now end with a `Boolean` error ("boolean partition exhausted", "nonzero VP9 boolean padding"). It comes from filmcraft commit
   `409f5418` (#131: "propagate strict VP9 partition errors"), which `craft-bitstream` carries; the filmcraft and deckcraft swaps inherit it. Every valid stream is unaffected (44 ffmpeg/libvpx encodes and 185 libvpx conformance vectors are identical; the libvpx encoder writes zero padding). Streams from an encoder that leaves nonzero padding would now fail where the forks decoded them. Decision for the owner: accept (the VP9 spec requires zero padding) and add it to #48's list, or relax the padding check.
2. **Frame-threaded decoding of damaged H.264/HEVC is not repeatable, in old and new code alike.** Decoding the same 528 damaged streams twice with default threads gives a different result on a second run for h264 58 (new) / 50 (filmcraft), hevc 36 / 30, av1 1 / 0, vp9 0 / 0 (`examples/determinism.rs`). Differences between a threaded old and new decoder are therefore not evidence of change; with the default thread count 74 + 64 + 242 h264 mutants differed in the first run and 3 + 0 + 1 single-threaded. All hostile sets here decode single-threaded. Valid streams are identical threaded and single-threaded (matrix, vectors).
3. **Soundcraft's and deckcraft's forks were older snapshots of filmcraft code** than the audit implies: 160 h264 differences and 13 more (av1, vp9) are checks filmcraft had and the forks lacked, so the swap *adds* protection for those two apps beyond the 20 listed fixes. None of these is a loss of behaviour.
4. **Opus is not bit-identical** to the forks (the private MDCT was replaced by craft-fft, #49): at most 2.4e-7 per sample (about 0.008 of one 16-bit LSB), identical final range states, quality unchanged to the printed precision.
5. No regression was found; no craft-* code was changed by this ticket.

## Reproduce

```text
export CARGO_TARGET_DIR=$PWD/target-o CODEC_ORACLE_FIXTURES=<scratch dir> CODEC_ORACLE_OUT=<report dir>
cd tools/codec-oracle
cargo build --release
target-o/release/codec-oracle gen            # ffmpeg matrix (needs ffmpeg with libx264, libx265, libvpx, libvpx-vp9, libsvtav1, libopus)
# download the vectors listed in docs/verification/codec-oracle-manifest.tsv into $CODEC_ORACLE_FIXTURES/{vp8/conformance,vp9/conformance,av1/aom-test-data,opus/vectors}
codec-oracle run                              # all scenarios; CODEC_ORACLE_ONLY=h264-filmcraft/sweep,hevc narrows; exit 1 on any unexplained difference
codec-oracle opus-quality
cargo test --release                          # harness self-checks (a planted POC overflow, identical traces, PCM metric)
scripts/gen_expected.py $CODEC_ORACLE_OUT/details.tsv > expected.toml   # only after reviewing every group it reports
```

A full run takes about 40 minutes on an idle Apple-silicon laptop. `details.tsv` in the report directory has one line per non-identical stream (outcome, first differing picture and sample counts); no per-stream artefact is written.

## Limits

- Valid-stream coverage is what ffmpeg encoders and the three conformance sets produce at small sizes (the 1080p/4K performance streams are skipped); H.264 and HEVC have no external conformance vectors in this run.
- AV1 4:2:2/4:4:4 streams could not be generated (no libaom encoder locally); the conformance vectors do not include them either.
- The unexercised fixes above, the HEVC budget accounting and finer attribution than "which source file" for the h264 panics rest on the crates' own red-green tests and on the commit messages, not on this differential.
- The oracle compares behaviour, not safety: it cannot show that the shared crates are free of panics on input none of the 143,435 streams covered.
