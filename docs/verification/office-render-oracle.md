# Office render oracle: LibreOffice vs wordcraft, gridcraft, deckcraft

Ticket: cloudbring/artcraft-algorithms#194 (V4.1). Tool: `tools/office-render-oracle/`. Run on 2026-11-04 (US Eastern).

LibreOffice is a **reference, not ground truth**. Every number below is agreement with its PDF, and
a disagreement can be our bug, LibreOffice's bug, or a convention difference. The triage section sorts the 30 worst files into those three.
Evidence level for "our DOCX/XLSX/PPTX PDF export agrees with an independent renderer at the rates below": L3 (differential over a pinned-corpus sample);
not L4, because the oracle is not authoritative (VERIFICATION.md 1.3).

## Versions

| Item | Version |
|---|---|
| LibreOffice | 26.8.2.2 (`9dea406ab7695df6325b11b0e19fdca8db4b7276`), `brew install --cask libreoffice`, dedicated profile via `-env:UserInstallation` |
| poppler (pdftoppm, pdftotext) | 26.10.0 |
| craft-compare | craft-libs `3b868c0` (`algo/integration`), path dependency |
| wordcraft-cli | origin/main `6f552abf8e8d9f8861706b3cd8313d4328fdb7b5` |
| gridcraft-cli | origin/main `919a6b58fe6661de27af0609d02443ac9db467bf` |
| deckcraft-cli | origin/main `6431460e8023c9ccb56c9857992ce95de31abb0b` |
| Corpus | craft-corpus manifests `office-poi`, `office-libreoffice`, `office-openxml-sdk` (pins in `crates/corpus/manifests`) |

Each app was built `--release` from a detached worktree at the revision above (one build at a time; target dir and worktree deleted after).
Our export is `<app>-cli convert IN OUT.pdf`. Rasters: `pdftoppm -r 72`, first 10 pages. Text: `pdftotext -layout` (word F1, reading order) and `-bbox-layout` (line matching, median |dy| in points).

## Sample method

