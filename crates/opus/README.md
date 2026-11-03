# craft-opus

Clean-room pure-Rust Opus decoder implementing **RFC 6716**, updated by **RFC 8251**:
SILK, CELT, hybrid packets, OpusHead, multistream surround and packet-loss concealment.
No unsafe code; builds for `wasm32-unknown-unknown`.

## Provenance and clean-room attribution

Moved from `cloudbring/filmcraft`, branch `algo/49-mdct-craft-fft`, commit
`a29f2893f1aa8426c57dcd0bac5bd958dd440883`, directory `crates/opus`:
**7,563 lines in 22 files**, including the original tests and specification comments.
Original work by the FilmCraft contributors, under MIT OR Apache-2.0, retained under
the shared workspace license. This is the #49 version whose CELT inverse MDCT already
uses `craft-fft`, not the older in-crate transform. Migration: artcraft-algorithms
#48, #49 and parent #7.

Decoder algorithms, tables, floating-point operations, packet parsing, and test
thresholds are unchanged. Adaptations are the crate/import name, inherited package
metadata and lint configuration, dependencies, and a local test-only adapter over
`craft-media-testkit`. The source's package-specific development optimization level
and 160-column rustfmt configuration are retained to preserve source formatting
and keep the long conformance tests practical. Runtime dependencies are `craft-fft`
and `craft-bitstream`; the Opus-specific bidirectional range decoder remains verbatim
in `src/range.rs` (it is not the VP8/VP9 boolean decoder or a generic bit reader).

The source's clean-room rules remain applicable: implement codecs from public
specifications and conformance vectors, not reference-decoder source; do not copy
FFmpeg, libopus, or other third-party codec implementations, even permissively
licensed ones. CELT tables retain their RFC section/table attribution in source.
FFmpeg/libopus are external test oracles and fixture generators only: never linked,
bundled or shipped. The upstream asset attribution index contains no committed
Opus vector assets; downloading test data does not relicense it as project code.

## Conformance data (downloaded, never committed)

The Craft family downloads the official RFC 6716 conformance corpus updated for
RFC 8251 from the Opus project; it does not commit the vectors or reference PCM:

```sh
mkdir -p target/fixtures/opus/vectors
curl --fail --location --retry 3 \
  https://opus-codec.org/static/testvectors/opus_testvectors-rfc8251.tar.gz \
  -o target/fixtures/opus/opus_testvectors-rfc8251.tar.gz
(cd target/fixtures/opus && shasum -a 256 -c ../../../crates/opus/test-support/vectors.sha256)
tar -xzf target/fixtures/opus/opus_testvectors-rfc8251.tar.gz \
  --strip-components=1 -C target/fixtures/opus/vectors
CRAFT_MEDIA_REQUIRE_ORACLES=1 cargo test -p craft-opus -- --nocapture
```

The archive contains `opus_newvectors/testvector01` through `testvector12` with
`.bit`, `.dec`, and `m.dec` files. `CRAFT_MEDIA_FIXTURES_DIR` overrides the fixture
root (then use `$CRAFT_MEDIA_FIXTURES_DIR/opus/vectors`); `CARGO_TARGET_DIR` changes
build output only. Oracle audio is generated locally under `opus/oracle` within
that root. All these files stay outside the committed crate.

There are **36 comparisons**: each of 12 bitstreams at 48 kHz in stereo, stereo
with phase inversion disabled, and mono (against the downmixed phase-inversion-free
reference). Every nonempty packet must match its recorded final range-coder state
**exactly**, and the source's `opus_compare`-style spectral quality score must be
**at least 0**. This local metric is calibrated so white noise at 48 dB SNR is near
zero; the calibration test requires scores in [-15, 15] at 48 dB and [60, 101]
at 70 dB. SNR is also reported for vectors, but is not their pass criterion.

The inherited external FFmpeg/libopus tests require exact trimmed PCM length,
CELT-only SNR at least 50 dB, and SILK/hybrid SNR at least 10 dB plus nonnegative
spectral quality where applicable; lower-output-rate cases keep their original
rate-specific checks. None of these tolerances were relaxed for the move.

