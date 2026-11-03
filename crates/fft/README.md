# craft-fft

Shared f32/f64 spectral kernels, independent of apps and usable on native and wasm.
The facade forbids unsafe code. rustfft's dependency internals may use SIMD/unsafe;
this is not a claim that the entire dependency graph forbids unsafe code.

## API and allocation contract

- `ComplexFft::<f32/f64>::new(N)` caches both directions for that size. Use
  `process_in_place(direction, buffer)` or `process_into(direction, input, output)`.
- `RealFft::<f32/f64>::new(N)` caches real forward/inverse kernels, including odd
  sizes. `forward_into` / `inverse_into` preserve input. For owned-buffer processing,
  fill `time_mut()` or `spectrum_mut()`, call `process_in_place(direction)`, then read
  `spectrum()` or `time()`. The source buffer is destroyed; this is typed staging,
  not an unsafe packed real/complex slice reinterpretation.
- `Mdct::new(M, TdacWindow::Sine)` / `Imdct::new(M, TdacWindow::Kbd { alpha: 4.0 })`
  transform `2*M` samples ↔ `M` coefficients. M must be positive and even, including
  120/240/480/960 and 128/1024. `process_into` preserves input and overwrites output.
- `Stft::new(window, hop)` accepts arbitrary finite windows with nonzero energy in
  every hop phase. `analyze_into` consumes a complete frame supplied by the caller;
  `synthesize_into` emits one hop with weighted overlap-add. `flush_into` drains the
  final tail (returning its valid length); `reset` discards history.

Constructors and `sine_window` / `kbd_window` allocate. **No processing, flushing,
reset, or buffer-access call allocates or plans**. Plans own all scratch and are
mutable, single-stream objects; create one per size/precision/worker off the realtime
thread, then reuse it. No global cache, locks, runtime resizing, or per-call planner.
Wrong lengths, zero/overflowing sizes, malformed real-spectrum endpoints, unsupported
MDCT sizes/windows, and invalid hops return `Error` rather than backend panics.

```rust
use craft_fft::{Complex, ComplexFft, Direction};
# fn main() -> Result<(), craft_fft::Error> {
let mut plan = ComplexFft::<f64>::new(4)?;
let mut data = [Complex::new(1.0, 0.0); 4];
plan.process_in_place(Direction::Forward, &mut data)?;
assert!((data[0].re - 4.0).abs() < 1e-12);
plan.process_in_place(Direction::Inverse, &mut data)?;
assert!((data[0].re - 1.0).abs() < 1e-12);
# Ok(())
# }
```

## Exact specification and normalization

For complex input x of length N, forward output is
`X[k] = sum(n=0..N-1) x[n] exp(-2*pi*i*n*k/N)` (unscaled). Inverse uses the
positive exponent and **1/N**. This matches gridcraft's Fourier convention.
Real FFT keeps bins `0..=floor(N/2)`; negative frequencies are the conjugates.
DC and, for even N, Nyquist must have exactly zero imaginary components.
Parseval is `sum |x|² = sum |X|² / N`.

For MDCT, set M = coefficient count and N = 2*M = time-block length:
`C[k] = sum(n=0..2*M-1) w[n]*x[n]*cos(pi/M*(n+1/2+M/2)*(k+1/2))`.
IMDCT is the transpose cosine sum multiplied by **2/M** and the synthesis window.
An individual IMDCT block is aliased: perfect reconstruction requires adding
consecutive equal-window blocks at hop M. Pad a stream with M zeros at both ends,
then discard the padding. Sine and KBD windows obey `w[n]²+w[n+M]²=1`.
KBD alpha 4/6 are the AAC long/short conventions. These are full-overlap transforms,
**not** a drop-in Opus low-overlap codec window or AAC window-switching implementation.

MDCT folds the four time quarters into a length-M DCT-IV. It packs even/reversed-odd
samples into M/2 complex values, pre-rotates by `exp(i*pi*j/M)`, runs one unscaled
positive-exponent FFT, and post-rotates by `exp(i*pi*(j+1/4)/M)` before unpacking.
Thus each direction uses **one N/4 complex FFT**, supporting non-power-of-two sizes.
All rotations/windows are precomputed; the O(N²) cosine implementation is test-only.

STFT uses the FFT convention above, applies its window in both directions, and
divides emitted samples by the accumulated squared-window weights. Zero-weight
boundaries emit zero; pad with at least a frame of zeros and trim to recover those
boundaries for windows with zero endpoints. Each synthesis call emits hop samples
after receiving a complete frame (N-hop lookahead relative to a hop-based stream).
Drain after the final frame; reset between unrelated streams.

## Dependencies and verification

Runtime: rustfft (MIT OR Apache-2.0), realfft (MIT). Independent MDCT test oracle:
rustdct (MIT OR Apache-2.0); allocation-counter (MIT OR Apache-2.0) verifies zero
processing allocations, including the first call. No workspace policy requires vendoring these crates.
SIMD selection belongs to rustfft; no platform-specific unsafe code lives here.

The equations above are the exact-real DFT/MDCT specification, not a machine-checked
floating-point proof. Integration tests compare with direct DFTs (f32 `1e-4*N`,
f64 `1e-9*N`), Parseval, round trips, real/complex agreement, randomized sizes,
TDAC overlap-add, rustdct, and STFT identity. The exhaustive large DFT test is
deliberately slower than a selected-bin smoke test.

Differential vectors: `filmcraft/crates/opus/src/celt/mdct.rs::tests::fft_matches_dft`,
source commit `eb59ec09daaea519c9c8fffa404a594ae50d00e0`.
Verbatim timing baseline: `gridcraft/crates/engine/src/cmd/analysis.rs::fft`,
source commit `85e51e009c24fad47575f04d3c08f12342615d82`.
Run `cargo test -p craft-fft --release --test gridcraft_timing -- --ignored --nocapture`
to compare warmed, preplanned transforms (no planning or allocation in either loop).
This crate does not migrate apps or claim codec bitstream compatibility.

Manual sample, 2026-11-03, aarch64 macOS, rustup stable Rust 1.99.0, release mode,
200 warmed f64 forward transforms per size (both loops include an input-buffer copy):

| N | gridcraft radix-2 | craft-fft | Speedup |
| ---: | ---: | ---: | ---: |
| 128 | 2.521 µs | 0.275 µs | 9.16× |
| 1024 | 21.794 µs | 3.190 µs | 6.83× |
| 8192 | 225.713 µs | 31.212 µs | 7.23× |

These are local observations, not a cross-platform performance guarantee.
