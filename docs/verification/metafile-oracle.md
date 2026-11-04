# craft-metafile swap oracle (cloudbring/artcraft-algorithms#185)

Proves, or measures, what the `craft-metafile` swap of #92 changed on real metafiles: the
EMF/WMF/EMF+ reader that DeckCraft and WordCraft each carried was replaced by one shared crate.
Harness: `tools/metafile-oracle/` (own `[workspace]` and lock). Registered differences:
`tools/metafile-oracle/expected.toml`. Evidence standard: research/VERIFICATION.md section 1.1.

## Result

| App | Files | Identical | Expected difference | Both fail (same message) | Regression | Only-old / only-new fails, timeouts, panics |
|---|---:|---:|---:|---:|---:|---:|
| DeckCraft | 1361 | 1343 | 7 | 11 | 0 | 0 |
| WordCraft | 1361 | 201 | 1149 | 11 | 0 | 0 |

"Zero regressions" means zero unexplained differences: every non-identical file is attributed to a
cause (below) and each cause is registered with evidence. It does not mean the apps behave the
same: **WordCraft's output changed on 1149 of 1350 parseable metafiles (85%)**, almost all of it
because the shared reader plays records the old WordCraft reader skipped. Three things are *not*
improvements and are reported as findings, not hidden in the registry (section "Findings").

