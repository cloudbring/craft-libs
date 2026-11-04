# Image speedup remeasurement: craft-libs / craft-jpeg (artcraft-algorithms#190, claim C10)

Measured 2026-11-04 (evening, US Eastern) on Apple M6, 12 logical cores, 32 GiB, macOS (Darwin 27.0.0), rustc 1.99.0, default release profile.
Harness: `tools/perf-remeasure/` (standalone workspace; craft-bench at cloudbring/craft-libs rev 94c6d76e5bc2c936d3db5ce5e19d70213e8390c0).
Raw per-run JSON, fingerprint and revs: `docs/verification/data/*.json`.

## Method

- OLD = storytold/craft-libs `47c9f62530db72bc5c3831c86653b47b20c640be` (merge-base of algo/151-30-huffman with origin/main, also origin/main's tip), renamed git dep `old-jpeg`.
  NEW = this checkout by path at `480f201e7acc9c5ada9ea0bf81370f238ba138de` ("jpeg: lookahead Huffman decoding (#30)"; the branch tip also contains 9508a5f for craft-crw, which craft-jpeg does not use).
- Operation: `craft_jpeg::decode(bytes, Limits::default(), None)`, the public end-to-end decode (Huffman decode, dequantise, IDCT, colour conversion).
- Paired ABBA, 20 timed runs per side per input, 2 warm-ups, 2000 bootstrap resamples. Speedup = old/new; the aggregate is the geometric mean over inputs with a bootstrap 95% CI.
- Output equivalence: for every input the decoded dimensions, component count, damaged flag and every sample (8- and 12-bit, widened to u16) were compared bit-for-bit
  before timing counted; inputs with differing results would be excluded (none were), inputs that fail identically on both sides are listed in the JSON and excluded from timing.
- Load was 3.8 to 4.4 of 12 cores (other processes on this Mac), under the guard limit of 6.0, not idle.

## Inputs

- libjpeg-turbo 3.0.0 test images from `crates/jpeg/tests/data` (10 .jpg files, 73 KB total, 3 to 12 KB each; several are arithmetic-coded or 12-bit, where the change does nothing or little).
- Real: 52 distinct JPEGs extracted from the office corpus (`word/media/*.jpeg`, >= 40 KB, sha256-deduplicated). 43 decode on both sides (6.4 MB); 9 are progressive Huffman, which craft-jpeg
  does not support (identical `Unsupported` error on both sides, excluded).
- Synthetic: 4 encoder-generated images (jpeg-encoder 0.6): RGB 1024x1024, RGB 4000x3000 (two noise levels), gray 4000x3000, dense in AC symbols.
- The original claim's synthetic is a kernel-only loop (not image decode, as the ticket itself says); it is private (`huffman_lookahead_timing`, an ignored unit test), so it was run
  as the app's own bench, 12 separate invocations, unpaired (serial vs lookahead inside the NEW build): see `docs/verification/data/C10-kernel-only-ignored-test.txt`.

## Results

| run | inputs | wall speedup (median ratio old/new) [95% CI] | CPU-time speedup [95% CI] | load before/after (limit) | publishable |
|---|---|---|---|---|---|
| C10-jpeg-libjpeg-turbo | 10 | 1.02x [1.01, 1.03] | 1.02x [1.01, 1.03] | 4.41 / 4.41 (6.0) | yes |
| C10-jpeg-office-real | 43 | 1.11x [1.11, 1.12] | 1.11x [1.11, 1.12] | 4.41 / 3.87 (6.0) | yes |
| C10-jpeg-synthetic | 4 | 1.34x [1.33, 1.35] | 1.34x [1.34, 1.35] | 3.87 / 3.80 (6.0) | yes |

Per-input end-to-end speedups: libjpeg-turbo images 0.94x to 1.14x (median 1.01x; sub-millisecond decodes); office JPEGs 1.03x to 1.29x (median 1.11x);
synthetic 1.27x to 1.42x. Kernel-only ignored test (12 runs): 2.03x median, range 1.99x to 2.22x (ticket: 1.77x).

## Verdict

| claim | claimed | measured | verdict |
|---|---|---|---|
| C10 craft-jpeg Huffman 1.77x | 1.77x, Huffman symbol decode kernel, 13,107,000 symbols | kernel-only (original form, unpaired, n=12): 2.03x [1.99, 2.22 range]. End-to-end `decode`, paired: libjpeg-turbo images 1.02x [1.01, 1.03]; real office JPEGs 1.11x [1.11, 1.12]; synthetic dense images 1.34x [1.33, 1.35] | **partially**: the Huffman kernel speedup reproduces (higher than 1.77x here), output bit-identical on every input, but that is not what an image decode gains: end-to-end the gain is 1.0x to 1.4x and the CI excludes 1.77x on every input set. The status page should say "Huffman symbol decode 1.8x to 2.0x; whole-image decode 1.1x on typical JPEGs". |

## Caveats

- Real JPEGs are the only photo-like inputs available (no camera corpus here); they are mostly small web images.
- The kernel-only figure is unpaired and not alternating, so it carries no CI; it is shown only to compare with the original form of the claim.
- Many libjpeg-turbo inputs decode in about 0.4 to 1 ms, near the harness's timer noise floor for a single call; their per-input CIs are wide relative to the effect.
