# craft-conformance

Test-only crate (`publish = false`): the official codec conformance streams run through the Craft
decoders. One suite per codec, over a pinned `craft-corpus` manifest.

| Suite | Manifest | Streams | Reference output |
| --- | --- | --- | --- |
| `h264` | `conformance-h264` | 195 | `expected/h264.tsv` (ffmpeg decode, see below) |
| `hevc` | `conformance-hevc` | 184 | `expected/hevc.tsv` |
| `vp8` | `conformance-vp8` | 62 | the published per-frame `.md5` files |
| `vp9` | `conformance-vp9` | 343 | the published per-frame `.md5` files |
| `av1` | `conformance-av1` | 244 | the published per-frame `.md5` files |

```text
craft-corpus fetch crates/corpus/manifests/conformance-vp8.toml corpus/conformance-vp8
CRAFT_CORPUS_DIR=$PWD/corpus CRAFT_CORPUS_REQUIRE=1 \
  cargo test --release -p craft-conformance --test vp8 -- --nocapture
```

Without the corpus a suite prints `SKIP corpus ...` and passes, unless `CRAFT_CORPUS_REQUIRE=1` (or
`CONFORMANCE_<CODEC>_CORPUS_REQUIRED=1`) is set. A corpus that does not match its manifest always
fails. Each stream prints a `RESULT` line (`pass`, `fail` or `unsupported`); `CRAFT_CONFORMANCE_REPORT=<file>`
also appends them to a file. A suite fails on any failure not listed in the test's `KNOWN_FAILURES`
(each with a reason), on a listed failure that now passes, and when fewer than `MIN_PASS` streams pass.

`unsupported` is the decoder's own `Error::Unsupported` (or a stream the reference decoder also
rejects); the reasons are listed per suite in `docs/verification/codec-conformance.md`.

H.264 and HEVC come from FFmpeg's FATE mirror, which ships bitstreams without reference output.
`scripts/gen_expected.py` writes the reference digests (frame count and MD5 of all output frames) by
decoding the pinned corpus with ffmpeg; FATE checks that decoder against the suites' own YUV MD5s.
For the streams that mattered here the official MD5s were also checked (see the doc).
