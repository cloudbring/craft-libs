# craft-jpeg

A clean-room JPEG decoder for the Craft apps, written from ITU-T T.81 (ISO/IEC 10918-1). It
covers the processes that common Rust JPEG decoders skip: 12-bit samples and arithmetic coding.
No `unsafe`, no panics on malformed input, and it builds for `wasm32-unknown-unknown`. The only
dependency is `thiserror`.

```toml
craft-jpeg = { git = "https://github.com/storytold/craft-libs", rev = "<commit>" }
```

## What it decodes

| Frame | Process | Precision |
|---|---|---|
| SOF0, SOF1 | sequential DCT, Huffman coded | 8 or 12-bit |
| SOF9 | extended sequential DCT, arithmetic coded | 8 or 12-bit |
| SOF10 | progressive DCT, arithmetic coded | 8 or 12-bit |

Every other frame returns `Error::Unsupported`: progressive Huffman (SOF2), lossless (SOF3,
SOF11), hierarchical (SOF5 to SOF7, SOF13 to SOF15), other precisions, more than one frame, and
a height given only in a DNL marker. Images have 1, 3 or 4 components with any sampling factors
and restart intervals.

## Use

```rust
use craft_jpeg::{decode, Limits, Samples};

let img = decode(&bytes, &Limits::default(), adobe_transform)?;
match img.samples {
    Samples::U8(px) => { /* 8-bit frames */ }
    Samples::U16(px) => { /* 12-bit frames, scaled to 16 bits */ }
}
if img.damaged { /* warn: part of the image is missing */ }
```

- `decode(bytes, &Limits, adobe_transform: Option<u8>) -> Result<Decoded, Error>`.
  `adobe_transform` is the transform byte of the APP14 `Adobe` segment, when the app found one.
- `frame(bytes) -> Option<FrameKind>` reads the headers up to the first SOFn marker and returns
  that marker and the declared sample precision, without decoding.
- `Error` is `Malformed`, `Unsupported` or `LimitExceeded`, each with a message.

## Output

`Decoded` has `width`, `height`, `components`, `samples` and `damaged`.

- `samples` is interleaved by pixel, rows top to bottom: `Samples::U8` for 8-bit frames,
  `Samples::U16` for 12-bit frames, scaled to the full 16-bit range
  (`round(v × 65535 / 4095)`).
- `components` is 1 (gray), 3 (RGB) or 4 (CMYK).
  - 3 components are converted from YCbCr to RGB (JFIF, T.871), unless the Adobe transform is
    0 or the component IDs are `R`, `G`, `B`.
  - 4 components are ink amounts (0 = no ink). The stored values are taken to follow Adobe's
    inverted convention (maximum = no ink) and are inverted back. With Adobe transform 2 (YCCK),
    C, M and Y are converted from YCbCr, which yields ink amounts directly, and K is inverted.
- `damaged` is set when entropy-coded data was cut short or corrupt after at least one MCU had
  decoded, or its scans would decode more than 64 passes over the frame's blocks. The rest of
  that scan, and every scan past the budget, is left out; those blocks keep what earlier scans
  decoded. A stream with no decodable image data is an error. Arithmetic-coded data that stops
  early at a marker reads as the zero bytes encoders drop at the end of a scan (libjpeg-turbo
  treats it the same way), so for arithmetic scans only data that runs to the end of the file
  and a missing restart marker count as damage.

## Limits

`Limits` (fields `max_pixels`, `max_alloc`) is checked against the frame header before anything
is allocated: `width × height` against `max_pixels`, and the output buffer plus the decoder's
own coefficient and sample buffers, together, against `max_alloc`. The defaults are 2³⁰ pixels
and 8 GiB on 64-bit targets, 1 GiB on 32-bit ones (wasm32). The struct is `#[non_exhaustive]`:
start from the defaults and set the fields you change:

```rust
let mut limits = craft_jpeg::Limits::default();
limits.max_alloc = 512 << 20;
```

A file of under a hundred bytes can declare a frame that fills the default limits, and
decoding it still renders every sample. Callers that decode for a UI or for previews should
pass a `max_alloc` well below the default.

## Routing

An app that decodes baseline JPEG with another decoder keeps its own rule for which files come
here, built on `frame`. PhotoCraft sends arithmetic-coded files (SOF9 to SOF11) and SOF0/SOF1
files whose precision is not 8 bits:

```rust
fn use_craft_jpeg(bytes: &[u8]) -> bool {
    match craft_jpeg::frame(bytes) {
        Some(f) if (0xC9..=0xCB).contains(&f.marker) => true,
        Some(f) if matches!(f.marker, 0xC0 | 0xC1) => f.precision != 8,
        _ => false,
    }
}
```

SOF11 (lossless) falls in that range and returns `Unsupported`.

## Inverse DCT

