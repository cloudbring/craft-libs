# craft-compare

Shared comparison metrics, so a verdict means the same thing in every Craft app. Pure Rust,
no dependencies by default, no `unsafe`. Ticket: artcraft-algorithms#179.

| Module | Metrics |
| --- | --- |
| `bytes` | `compare_bytes(a, b) -> BytesDiff { equal, first_diff, len_a, len_b }` |
| `image` | `Image { width, height, channels, data }` (RGBA8 via `from_rgba8`, or f32 planes); max/mean/p99 abs error, PSNR, SSIM (Gaussian 11x11, sigma 1.5, luma), CIEDE2000 mean/max, bad-pixel fraction; `compare_images` -> `ImageReport`; `ImageTolerance::verdict` |
| `audio` | `snr_db`, `max_abs_error` over interleaved `f32` |
| `xml` | `compare_xml` -> `Option<path>` of the first difference, canonicalised (attribute order, inter-element whitespace, comments, namespace prefixes) |
| `text` | `word_f1`, `reading_order_score` |
| `numbers` | `Tolerance` (abs + rel), `ulp_distance_f32/f64`, `abs_rel_error` |
| `package` | `Package` (part name -> bytes) from `from_zip` (feature `zip`), `from_cfb` (feature `cfb`) or `open` (by magic); `compare_packages` -> `PackageDiff { only_in_a, only_in_b, common: [PartDiff { name, status }] }`, status `Identical`, `Equivalent` (canonical XML equal), `XmlDiffers { path }`, `BytesDiffer { first_diff, .. }`; `verdict()` |
| `loudness` | BS.1770-4 `integrated_lufs(interleaved, channels, rate, weights)` (K-weighting for any rate, 400 ms blocks at 75 % overlap, -70 LUFS and -10 LU gates), `STEREO`/`FIVE_ZERO`/`FIVE_ONE` weights, `compare_loudness` |
| `layout` | `TextLine { page, x, y, text }`; `compare_layout` -> `LayoutReport`: LCS line matching, per-page same-break %, median abs dy, max abs dx, page-start match, `dx_outliers`, break runs |
| `verdict` | `Verdict { Identical, Within { metric, value }, Exceeds { metric, value, limit } }` with `at_most`, `at_least`, `combine` |

Feature `png` adds `image::write_diff_png` (amplified per-pixel difference). Features `zip` and
`cfb` pull in `craft-archive` / `craft-cfb` for `package`; the default build has no dependencies.

## Provenance

Ported, not depended on: image error metrics from vectorcraft `testkit::raster` and filmcraft's
render tolerance; word F1 and reading-order from pdfcraft's `xtask` text oracle (two empty
texts score 1.0 here, 0.0 there). CIEDE2000 is checked against Sharma, Wu and Dalal 2005 test
data (20 pairs). Loudness is checked against EBU Tech 3341 (2023) minimum-requirement test
signals 1 to 6 (synthesised in the tests, 0.1 LU tolerance) and the BS.1770-4 48 kHz filter table.
`layout` is ported from wordcraft `xtask visual-diff` (`lcs`, dy/dx/same-break/page-start).