Without vectors or FFmpeg, local tests may report skips, as in FilmCraft. Set
`CRAFT_MEDIA_REQUIRE_ORACLES=1` to fail on missing fixtures or tools. The per-crate
CI downloads and checksum-verifies the complete vector corpus, checks FFmpeg's
libopus encoder and decoder, and enables this strict mode. The existing
`vector_diag` diagnostic remains ignored by default.

## Checks

```sh
cargo fmt -p craft-opus --check
cargo clippy -p craft-opus --all-targets -- -D warnings
CRAFT_MEDIA_REQUIRE_ORACLES=1 cargo test -p craft-opus
cargo doc -p craft-opus --no-deps
cargo check -p craft-opus --target wasm32-unknown-unknown
```

## Migration validation — November 3, 2026

Validated on macOS arm64 with rustup stable Rust 1.99.0 and external FFmpeg 9.0.2.
Both suites passed **39 tests (30 unit + 9 integration), 0 failures, 1 ignored
vector diagnostic, 0 doctests**. No fixture-dependent tests skipped. All 36
conformance comparisons passed in both crates; their printed range, quality,
SNR and sample-length results were identical. The zero-allocation MDCT tests
also passed unchanged.

The FilmCraft baseline used byte-identical `crates/opus` and `crates/testkit`
exports from the source SHA in a temporary two-member workspace, with the
source package/lints/development profiles and its lockfile-pinned `craft-fft`
revision `79a39babab1242b71217bcc1e7d876ae7991d12a`. The integration branch's
`crates/fft` matches that revision. This is crate-level parity, not a claim of a
full FilmCraft workspace run. Both runs required their external oracles and
used the same downloaded vector files.

`cargo fmt -p craft-opus --check`, `cargo clippy -p craft-opus --all-targets
--locked -- -D warnings`, `cargo doc -p craft-opus --no-deps --locked`, and
`cargo check -p craft-opus --locked --target wasm32-unknown-unknown` passed.
`actionlint .github/workflows/opus.yml` passed. A separate negative check
confirmed strict mode fails rather than silently passing with missing vectors.
The GitHub-hosted workflow itself has not been run (this branch is not pushed).

All Cargo commands used `rustup run stable`, with `RUSTC` and `RUSTDOC` set to
`rustup which --toolchain stable rustc` / `rustdoc`, and that toolchain's `bin`
prepended to `PATH`. This avoids the host's Homebrew `rustc` shadowing the rustup
compiler and missing its installed WASM standard library. All build, baseline,
and fixture artifacts were confined to
`CARGO_TARGET_DIR=/Users/emm/ArtCraft/worktrees/craft-libs-48-opus/target-48`
(with both fixture-root environment variables set there) and removed afterward.

Each cell below is **quality / SNR dB**, the same for FilmCraft and craft-opus.
Every comparison additionally passed the unchanged exact final-range check and
quality >= 0 threshold; SNR is diagnostic, not a vector acceptance threshold.

| Vector | Stereo | Stereo, phase inversion disabled | Mono |
|---|---:|---:|---:|
| testvector01 | 99.9 / 79.8 | 99.9 / 79.8 | 100.0 / 81.5 |
| testvector02 | 94.1 / 15.6 | 94.1 / 15.6 | 94.2 / 15.9 |
| testvector03 | 99.3 / 19.1 | 99.3 / 19.1 | 99.5 / 20.7 |
| testvector04 | 99.0 / 18.5 | 99.0 / 18.5 | 99.1 / 20.4 |
| testvector05 | 97.3 / 20.5 | 97.3 / 20.5 | 97.7 / 23.4 |
| testvector06 | 97.2 / 21.2 | 97.1 / 21.2 | 98.1 / 23.5 |
| testvector07 | 99.9 / 66.3 | 99.9 / 66.3 | 100.0 / 67.4 |
| testvector08 | 100.0 / 65.8 | 100.0 / 65.9 | 100.0 / 67.7 |
| testvector09 | 99.9 / 65.6 | 99.9 / 65.6 | 99.9 / 66.5 |
| testvector10 | 99.7 / 53.4 | 99.7 / 53.4 | 99.7 / 53.6 |
| testvector11 | 100.0 / 82.7 | 100.0 / 82.7 | 100.0 / 84.6 |
| testvector12 | 96.5 / 19.6 | 96.5 / 19.6 | 96.5 / 19.6 |