The separable inverse transform transposes the 11-multiply, 29-add butterfly/rotation graph
in figure 1 of C. Loeffler, A. Ligtenberg and G. S. Moschytz, *Practical fast 1-D DCT algorithms
with 11 multiplications* (ICASSP 1989, pp. 988–991,
[DOI 10.1109/ICASSP.1989.266596](https://doi.org/10.1109/ICASSP.1989.266596)). The even rotation
is at 6π/16 with gain √2 (the figure's printed c1 is c6); the odd rotations are at 3π/16 and
π/16 with unit gain. Each rotation `(c·a − s·b, s·a + c·b)` uses the shared product
`c·(a+b)` and the two products `(c+s)·b`, `(s−c)·a`. This implementation was derived from
that graph and the T.81 A.3.3 transform, not from IJG implementation code. The existing
libjpeg-turbo decoded fixture remains an independent output reference.

### Precision and rounding

- Dequantization is exact `i64` multiplication. Signals carry eight fractional bits (Q8).
  The three rotation triplets `(c, c+s, s−c)` and √2 are independently rounded Q56 constants.
  Blocks with dequantized Q8 magnitudes at most `2²² − 2` use rounded Q32 constants
  in `i64`, add 2³¹, and arithmetically shift right 32 bits. Other blocks use the full
  Q56 `i128` product, add 2⁵⁵, and shift right 56 bits. Both round to nearest, with exact
  ties toward positive infinity. The narrow path's extra error is below 1/16 Q8 unit per
  product and avoids wide multiplications on ordinary 8-/12-bit blocks.
- A line produces √8 times the T.81 one-dimensional inverse. After both passes, division
  by `8 × 256` removes that gain and the Q8 scale. Fractions are retained in the existing
  `f32` sample planes; they are not rounded to integer samples before chroma interpolation
  and color conversion. The unchanged output stage clamps and rounds nonnegative samples
  to nearest (ties upward), scaling 12-bit samples to `0..=65535` first.
- The DC-only block shortcut computes `F00 / 8` exactly before conversion to the sample
  plane. A DC-only line also skips the rotations, covering first-row/first-column sparsity.
  All coefficient orders and quantization tables remain unchanged.

### Integer headroom

The bounds include malformed inputs, not just valid 12-bit DCT ranges: an `i32` coefficient
times any `u16` quantizer has magnitude below 2⁴⁷; its Q8 representation is below 2⁵⁵.
For a line whose input magnitudes are bounded by B, the absolute row sum of the scaled
inverse matrix is below 7.473, including Q56 constant quantization. Product rounding and
the bounded Q32 approximation add less than four Q8 units to an output, so `8B + 8` bounds
each returned value. Evaluating
the butterfly graph with triangle bounds gives `16B + 8` for every internal sum/difference.
Thus first-pass outputs are below `2⁵⁸ + 8`, and every second-pass intermediate is below
`2⁶² + 136`, safely inside `i64`. Multipliers have magnitude below 2⁵⁷, so full products
and rounding offsets also fit comfortably in `i128` (magnitude below 2¹²⁰).
The narrow path multiplies a magnitude below 2²⁹ by a Q32 factor below 2³³; its product
plus rounding offset has magnitude below 2⁶³. Selecting the path once per block bounds
every second-pass intermediate by `128B + 136 < 2²⁹`, without per-product branching.

`src/idct_tests.rs` checks 8,192 seeded random blocks against a separate f64 T.81 matrix
transform, including 8-/12-bit coefficient ranges and 8-/16-bit quantizers. It also checks
every frequency impulse, sparse axes, all 65,536 signed 16-bit DCs, all 256 sign patterns
at both line bounds, full-i32 coefficient extremes and randomized malformed blocks.
The retained test-only previous f32 reconstruction compares final pixels for every JPEG
fixture and fuzz seed: differences must be at most one **output** LSB, including scaled
16-bit outputs. Bit identity is not guaranteed because the old f32 cosine matrix and the
fixed-point graph have different intermediate rounding; entropy transcodes remain identical.

For whole-decode timing (200 iterations of the largest image, 512×512 whitetail, and the
largest 12-bit fixture), run on the old and new revisions with the same release toolchain:

```sh
cargo test -p craft-jpeg --release --locked --test idct_timing -- --ignored --nocapture
```

## Boundaries

- In scope: decoding the frames above into samples.
- Out of scope: metadata. The decoder skips every APPn and COM segment. ICC profiles, EXIF,
  XMP, Photoshop resources, resolution and the APP14 `Adobe` segment are the app's to read.
  Color management and encoding also stay in the apps.

## Testing

`cargo test -p craft-jpeg` runs:

- unit tests in `src/tests.rs`: arithmetic-coded streams from a test-only encoder must decode to
  the coefficients that went in, and Huffman streams are compared against `zune-jpeg`.
- `tests/conformance.rs`, on libjpeg-turbo's test images in `tests/data` (provenance and how
  the derived files were made: `tests/data/SOURCE.md`):
  - `testimgari.jpg` (SOF9) decodes to exactly the samples of its jpegtran Huffman transcode.
  - jpegtran's arithmetic and progressive arithmetic transcodes of `testorig.jpg` (one with
    restarts) and `testorig12.jpg` decode to exactly the samples of their sources.
  - `whitetail-ari.jpg` (SOF9 from cjpeg, a white lower part that decodes from the zeros
    supplied at EOI) decodes undamaged and equal to its Huffman transcode.
  - `testorig.jpg` matches `zune-jpeg`, and `testorig12.jpg` matches libjpeg-turbo's 12-bit
    decode, allowing for fractional reconstruction and color-conversion differences
    (mean under 0.75, max 4).
- `tests/mutation.rs`: a seeded mutation run (truncations, bit flips, byte and frame-header
  overwrites, deleted and repeated spans) over the test images and the fuzz seeds. Decoding must
  not panic and must stay within small `Limits`.

`.github/workflows/jpeg.yml` runs fmt, clippy, the tests and a `wasm32-unknown-unknown` check on
Linux.

### Fuzzing

`fuzz/` is a cargo-fuzz crate outside the workspace, with one target, `decode`. Its seeds in
`fuzz/corpus/decode` are the test images plus small SOF9 (with restarts), 12-bit SOF9, SOF10
and 12-bit SOF1 streams. With nightly and `cargo install cargo-fuzz`:

```sh
cd crates/jpeg/fuzz
cargo +nightly fuzz run decode
```

Running it adds the inputs it finds to `fuzz/corpus/decode`; commit only seeds that cover
something new.

## Credits

The decoder was written by Michael Higgins (Higgs32584) for PhotoCraft.
