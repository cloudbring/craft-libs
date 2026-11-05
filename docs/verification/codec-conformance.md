# Codec conformance streams in CI (H.264, HEVC, VP8, VP9, AV1)

Ticket: cloudbring/artcraft-algorithms#195 (V4.2). Builds on the #187 codec oracle (`codec-oracle.md`), which compared the
forks with the shared crates but never checked a decoder against published reference output ("L3, not L4"). This
does: every stream below is decoded by the shared crate and compared with reference MD5s. Base: craft-libs `algo/integration`
8fa1b4c, run on 2026-11-04.

## Result

Streams are `pass`, `fail`, or `unsupported` (the decoder's own `Error::Unsupported`, or a stream the reference decoder
rejects too). Per-stream lines: `codec-conformance-results.tsv`.

| Codec | Suite (manifest) | Streams | Pass | Fail | Unsupported | Reference |
| --- | --- | ---: | ---: | ---: | ---: | --- |
| H.264 | JVT H.264.1 (FATE mirror) | 195 | 99 | 0 | 96 | ffmpeg decode, digest per stream |
| HEVC | JCT-VC H.265.1 (FATE mirror) | 184 | 170 | 0 | 14 | ffmpeg decode, digest per stream (equal to the official md5 where checked) |
| VP8 | libvpx `vp80-*` | 62 | 62 | 0 | 0 | published per-frame `.md5` |
| VP9 | libvpx `vp90/91/92/93-2-*` | 343 | 340 | 3 | 0 | published per-frame `.md5` |
| AV1 | libaom `av1-1-*` | 244 | 244 | 0 | 0 | published per-frame `.md5` |
| | | 1028 | 915 | 3 | 110 | |

The 3 VP9 failures are registered in the test (`KNOWN_FAILURES`, with reasons) and are findings below; a suite fails on a new failure,
on a registered one that starts passing, and below its `MIN_PASS` floor (H.264 99, HEVC 170, VP8 62, VP9 340, AV1 244).

### By profile

| Codec | Group | Pass | Fail | Unsupported |
| --- | --- | ---: | ---: | ---: |
| H.264 | Baseline / Main / Extended (AVCv1 set, 137 streams) | 78 | 0 | 59 |
| H.264 | FRExt (High, Hi10, Hi422, 58 streams) | 21 | 0 | 37 |
| HEVC | Main (v1, 163 streams) | 160 | 0 | 3 |
| HEVC | Main 10 (11 streams) | 10 | 0 | 1 |
| HEVC | RExt (10 streams) | 0 | 0 | 10 |
| VP8 | all | 62 | 0 | 0 |
| VP9 | profile 0 (`vp90`, 331) | 329 | 2 | 0 |
| VP9 | profile 1 (`vp91`, 4) | 3 | 1 | 0 |
| VP9 | profile 2, 3 (`vp92` 2, `vp93` 6; 10 and 12 bit) | 8 | 0 | 0 |
| AV1 | 8-bit (`b8`) | 178 | 0 | 0 |
| AV1 | 10-bit (`b10`) | 66 | 0 | 0 |

### Unsupported, and why (every one is a README-listed gap or a stream no reference decoder handles)

- **H.264 (96).** Interlaced coding (PAFF, MBAFF, field pictures; `frame_mbs_only_flag = 0`) 78; 4:2:2 7 (`Hi422*`, `PPH422*`);
  10-bit 3 (`PPH10I1-3`); monochrome 2 (`HPCAMOLQ_BRCM_B`, `HPCVMOLQ_BRCM_B`); FMO slice groups 4 (`FM1_BT_B`,
  `FM1_FT_E`, `FM2_SVA_B`, `FM2_SVA_C`); SP/SI slices 2 (`sp1_bt_a`, `sp2_bt_b`). Interlaced content is the largest gap: 78 of 195 streams, 40% of the suite.
  FM1_BT_B, FM2_SVA_B and FM2_SVA_C are also rejected by ffmpeg (no digest), so they could not be checked either way.
- **HEVC (14).** RExt 4:2:2 / 4:4:4 (chroma_format_idc 2 or 3) 9, RExt tools (`WPP_HIGH_TP_444_8BIT_RExt_Apple_2`) 1, SPS multilayer extensions
  (`PS_A_VIDYO_3`, `PS_B_VIDYO_3`) 2, and two streams ffmpeg rejects, so there is nothing to compare with: `TSUNEQBD_A_MAIN10_Technicolor_2` (unequal luma/chroma
  bit depth) and `EXT_A_ericsson_3` (a superseded draft of the extension-flags stream; `EXT_A_ericsson_4` passes).
- **VP9 / AV1 / VP8.** None. Argon (AV1) is not included, see below.
- **Not fetched at all:** MV-HEVC / SHVC / SCC / 3D-HEVC (layers above 0 are not decoded), H.264 MVC / SVC / professional-profile streams
  (outside the supported profiles; the 4:4:4 Predictive and CAVLC 4:4:4 Intra sets would all be `unsupported`).

## Fixes (HEVC, red then green)