Deterministic: `office-render-oracle sample --seed 194` (splitmix64; candidates sorted by source and path, then shuffled per format). Quotas: 150 docx, 100 xlsx, 100 pptx
(350 files). Candidates: tagged `ooxml`, not `encrypted`/`not-zip`, 0 < size <= 8 MiB, present on disk. Strata are the manifest tags
tables, charts, rtl, fields, math, tracked-changes, images, large, plus "none" (no focus tag). Each stratum gets ceil(quota/9) files, rarest stratum first; a stratum with fewer files
than its quota is topped up from the remaining pool ("fill"). Tags are corpus heuristics, so a tag is a coverage hint. The sample is
committed as `tools/office-render-oracle/baseline/sample.json` (with per-file stratum) and per-file results as `baseline/results.jsonl`.
Source mix: office-libreoffice 246, office-poi 56, office-openxml-sdk 48 (LibreOffice's own QA files dominate the pools). rtl, math and tracked-changes
are thin for xlsx and pptx (the corpus has few such files), so those rows have n of 1 to 8; read them as anecdotes.

"Agree" thresholds: text = word F1 >= 0.90; visual = mean SSIM >= 0.80 over compared pages (72 dpi pages are mostly white, so SSIM is generous: a blank page against a one-line page still scores high); layout =
at least 80% of LibreOffice's text lines matched with identical text by the LCS matcher; pages = equal page count. Page images that differ in size by more than 2 px are padded with white to a common size.

## Summary

- Both sides rendered all 350 files: zero failures, zero timeouts, no panics. Our export takes about 46 ms median; LibreOffice 0.5 to 0.7 s.
- Visual agreement is high: 89% / 89% / 95% of files (wordcraft / gridcraft / deckcraft) have SSIM >= 0.8; median SSIM about 0.97 for all three.
- Text and layout agreement differ by app: text agree 73% / 56% / 77%; layout agree 54% / 52% / 56%; page count equal 96% / 68% / 100%.
- Weakest areas: charts in every app (median F1 0.90 / 0.73 / 0.40), gridcraft pagination (page counts equal for 68%; 10 of 100 files have fewer than half LibreOffice's pages) and files with images or drawings (27% page-count agreement), deckcraft SmartArt (6 of 10 SmartArt files export with no text), wordcraft math (median F1 0.60).
- Part of the text disagreement is a measurement artefact, not an app defect: LibreOffice writes some text (text effects, WordArt, OLE previews) as outlines that pdftotext cannot extract, and equations and chart labels are split into glyph runs differently.

## Baselines a CI job could gate on

`tools/office-render-oracle/baseline/baseline.json` holds, per app, floors set 5 percentage points (rates) or 0.05 (medians) below the value measured on this sample, so noise cannot fail the gate but a real regression can. A CI job would rebuild the three CLIs at the PR revision, run `run` on `baseline/sample.json`, `report`, and compare to `baseline.json`.
Caveats: the numbers depend on LibreOffice, poppler and installed fonts (pin them; the floors are for the versions above), and the corpus must be fetched with `craft-corpus fetch`. For a swap PR, the better gate is per file: any file whose SSIM or F1 drops by more than 0.05 against its row in `baseline/results.jsonl`.

| App | ok rate | text agree | visual agree | layout agree | pages equal | median F1 | median SSIM |
|---|---|---|---|---|---|---|---|
| wordcraft | >= 95% | >= 68% | >= 84% | >= 49% | >= 91% | >= 0.95 | >= 0.91 |
| gridcraft | >= 95% | >= 51% | >= 84% | >= 47% | >= 63% | >= 0.88 | >= 0.91 |
| deckcraft | >= 95% | >= 72% | >= 90% | >= 51% | >= 95% | >= 0.95 | >= 0.92 |

## Worst-case triage (10 per app, 30 files)

Selection: lowest 0.5*word F1 + 0.5*mean SSIM among ok files; the page shown is the page with the lowest SSIM. Side-by-side PNGs (LibreOffice | ours | difference) are in the scratchpad, not the repo:
`/private/tmp/claude-501/-Users-emm--t3-scratch-2026-11-03-check-out-the-artcraft-family-e0dad6b8/4624ffcc-66a8-43e4-b84a-d002385bf896/scratchpad/lo-oracle/triage/` (regenerate with `triage`).
28 of 30 were viewed; deckcraft 05 and 10 are SmartArt files with the same signature (our PDF has 0 words) as 02, 03, 04 and 07, and were classified by that pattern.

Classes: **ours** = our export is wrong; **amb** = ambiguous, a convention difference or a measurement artefact; **theirs** = LibreOffice is wrong.

### wordcraft

| # | File (corpus path) | F1 / SSIM | Class | Reason |
|---|---|---|---|---|
| 1 | O14ISOStrict/Graphics/AllShadows-O12-Word-OartTextEffects.docx | 0.00 / 0.81 | ours | LibreOffice draws the outline text effect; we draw plain text. The F1 of 0 is the extraction artefact, the visual difference is real |
| 2 | chart2/qa/extras/data/docx/3d-bar-label.docx | 0.06 / 0.89 | ours | We print a placeholder "3-D bar chart (not supported yet)" where LibreOffice draws the 3-D chart |
| 3 | sw/qa/extras/layout/data/tdf134866.docx | 0.00 / 0.98 | ours | Pie chart custom data label "Cusom label 100%" is missing in our output |
| 4 | sw/qa/extras/ooxmlexport/data/tdf115212.docx | 0.00 / 0.98 | ours | Page is blank; LibreOffice shows the TOC whose entries are tracked changes |
| 5 | sw/qa/extras/ooxmlexport/data/tdf130907.docx | 0.00 / 0.99 | amb | Equations render alike; math glyph extraction differs (15 vs 10 words); lines sit a few px apart |
| 6 | sw/qa/core/doc/data/ins-then-del.docx | 0.00 / 1.00 | amb | LibreOffice shows markup (AAA BBB CCC coloured); we show the final text AAACCC. Display-mode convention |
| 7 | xmloff/qa/unit/data/del-then-format.docx | 0.00 / 1.00 | ours | Page is blank; AAA and CCC should be visible in any view |
| 8 | sw/qa/core/undo/data/textbox-cut-save.docx | 0.00 / 1.00 | ours | Captions "Figure 1" and "Figure 2" (text box text) are missing; the images match |
| 9 | sw/qa/extras/ooxmlexport/data/fdo81381.docx | 0.00 / 1.00 | amb | Embedded OLE icon and caption look identical; LibreOffice's caption is not extractable as text |
| 10 | sw/qa/extras/ooxmlexport/data/math-matrix.docx | 0.00 / 1.00 | amb | Matrix renders alike; word counts differ through glyph run splitting |

### gridcraft

| # | File | F1 / SSIM / pages ref:ours | Class | Reason |
|---|---|---|---|---|
| 1 | sc/qa/unit/data/xlsx/forum-mso-en4-134670.xlsx | 0.02 / 0.62 / 87:3 | ours | Table cell text and fills are missing (borders only) and most pages are absent |
| 2 | OpenXmlTests v2FxTestFiles/spreadsheet/Connection.xlsx | 0.00 / 0.70 / 9:9 | ours | First-column text and arrow icons are missing; our cells show #N/A where LibreOffice shows cached #NAME?. Both error out, but dropping the text is ours |
| 3 | test-data/spreadsheet/LIBRE_OFFICE-116306-0.xlsx (pivot) | 0.00 / 0.73 / 3842:4 | amb | LibreOffice paginates a huge sheet into 3842 pages and shows another sheet first; we show the pivot first in 4 pages. Page order and print areas differ |
| 4 | test-data/spreadsheet/FillWithoutColor.xlsx | 0.03 / 0.82 / 12:6 | amb | Same tables; page splitting differs, and LibreOffice's fills spill past the print width |
| 5 | O14ISOStrict/Excel/Photo Formats - CGM-O12-XL-Pictures.xlsx | 0.03 / 0.94 / 18:1 | ours | Embedded pictures are not drawn; only a cell text remains |
| 6 | test-data/spreadsheet/56557.xlsx | 0.00 / 0.98 / 1:1 | ours | The chart-only sheet comes out blank; LibreOffice draws the chart frame and title |
| 7 | sc/qa/unit/data/xlsx/tdf111980_radioButtons.xlsx | 0.00 / 0.98 / 2:1 | ours | Form-control and ActiveX radio buttons are not drawn |
| 8 | test-data/spreadsheet/58325_db.xlsx | 0.00 / 0.99 / 4:1 | ours | Embedded OLE object (icon and caption) is not drawn |
| 9 | OpenXmlTests TestFiles/Spreadsheet.xlsx | 0.09 / 0.94 / 4:2 | ours | The chart and the picture are missing; only two small shapes remain |
| 10 | test-data/spreadsheet/62629_toMerge.xlsx | 0.07 / 0.97 / 2:8 | amb | The table is on LibreOffice page 2; ours splits the sheet over 8 pages and page 2 holds only the footer. Pagination convention |

### deckcraft

| # | File | F1 / SSIM | Class | Reason |
|---|---|---|---|---|
| 1 | chart2/qa/extras/data/pptx/tdf122765.pptx | 0.00 / 0.81 | ours | Pie chart data labels (percentages, names) are missing; LibreOffice also lets the pie run off the slide edge, a minor theirs defect |
| 2 | sd/qa/unit/data/pptx/table-list.pptx | 0.00 / 0.85 | ours | Slide is blank; SmartArt is not rendered |
| 3 | sd/qa/unit/data/pptx/smartart-composite-infer-right.pptx | 0.00 / 0.87 | ours | SmartArt not rendered, blank slide |
| 4 | sd/qa/unit/data/pptx/smartart-org-chart2.pptx | 0.00 / 0.87 | ours | SmartArt not rendered, blank slide |
| 5 | sd/qa/unit/data/pptx/smartart-vertical-block-list.pptx | 0.00 / 0.90 | ours | Same SmartArt pattern (not individually viewed) |
| 6 | chart2/qa/extras/data/pptx/chart.pptx | 0.00 / 0.90 | amb | Chart looks alike (colours, axes, legend); our chart text is not extractable, which is worth a look (search, accessibility) but is not a rendering error |
| 7 | sd/qa/unit/data/pptx/smartart-snake-rows.pptx | 0.00 / 0.91 | ours | SmartArt not rendered, blank slide |
| 8 | oox/qa/unit/data/tdf125085_WordArtFontText.pptx | 0.00 / 0.93 | amb | Hebrew WordArt renders alike; LibreOffice emits it as outlines (0 extractable words) |
| 9 | chart2/qa/xshape/data/pptx/tdf149204.pptx | 0.00 / 0.94 | ours | 3-D bar chart is drawn flat; bar layout differs; labels present |
| 10 | sd/qa/unit/data/pptx/vertical-bracket-list.pptx | 0.00 / 0.94 | ours | Same SmartArt pattern (not individually viewed) |

Totals: wordcraft ours 6, amb 4; gridcraft ours 7, amb 3; deckcraft ours 8, amb 2; theirs 0 (one minor theirs note on deckcraft 1).

## Findings: clear ours-wrong defects (not fixed here)

Each has a corpus file that reproduces it with `office-render-oracle one FILE`.

| ID | App | Defect | Reproducer | Sample prevalence |
|---|---|---|---|---|
| F1 | deckcraft | SmartArt (drawing fallback) is not exported to PDF; slide is blank | smartart-org-chart2.pptx, table-list.pptx, smartart-snake-rows.pptx | 6 of 10 SmartArt-tagged files export with no text; 12 of 100 pptx have zero words where LibreOffice has some |
| F2 | gridcraft | Pictures, charts, OLE objects and form controls are not drawn (cell text only) | Photo Formats - CGM-O12-XL-Pictures.xlsx, 56557.xlsx, 58325_db.xlsx, tdf111980_radioButtons.xlsx, TestFiles/Spreadsheet.xlsx | images tag: 27% page-count agreement, 47% text agree |
| F3 | gridcraft | Cell text and fills missing on some sheets (an 87-page sheet exports 3 pages) | forum-mso-en4-134670.xlsx, Connection.xlsx (first column text) | 10 of 100 files have fewer than half LibreOffice's pages (some of these are pagination, see F8) |
| F4 | wordcraft | Tracked-change content disappears: blank page for TOC entries that are tracked changes, and for delete-then-format | tdf115212.docx, del-then-format.docx | 2 of 18 tracked-changes files export with no text |
| F5 | wordcraft | Text box / frame text dropped | textbox-cut-save.docx | 4 of 150 docx export with no text where LibreOffice has some (includes F4) |
| F6 | wordcraft, deckcraft | Charts: 3-D charts unsupported (placeholder text in docx, flat in pptx); custom data labels and pie percentages missing | 3d-bar-label.docx, tdf134866.docx, tdf122765.pptx, tdf149204.pptx | charts-tag median F1: wordcraft 0.90, gridcraft 0.73, deckcraft 0.40; 5 of 15 deckcraft chart files with no text |
| F7 | wordcraft | OOXML text effects (outline, shadow) rendered as plain text | AllShadows-O12-Word-OartTextEffects.docx | one file seen |
| F8 | gridcraft | Pagination: page count equal for only 68% of files, and default page size differs for sheets without page setup (LibreOffice Letter portrait, ours A4, landscape in some). Classified ambiguous in triage because the default is locale-dependent, but 22 of 100 files give us fewer pages | FillWithoutColor.xlsx, 62629_toMerge.xlsx, xlsx-030/072/093 in `baseline/results.jsonl` | pages equal 68% |

Not defects: our PDFs carry no extractable text for some chart labels (deckcraft chart.pptx), which hurts search and accessibility.

## Reproduce

```text
cd tools/office-render-oracle
cargo run --release -- run --sample baseline/sample.json --work $W --bin-dir $BINS --out results.jsonl
cargo run --release -- report --results results.jsonl --baseline baseline.json
```

`soffice`, `pdftoppm`, `pdftotext` on PATH; `--bin-dir` holds wordcraft-cli, gridcraft-cli, deckcraft-cli. Paths in `sample.json` are absolute (`/Users/emm/ArtCraft/corpus/...`);
re-run `sample` on another machine to rebase them. A full run takes about 3 minutes.

## Full results

#### Outcomes

| App | Sampled | ok | ours failed | reference failed | both failed |
|---|---|---|---|---|---|
| wordcraft | 150 | 150 | 0 | 0 | 0 |
| gridcraft | 100 | 100 | 0 | 0 | 0 |
| deckcraft | 100 | 100 | 0 | 0 | 0 |

Agreement thresholds: text = word F1 >= 0.9; visual = mean SSIM >= 0.8; layout = matched-line % >= 80; pages = equal page count. Only files both sides rendered (status ok) enter the tables below.

#### Distributions (ok files)

| App | Metric | n | p10 | p25 | median | p75 | p90 |
|---|---|---|---|---|---|---|---|
| wordcraft | word F1 | 150 | 0.358 | 0.857 | 1.000 | 1.000 | 1.000 |
| wordcraft | reading order | 150 | 0.218 | 0.750 | 1.000 | 1.000 | 1.000 |
| wordcraft | SSIM mean | 150 | 0.787 | 0.896 | 0.969 | 0.994 | 0.999 |
| wordcraft | SSIM min page | 150 | 0.756 | 0.889 | 0.967 | 0.994 | 0.999 |
| wordcraft | DeltaE mean | 150 | 0.020 | 0.075 | 0.455 | 1.691 | 4.096 |
| wordcraft | p99 abs err | 150 | 0.000 | 0.000 | 0.043 | 0.602 | 0.859 |
| wordcraft | layout match % | 150 | 0.000 | 23.810 | 87.500 | 100.000 | 100.000 |
| wordcraft | median dy (pt) | 150 | 0.000 | 0.000 | 2.529 | 5.048 | 25.868 |
| gridcraft | word F1 | 100 | 0.110 | 0.629 | 0.939 | 1.000 | 1.000 |
| gridcraft | reading order | 100 | 0.058 | 0.488 | 0.889 | 1.000 | 1.000 |
| gridcraft | SSIM mean | 100 | 0.795 | 0.912 | 0.969 | 0.989 | 0.995 |
| gridcraft | SSIM min page | 100 | 0.663 | 0.890 | 0.952 | 0.988 | 0.995 |
| gridcraft | DeltaE mean | 100 | 0.045 | 0.132 | 0.446 | 1.826 | 3.813 |
| gridcraft | p99 abs err | 100 | 0.000 | 0.000 | 0.129 | 0.516 | 0.827 |
| gridcraft | layout match % | 100 | 5.263 | 40.000 | 81.818 | 100.000 | 100.000 |
| gridcraft | median dy (pt) | 100 | 0.000 | 0.243 | 2.002 | 11.373 | 44.752 |
| deckcraft | word F1 | 100 | 0.000 | 0.947 | 1.000 | 1.000 | 1.000 |
| deckcraft | reading order | 100 | 0.000 | 0.857 | 1.000 | 1.000 | 1.000 |
| deckcraft | SSIM mean | 100 | 0.831 | 0.893 | 0.972 | 0.989 | 0.996 |
| deckcraft | SSIM min page | 100 | 0.814 | 0.889 | 0.971 | 0.989 | 0.996 |
| deckcraft | DeltaE mean | 100 | 0.229 | 0.379 | 0.800 | 2.732 | 5.205 |
| deckcraft | p99 abs err | 100 | 0.004 | 0.004 | 0.129 | 0.702 | 0.949 |
| deckcraft | layout match % | 100 | 0.000 | 25.000 | 100.000 | 100.000 | 100.000 |
| deckcraft | median dy (pt) | 100 | 0.000 | 0.000 | 0.086 | 2.178 | 7.141 |

#### Agreement rates per app (ok files)

| App | n | text agree | visual agree | layout agree | page count equal | all four |
|---|---|---|---|---|---|---|
| wordcraft | 150 | 73.3% | 89.3% | 54.0% | 96.0% | 48.0% |
| gridcraft | 100 | 56.0% | 89.0% | 52.0% | 68.0% | 40.0% |
| deckcraft | 100 | 77.0% | 95.0% | 56.0% | 100.0% | 56.0% |

#### wordcraft per feature tag

A file counts under every tag it carries. `ours fail` counts files where our export errored but LibreOffice rendered.

| Tag | sampled | ok | ours fail | median F1 | median SSIM | text agree | visual agree | layout agree | pages equal |
|---|---|---|---|---|---|---|---|---|---|
| tables | 34 | 34 | 0 | 1.000 | 0.913 | 88% | 79% | 62% | 85% |
| charts | 22 | 22 | 0 | 0.896 | 0.900 | 45% | 86% | 36% | 91% |
| rtl | 20 | 20 | 0 | 1.000 | 0.994 | 85% | 95% | 60% | 100% |
| fields | 27 | 27 | 0 | 1.000 | 0.969 | 78% | 85% | 52% | 93% |
| math | 17 | 17 | 0 | 0.603 | 0.993 | 35% | 100% | 12% | 100% |
| tracked-changes | 18 | 18 | 0 | 0.903 | 0.980 | 50% | 89% | 33% | 94% |
| images | 29 | 29 | 0 | 1.000 | 0.949 | 72% | 83% | 41% | 93% |
| large | 2 | 2 | 0 | 1.000 | 0.997 | 100% | 50% | 50% | 50% |
| (no focus tag) | 29 | 29 | 0 | 1.000 | 0.973 | 97% | 93% | 79% | 100% |
| all | 150 | 150 | 0 | 1.000 | 0.969 | 73% | 89% | 54% | 96% |

#### gridcraft per feature tag

A file counts under every tag it carries. `ours fail` counts files where our export errored but LibreOffice rendered.

| Tag | sampled | ok | ours fail | median F1 | median SSIM | text agree | visual agree | layout agree | pages equal |
|---|---|---|---|---|---|---|---|---|---|
| tables | 23 | 23 | 0 | 0.956 | 0.944 | 61% | 91% | 61% | 70% |
| charts | 23 | 23 | 0 | 0.728 | 0.935 | 17% | 96% | 13% | 78% |
| rtl | 3 | 3 | 0 | 0.593 | 0.901 | 0% | 67% | 0% | 67% |
| fields | 12 | 12 | 0 | 0.969 | 0.923 | 75% | 100% | 58% | 75% |
| tracked-changes | 3 | 3 | 0 | 1.000 | 0.996 | 100% | 100% | 100% | 100% |
| images | 15 | 15 | 0 | 0.833 | 0.942 | 47% | 80% | 53% | 27% |
| large | 3 | 3 | 0 | 0.002 | 0.732 | 0% | 33% | 0% | 0% |
| (no focus tag) | 40 | 40 | 0 | 1.000 | 0.986 | 70% | 88% | 60% | 68% |
| all | 100 | 100 | 0 | 0.939 | 0.969 | 56% | 89% | 52% | 68% |

#### deckcraft per feature tag

A file counts under every tag it carries. `ours fail` counts files where our export errored but LibreOffice rendered.

| Tag | sampled | ok | ours fail | median F1 | median SSIM | text agree | visual agree | layout agree | pages equal |
|---|---|---|---|---|---|---|---|---|---|
| tables | 15 | 15 | 0 | 1.000 | 0.959 | 80% | 87% | 73% | 100% |
| charts | 15 | 15 | 0 | 0.395 | 0.874 | 33% | 80% | 7% | 100% |
| rtl | 8 | 8 | 0 | 1.000 | 0.995 | 62% | 100% | 38% | 100% |
| fields | 3 | 3 | 0 | 1.000 | 0.998 | 100% | 100% | 100% | 100% |
| math | 1 | 1 | 0 | 1.000 | 0.988 | 100% | 100% | 100% | 100% |
| images | 25 | 25 | 0 | 1.000 | 0.944 | 84% | 92% | 64% | 100% |
| large | 5 | 5 | 0 | 0.978 | 0.826 | 80% | 80% | 60% | 100% |
| (no focus tag) | 47 | 47 | 0 | 1.000 | 0.978 | 85% | 98% | 60% | 100% |
| all | 100 | 100 | 0 | 1.000 | 0.972 | 77% | 95% | 56% | 100% |

#### Failure reasons (ours failed, reference rendered)

| App | Reason | Files |
|---|---|---|