DeckCraft: of 1343 identical files, 1332 draw something (items and a non-transparent pixel) and
1139 contain text runs, so the equal rasters include real glyph drawing. The 7 differences are
fixes (one intended in #92, six found by this oracle, below). WordCraft: 194 identical files draw
something, 37 with text.

Per format (both apps parse the same 446 EMF and 915 WMF):

| | EMF identical | EMF expected diff | EMF both-fail | WMF identical | WMF expected diff | WMF both-fail |
|---|---:|---:|---:|---:|---:|---:|
| DeckCraft | 436 | 1 | 9 | 907 | 6 | 2 |
| WordCraft | 143 | 294 | 9 | 58 | 855 | 2 |

The 11 both-fail files fail identically on all four readers with the same message (9 EMF "no
readable records", 2 WMF "not a WMF or EMF file"): truncated or non-metafile BLIP payloads.

## Corpus

`cargo run --release --bin extract -- <corpus-root> <cache-dir>` walks `office-poi`,
`office-openxml-sdk` and `office-libreoffice` and writes `<sha256>.<emf|wmf>` plus `manifest.tsv`.
Sources:

- OOXML zips (`.docx .docm .pptx .pptm .xlsx .xlsm`): every part ending `.emf .wmf .emz .wmz`
  (gunzipped when compressed): 1053 occurrences.
- OLE/CFB (`.doc .ppt .xls`): OfficeArt BLIP records for EMF (0xF01A) and WMF (0xF01B) in every
  stream (`Data`, `WordDocument`, `Pictures`, `Workbook`, nested `ObjectPool`), inflated per the
  BLIP header: 526 occurrences. The WMF BLIPs carry no placeable header (that is how Office stores
  them) and are fed to the readers as stored.
- 5831 Office files scanned; 355 contain metafiles; 1579 occurrences; 1361 unique by SHA-256
  (446 EMF, 915 WMF).

| Corpus tree | Source files with metafiles | Occurrences | Unique |
|---|---:|---:|---:|
| office-poi | 100 | 536 | 456 |
| office-openxml-sdk | 25 | 255 | 212 |
| office-libreoffice | 230 | 788 | 694 |

The manifest (source file, part or `stream@offset`, sha256, kind, bytes; one line per occurrence)
is `docs/verification/metafile-oracle-manifest.tsv`. The metafiles themselves are not committed
(cache under the scratchpad). The corpus has no standalone `.emf/.wmf` files and no OLE
`OlePres` or `CONTENTS` metafiles (not extracted).

Not exercised: all 136 corpus files that carry EMF+ records are dual (`EMF+ Header` flag 1), so
every reader plays their plain-EMF part; EMF+-only playback has no corpus case. The three
oversized-input cases of the DeckCraft parity run (#92 comment) are not in the corpus either.

## Sides

| | OLD | NEW |
|---|---|---|
| DeckCraft | `storytold/deckcraft` @ `3e178d8f5a59c787a6a53a7ed0f052b3e4a1489f` (merge-base of `algo/92-use-craft-metafile` and `origin/main`): crates `deckcraft-metafile` (parser) and `deckcraft-render` | `cloudbring/deckcraft` @ `4c29ec9cbe83d5d9484c67fcf57188de6e41df1a`, `deckcraft-render` on this tree's `craft-metafile` (`[patch]` to `../../crates/metafile`) |
| WordCraft | `storytold/wordcraft` @ `b47079fc791595fd84bb499d64dcd3f0624ef35a`: `wordcraft-metafile`, `wordcraft-render` (+ doc, layout, geom) | `cloudbring/wordcraft` @ `02a7f13d4cdd17340a850dc5652773a6afa9a820`, same crates on `craft-metafile` |

Both swap branches name `craft-metafile` by branch (`algo/integration`); the `[patch]` makes NEW
this working tree, so the harness tests the code under review, not the published branch.

Per metafile and app each side reports (1) parse ok or the error text (an `Err` side lets
craft-oracle classify both-fail / only-old / only-new), (2) the parsed record stream, (3) a raster
drawn by the app's own renderer. The judge is strict: records equal and rasters byte-identical.

- Record stream. DeckCraft: the whole scene as `Debug` (the types match field for field). WordCraft:
  a projection onto what both scenes carry (paths with segments, fills, strokes; bitmaps with a
  pixel hash; text with string, dx, font name, bold, italic, colour, position, size, angle and
  alignment, mapped from the shared affine and rounded to 1e-3). Clip regions are not projected
  (neither renderer draws them).
- Raster. DeckCraft: `deckcraft_render::decode_image` (longer side 1024 px). WordCraft:
  `wordcraft_render::render_items` of one `Draw::Image` over a document holding the bytes, at
  min(1024 / longer side, 4) px per point of the picture's own frame, the same for both sides.
  Metrics on a difference: max abs, p99, SSIM, CIEDE2000 mean and max, bad-pixel fraction
  (craft-compare); the diff images of the run are written next to the report (not committed).
- Tolerance: none. "Within 1/255" is never accepted silently; the 30 files that differ by 1/255
  are registered with their cause (float rounding, below).

### Attribution by masking (WordCraft)

A difference is attributed by replacing families of records, in the input of *both* sides, by
same-size no-ops (an EMF comment without payload, an unknown WMF function) and taking the first
level at which old and new agree byte for byte (records and raster):

1. records the pre-swap WordCraft reader did not handle (COVERAGE.md, WordCraft column);
2. plus SETROP2;
3. plus the EMF text-out records (WMF text is level 1);
4. plus pattern and mono brush creation.

The judge reports the level as the metric (`gap-records`, `gap-records+rop2`,
`gap-records+rop2+text-drawing/<cause>`, `...+pattern-brushes`), so `expected.toml` registers
classes with `metric =`. A difference that no level explains keeps its raw metric (`records`,
`max_abs`) and fails the run: there are none. At level 1 the harness also records which family
alone, with all others masked, still differs; at level 3 the text cause is read off the two record
streams. The masking, not the pixel statistics, is the evidence.

## DeckCraft: 7 differences, all fixes

Attribution is by masking, as for WordCraft: the named records replaced by no-ops in the input of
both sides make old and new identical.

**`deckcraft-extent-scaling` (1, EMF).** `301fe29a...emf` (office-poi `spreadsheet/60460.xls`,
`Workbook@5497`). The file scales its viewport with EMR_SCALEVIEWPORTEXTEX (record 31: 700/3456,
250/844). The shared reader honours records 31/32 (added from VectorCraft in #92); the pre-swap
DeckCraft reader ignored them and placed text runs 4.9x too far right (x = 1691 pt in a picture
490 pt wide, i.e. off the image; 34 of 46 records differ). The writer's own device bounds confirm
the new placement: EMR_EXTTEXTOUTW bounds start at 387 px for logical x = 290, and the viewport
553 px x 700/3456 = 112 px over a window of 84 units is 4/3 px per unit (290 x 4/3 = 387).

**`deckcraft-select-clip-region` (6, WMF): a bug in craft-metafile found by this oracle, fixed.**
META_SELECTCLIPREGION (0x012C) with the null region restores the default clip; Office writes
`INTERSECTCLIPRECT .. text .. SELECTCLIPREGION(0)` around every clipped text run
(`GeomLec1.pptx` `image5.wmf`: 22 such records, and two of its INTERSECTCLIPRECTs are empty rectangles). Neither
the pre-swap DeckCraft reader nor the shared reader handled 0x012C, so the first clip rectangle
stayed in force: later shapes outside it vanished (the arcs of `image5.wmf`, the red markers of
`23884_defense_FINAL...ppt` `Pictures@802575`: 317 vs 324 records). Before the fix the oracle
showed it as three WordCraft files whose differences only the clip family explained (WordCraft's
old reader ignored clips, so there it was a swap regression, not a pre-existing bug); the same six
DeckCraft WMF files differed after the fix because DeckCraft's old output was wrong. Fix:
`crates/metafile/src/wmf.rs` resets the clip on 0x012C (regions are object slots only, so any
selection ends the rectangle clip; the picture is drawn, not lost). Red-then-green:
`wmf_select_clip_region_restores_the_default_clip` failed with the clip still
`Some([[10,5,40,30]])` before the fix; `cargo test -p craft-metafile` passes 190 after it.

No other DeckCraft difference exists in 1361 files (1343 identical, 11 same-message failures).
This agrees with the #92 parity run (4,554 exact matches, 4,949 matching errors) on an independent
corpus.

## WordCraft: 1149 differences by cause

| Class (metric) | Files | EMF / WMF | Worst max abs | Lowest SSIM | Worst dE max |
|---|---:|---:|---:|---:|---:|
| `gap-records` | 850 | 0 / 850 | 255/255 | 0.00002 | 101.2 |
| `gap-records+rop2` | 3 | 2 / 1 | 255/255 | 0.0005 | 101.2 |
| `...+text-drawing/opaque-cell-fill` | 14 | 14 / 0 | 255/255 | -0.014 | 100.0 |
| `...+text-drawing/face-name-nul` | 30 | 30 / 0 | 255/255 | 0.9115 | 100.0 |
| `...+text-drawing/affine-scale-over-1pct` | 40 | 40 / 0 | 255/255 | 0.7191 | 100.0 |
| `...+text-drawing/affine-scale-up-to-1pct` | 145 | 145 / 0 | 74/255 | 0.99966 | 18.7 |
| `...+text-drawing/float-rounding` | 30 | 30 / 0 | 1/255 | 1.0 | 0.86 |
| `...+text-drawing+pattern-brushes` | 37 | 33 / 4 | 255/255 | 0.0051 | 100.0 |

(max abs is on a 0..255 scale over RGBA; SSIM and dE are per image. The large values are where
content appears that the old reader did not draw, i.e. wholly different images.)

### `gap-records` (850, WMF only): the shared reader plays records WordCraft skipped

Masking those records makes old and new identical. Which family alone still differs when all
others are masked: WMF text-out 372, text-out plus PATBLT 450, PATBLT alone 28. The records: TEXTOUT / EXTTEXTOUT with CREATEFONTINDIRECT, SETTEXTCOLOR,
SETTEXTALIGN, SETBKMODE/COLOR; PATBLT; INTERSECTCLIPRECT/EXCLUDECLIPRECT (carried as regions that
neither renderer draws; with the SELECTCLIPREGION fix they never change a pixel alone). The old reader parsed a WMF's shapes
and bitmaps and dropped its text (a WMF equation or chart label simply missing), PATBLT fills and
frames. New output is a superset; nothing the old reader drew is removed. Sample: an equation WMF
(`equation-1.docx`, `word/media/image69.wmf`) goes from no text runs to the sigma and equals signs;
a thumbnail `.wmf` goes from 1 item to 10.

### `gap-records+rop2` (3): known limitation, a regression of sorts

LibreOffice-written files (`tdf136841.docx` `word/media/image1.emf`, `WPC_MulticolorGradient.docx`
`docProps/thumbnail.emf`, `tdf110440.xlsx` `docProps/thumbnail.wmf`) draw a gradient in
R2_XORPEN, a shape in R2_COPYPEN, and the same gradient again in R2_XORPEN: the XOR-mask idiom
that leaves the gradient inside the shape only. For `image1.emf` the two XOR passes are the same
79 polygons, verified from the records. The shared reader (DeckCraft's rule: XOR-mixed shapes
only remember their colour and are skipped, "an XOR pair cancels", `canvas.rs: mix_skips`) draws
only the copy-mode shape, a black arrow; the old WordCraft reader drew every XOR polygon as a copy,
a red gradient also outside the arrow. Masking SETROP2 makes old and new identical. **Neither is
the GDI result** (gradient inside the arrow only, which needs per-pixel XOR); for these three files
the new output is arguably further from the truth than the old. DeckCraft shipped this behaviour
already. Registered as a known limitation (`wordcraft-xorpen-mix`), not as an improvement.

### `text-drawing/opaque-cell-fill` (14, EMF): ETO_OPAQUE backgrounds

Excel ActiveX control images (`MiscControlTests.xls`, `ActiveXControls-O12-XL-Controls.xlsx`):
EMR_EXTTEXTOUTW with ETO_OPAQUE and SETBKCOLOR white fills the text rectangle. New emits the fill,
old drew nothing and the control showed through (raster mostly transparent vs white, SSIM near 0).
New is right (the control's text box is white in Office).

### `text-drawing/face-name-nul` (30, EMF): old reader bug

The pre-swap reader kept all 32 characters of the LOGFONT face name, including the bytes after the
NUL (record 4 of `PPTWithAttachments.pptm` `image5.emf`: `"Arial\0<garbage>"`). No font resolved and
the renderer fell back. The shared reader trims at the NUL ([MS-EMF] 2.2.13). Everything else in
the records is equal.

### `text-drawing/affine-scale-*` (40 over 1%, 145 up to 1%; EMF): glyphs now follow the file's scale

The old text model had one isotropic size (the y scale) and spaced glyphs by the dx array scaled
in x. The shared model carries the full affine. In 185 files the x and y scale differ (1e-6 to
0.32 relative), so the old glyphs were too wide or too narrow for their advances. Two files show
it: `CalcFont.xls` (`image` "Run Tests": kx 0.503, ky 0.670) and ActiveX `button1`: the old
raster reads "ActiveXbutton1" (glyphs overlap, the space vanishes); the new raster reads
"ActiveX button1". The projection equalises position, size (y scale), dx (x scale), angle,
alignment, font and colour, so only the glyph x scale differs. The sub-1% class is the same
mechanism at a fraction of a pixel (worst 74/255 on one edge pixel of a very large glyph, SSIM at
least 0.9996). The same model shipped in DeckCraft, and the 1344 DeckCraft files with text show it
equal between its old and new readers.

### `text-drawing/float-rounding` (30, EMF): 1/255

Isotropic text (|kx/ky - 1| below 1e-6) with equal projected records; the glyph transform is built
in f64 from the logical height and the affine instead of from an f32 pre-scaled size. Antialiased
edge pixels differ by at most 1/255 on at most 1.3e-5 of the image, dE max 0.86. This is the only
class that is a pure numeric difference.

### `pattern-brushes` (37: 33 EMF, 4 WMF)

Pattern, DIB-pattern and mono brushes: old drew the outline only (chart bars), new fills with the
brush tile and WordCraft's renderer repeats it (`bug60345_suba.ppt`, `tdf111974.xlsm`). These
files also differ in text drawing; masking the brush-creating records as well makes old and new
identical.

## Findings

1. **Latent swap bug in WordCraft's adapter (not in craft-metafile; no corpus file reaches it).**
   The pre-swap reader called a font bold from LOGFONT weight 600 (`bold: font.weight >= 600`,
   `canvas.rs:680`). The swapped renderer and PDF text use `t.font.weight >= 700`
   (`crates/render/src/media.rs`, `crates/pdf`). Semibold fonts (weights 600 to 699: Segoe UI
   Semibold, Calibri semibold requests) draw regular instead of bold. The corpus weights are
   {0, 300, 400, 700, 900}, so no file shows it; the ignored test
   `semibold_text_is_bold_before_and_regular_after_the_swap` patches weight 700 to 600 in a corpus
   EMF and shows bold runs 26 (old) vs 0 (new). Fix in the wordcraft swap branch: `>= 600`. It
   cannot be fixed here (the app repository is not part of this tree); the test fails once it is.
2. **R2_XORPEN gradient idiom (3 files): new is worse than old** (above). Needs per-pixel XOR or an
   XOR-mask idiom in craft-metafile; DeckCraft has the same gap.
3. Corpus blind spots: EMF+-only playback (no corpus case), non-placeable WMF outside BLIP
   storage, OLE `OlePres`/`CONTENTS` metafiles. The two VectorCraft divergences from #92
   (non-placeable WMF unit scaling; unbracketed MoveTo/LineTo coalescing) are VectorCraft-vs-shared
   and not part of this oracle (VectorCraft keeps its reader; only its differential test exists).
4. The harness found one bug in `craft-metafile` (WMF SELECTCLIPREGION never reset the clip,
   DeckCraft section above); it is fixed here with a red-then-green test. Every other difference is
   an old-reader limitation the shared reader fixes, or the two limitations above that predate
   the swap in DeckCraft.

## Reproduce

```text
cargo run --release --manifest-path tools/metafile-oracle/Cargo.toml --bin extract -- <corpus-root> <cache>
METAFILE_CACHE=<cache> METAFILE_ORACLE_OUT=<out> cargo test --release --manifest-path tools/metafile-oracle/Cargo.toml -- --ignored --nocapture
```

Needs network (git dependencies on the four app revisions). About 40 s per app after a first
build of roughly 1 GB (use one `CARGO_TARGET_DIR` and delete it afterwards). Output per app:
`report.json`, `report.md`, `details.txt` (per file: sources, first differing records, raster
metrics, attribution; summary counts at the end), `failures/` and the old/new PNGs of every
non-identical raster. `dump_one` and `probe_mask` are debug aids (`METAFILE_FILE`, `MASK`,
`MASKW`, `SETDIFF`).