The first HEVC run had 4 failures in the supported profile. Before (`cargo test --release -p craft-conformance --test hevc`, `fail=4`):

```text
BUMPING_A_ericsson_1.bit        fail  66 frames, reference 49
NoOutPrior_A_Qualcomm_1.bit     fail  43 frames, reference 40
RAP_B_Bossen_1.bit              fail  83 frames, reference 80
EXT_A_ericsson_3.bit            fail  access unit 0: bitstream error   (ffmpeg rejects it too: superseded draft stream)
```

The reference digests are the ffmpeg decode; for the three output-order streams the official md5 from the JCT-VC zip
(`.../draft_conformance/HEVC_v1/<name>.zip`, `.md5`) was compared and **equals** ffmpeg's (`64f213eb...`, `1c5f20a0...`, `f38befcc...`), so the decoder, not the
reference, was wrong. Root cause, `crates/hevc/src/decoder.rs` (C.5.2.2, output and removal of pictures at an IRAP with NoRaslOutputFlag = 1):

1. `NoOutputOfPriorPicsFlag` was honoured only for IDR pictures. It is `no_output_of_prior_pics_flag` for IDR **and BLA** pictures and is **inferred to be 1 for a CRA
   picture** with NoRaslOutputFlag = 1 (a CRA after an end-of-sequence NAL unit). The condition is now `sh.no_output_of_prior_pics || hdr.is_cra()`
   (BUMPING_A: 17 pictures the specification discards were output; NoOutPrior_A: 3).
2. A change of active SPS flushed (output) the whole DPB before that rule ran, so a new SPS at such an IRAP defeated it (RAP_B_Bossen switches
   VPS/SPS/PPS at the CRA after EOS: 3 pictures). The SPS-change flush is skipped for an IRAP that resets the DPB, whose own handling flushes or discards.

After: `pass=170 fail=0 unsupported=14`; BUMPING_A, NoOutPrior_A and RAP_B_Bossen pass; `cargo test --release -p craft-hevc` is unchanged (60 + 46 passed, 4 ignored).
The regression test is the conformance suite itself (the three streams, with the registry rule that a registered failure that passes must be removed).

The visible effect: before, playback after an end-of-sequence or an IRAP with `no_output_of_prior_pics_flag` showed pictures a conforming decoder never shows.

## Findings (not fixed: policy, or no reference)

1. **VP9 is stricter than libvpx, by design (#131), and rejects two libvpx vectors.**
   - `vp90-2-13-largescaling.webm` (frame width 19200): `crates/vp9/src/decoder.rs:285` caps each dimension at 16384 (filmcraft #2930, a hostile-input
     guard); VP9 allows 65536. libvpx decodes it. Decision needed: raise the cap for streams whose decoded area is small (19200x108 is 2 Mpixel), or keep it.
   - `vp90-2-15-fuzz-flicker.webm`: "boolean partition exhausted" at packet 4. libvpx zero-fills a tile partition that ends early; `exit_bool` here rejects it.
     With the padding/overread check disabled (experiment, not committed) the stream **matches the libvpx md5**, so the decode is right and only the strictness differs.
   - A fuzz-derived vector failing under a strict decoder is expected; whether playback of such files should fail is a product decision (a lenient mode would change nothing for valid streams).
2. **`vp91-2-04-yv444.webm`** (profile 1, 4:4:4, 1280x720, 5 frames): craft rejects it ("nonzero VP9 boolean padding", packet 0). With the check disabled the frames do not
   match its `.md5` either. ffmpeg's native decoder and its libvpx wrapper both fail on this file too (4 of 5 frames "invalid data"; "Invalid frame marker"), and the `.md5` lines are named
   `d.1280x720_NNNNN.yv12`, so the md5 is not for a plain 4:4:4 decode. Most likely an invalid or legacy vector; no verdict on the decoder from it. The other profile 1 vectors
   (`yuv422`, `yuv440`, `yuv444`) pass.
3. **H.264 has no interlaced support** (78 of 195 JVT streams, plus PAFF/MBAFF in every broadcast-style FRExt stream). README-listed, but the largest single gap against the suite; it is the first
   thing a broadcast or DVD-sourced file would hit.
4. **The H.264 and HEVC references are ffmpeg's decode**, not the suites' own YUV MD5s. For HEVC the official md5 was checked for the 3 streams above (equal). The ITU zips
   (`https://www.itu.int/wftp3/av-arch/jvt-site/draft_conformance/`, `.../jctvc-site/bitstream_exchange/draft_conformance/`) are HTTPS but ship the reference YUVs inside zips (AVCv1 1.7 GB, FRExt 1.4 GB) at ~250 KB/s from that
   server; FATE serves the bitstreams alone at 1 MB/s. A follow-up could pin the ITU HEVC zips (129 MB, md5 only) and compare with the official md5s directly.
5. **fate-suite.ffmpeg.org drops connections** under parallel fetches (several resets and connect timeouts while fetching 195 files next to a second download); `craft-corpus fetch` has no retry, the workflow retries the step (it reuses verified files).

## Corpora

Manifests are in `crates/corpus/manifests/` (generated by `crates/corpus/scripts/build_codec_manifests.py`; HTTPS only; sha256 for every file; licence and provenance per file).
Fetched into `/Users/emm/ArtCraft/corpus/conformance-<codec>` (outside the repo; `CRAFT_CORPUS_DIR` selects the root), total 245 MB (limit was ~3 GB).

| Manifest | Files | Size | Source | Licence |
| --- | ---: | ---: | --- | --- |
| `conformance-h264` | 195 | 134 MB | https://fate-suite.ffmpeg.org/h264-conformance/ (+ `FRext/`): JVT H.264.1 bitstreams; official site https://www.itu.int/wftp3/av-arch/jvt-site/draft_conformance/ is reachable over HTTPS but zipped with YUVs | JVT conformance bitstreams (no licence text shipped) |
| `conformance-hevc` | 184 | 79 MB | https://fate-suite.ffmpeg.org/hevc-conformance/ (JCT-VC H.265.1 v1 + RExt; MV-HEVC/SHVC left out); official: https://www.itu.int/wftp3/av-arch/jctvc-site/bitstream_exchange/draft_conformance/ | JCT-VC conformance bitstreams (no licence text shipped) |
| `conformance-vp8` | 124 (62 + 62 md5) | 4 MB | https://storage.googleapis.com/downloads.webmproject.org/test_data/libvpx/ | BSD-3-Clause (libvpx) |
| `conformance-vp9` | 686 (343 + 343 md5) | 28 MB | same bucket, `vp90/91/92/93-2-*.webm` | BSD-3-Clause (libvpx) |
| `conformance-av1` | 488 (244 + 244 md5) | 8 MB | https://storage.googleapis.com/aom-test-data/ (`av1-1-*.ivf`, 2 `.mkv`) | BSD-2-Clause-Patent (libaom) |

VP9: 19 vectors with no published `.md5` are left out (`bbb_*`, `sintel_*`, `tos_*` tile streams, about 1.4 GB): nothing to compare with, and they would blow the size budget.
AV1: this replaces the unpinned `curl` in `crates/av1/tests/it/common/mod.rs` as the pinned source (that helper is unchanged and still downloads its five vectors).
VP8: `crates/vp8/tests/it/vectors.rs` still reads `$VP8_TEST_VECTORS`; the manifest makes the same 62 vectors fetchable and run in CI.

### Argon (AV1): documented, skipped

Argon Streams AV1 was commercial (Argon Design, sold via CamverTech) and has since been open-sourced by Graphcore and Broadcom through AOMedia
(https://aomedia.org/av1-video-decoder-verification-tool/). It is public over HTTPS, no registration:
`https://aom-cwg-av1-argon-streams-public.s3.us-east-1.amazonaws.com/argon_coveragetool_av1_base_and_extended_profiles_v2.1.1.zip`, **7,028,947,374 bytes** (one zip),
published MD5 `5dd3554c3f73c80f06c7fceeaa1f5dd9` (the `.md5sum` file next to it). The licence is not stated on the page. Skipped here: one 7 GB archive is over the 3 GB budget on this machine,
the manifest format has no zip member extraction (a file needs a sha256 for the zip, then the test would unpack 7 GB), and Argon exercises decoder profiles (Professional/4:4:4, large scale tile, every tool
at the limits) beyond what the AV1 crate claims. A follow-up needs a sha256 of that zip (a 7 GB download), a CI cache plan, and a decision on how much of the Main-profile subset to run.

## CI

`.github/workflows/conformance.yml`: path-filtered (the five codec crates, `crates/conformance`, the manifests, `craft-corpus`, `craft-bitstream`, `craft-media`), a matrix of five jobs,
each: `craft-corpus cache-key` -> `actions/cache` on `corpus-conformance-<codec>-<key>` -> `craft-corpus fetch` (retried) -> `craft-corpus verify --manifest` ->
`cargo test --release -p craft-conformance --test <codec>` with `CRAFT_CORPUS_REQUIRE=1`, so a missing corpus fails instead of skipping. **It cannot run on the fork until the workflow is on `main`** (the fork's
Actions only dispatch workflows present on the default branch); it was not run in CI here, only locally with the same commands.

## Reproduce

```text
cargo build --release -p craft-corpus
for c in h264 hevc vp8 vp9 av1; do target/release/craft-corpus fetch crates/corpus/manifests/conformance-$c.toml ~/ArtCraft/corpus/conformance-$c; done
CRAFT_CORPUS_DIR=~/ArtCraft/corpus CRAFT_CORPUS_REQUIRE=1 cargo test --release -p craft-conformance -- --nocapture
python3 -I crates/conformance/scripts/gen_expected.py h264 ~/ArtCraft/corpus/conformance-h264 > crates/conformance/expected/h264.tsv   # ffmpeg 9.0.2
```

Run time on an Apple-silicon laptop: H.264 2 s, HEVC 6 s, VP8 0.2 s, VP9 7 s, AV1 0.8 s (release).
