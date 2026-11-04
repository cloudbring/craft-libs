# Container oracle: craft-archive zip swap (#120) and craft-cfb swap (#123)

Ticket: cloudbring/artcraft-algorithms#186 (V2.3). Evidence standard: `research/VERIFICATION.md` section 1.1
(old and new in one test binary, over a pinned corpus, every difference identical / within tolerance /
registered with evidence, anything else a regression). Harness: `tools/container-oracle/` (its own
`[workspace]`), registry: `tools/container-oracle/expected.toml`. Run on 2026-11-04.

## Result in one paragraph

Over 5,831 corpus files (4,446 zip archives, 1,355 compound files, 30 files that are neither) in 19
scenarios (11 zip, 8 compound-file) and 15,372 generated hostile inputs in 20 more, every input ends as
identical, within tolerance, a registered difference, or rejected by both sides: **0 regressions, 0 panics,
0 timeouts, 0 stale registry entries**. Getting there found **six real swap bugs** (five in `craft-cfb`, one in
`craft-archive`), fixed here with red-then-green tests; before the fixes 27 corpus compound files that the
old gridcraft reader opened were rejected by `craft-cfb`. 224 registry entries remain; each names a root
cause below. The level this raises C31 and C32 to is **L3** (corpus differential with zero unexplained
differences); neither reaches L4 (no independent oracle for the zip *writers*; the compound-file reference is
the crates.io `cfb` crate, which disagrees with all three old readers on many damaged files).

## What was compared

OLD is each app's own reader or writer at the swap's merge-base; NEW is the swap branch's adapter over the
workspace `craft-archive` / `craft-cfb` (path dependencies on this checkout).

| app | OLD (repo, commit) | NEW (branch commit) | how linked |
|---|---|---|---|
| pdfcraft zip writer | storytold/pdfcraft `3936f93a1479366a951206fc6d3094680fe2d3dc`, `crates/export/src/zip.rs` | cloudbring/pdfcraft `eefdea880bcbf2c6e762e6e1c850bd9a3b22becc`, same path | vendored under `old/pdfcraft/`, `new/pdfcraft/` (a private module of a heavy crate) |
| effectcraft lottie zip (writer + stored-only reader) | storytold/effectcraft `7870b5916ae0749d497d880989683bf2946d6c8b`, `crates/lottie/src/zip.rs` | `e38f5598e624db5d4090f71a807c30f3251038f0` | vendored (the crate pulls nine effectcraft crates and kurbo) |
| vectorcraft fileio zip (writer + reader) | storytold/vectorcraft `11862f61198c8f62c0b22b6c9a331cc5950ac98d`, `crates/engine/src/cmd/fileio/zip.rs` | `adcf7faf54ea3f75ffda528bcb867404a92571c9` | vendored (the engine crate) |
| craft-doc `.pcraft` reader | cloudbring/craft-libs `77d3e6039224e2ae017701cb7051ab7abb1ee9a2` (parent of c3aa560 "Make craft-doc ZIP use craft-archive") | workspace `crates/doc` | git dependency `craft_doc_old` and path dependency |
| gridcraft CFB (`is_encrypted`, `Compound::root_stream`) | storytold/gridcraft `85e51e009c24fad47575f04d3c08f12342615d82`, `crates/xlsx/src/cfb.rs` | cloudbring/gridcraft `f0048bf34430f01ca049eeb359f2a0e8a1b3c14d` (`cfb.rs`; the `crypto.rs`/`xls.rs` call sites are re-created in `cfbside::gridcraft` from the diff) | vendored |
| deckcraft CFB (`is_encrypted`) | storytold/deckcraft `3e178d8f5a59c787a6a53a7ed0f052b3e4a1489f`, `crates/pptx/src/cfb.rs` | cloudbring/deckcraft `0bff70074fcd91e154235cd7792e16f80b987e9e` | vendored |
| filmcraft CFB reader (`CompoundFile`) | storytold/filmcraft `eb59ec09daaea519c9c8fffa404a594ae50d00e0`, crate `filmcraft-cfb` | cloudbring/filmcraft `40274e630ac76764daf690945e57b62731c3e00a` | git dependencies with `package` rename; the new one's `craft-cfb` is `[patch]`ed to this checkout |
| wordcraft CFB (docbin / docx crypt / xls) | storytold/wordcraft `b47079fc791595fd84bb499d64dcd3f0624ef35a`: crates.io `cfb` 0.15 behind `cfb_guard::check` | cloudbring/wordcraft `d3b702fb5110214efcc8afc7a042a71e0291611b` | old: `cfb_guard.rs` vendored + `cfb` 0.15; new: the call sites are re-created in `cfbside::wordcraft` from the diff (they are inline in the app) |

Vendored files carry a header with repository, commit and path; the only change is that the trailing
`#[cfg(test)]` items are dropped (they need the app's dev-dependencies). Where the new side is
"re-created from the diff" (wordcraft, gridcraft's stream call sites) the harness reproduces the swap's
logic; that is a transcription, not the app's code.

The independent reference for zip is a hand-written central-directory parser plus the `zip` 3.x crate (craft-archive
uses `zip` 9); for compound files it is the crates.io `cfb` 0.15 crate behind wordcraft's size guard
(scenario `reference-vs-craft-cfb`).

## Corpus

`/Users/emm/ArtCraft/corpus/{office-poi,office-openxml-sdk,office-libreoffice}` (sha256-verified, read-only), classified
by magic bytes rather than extension: **4,446 zip** (`PK`), **1,355 compound files** (`D0CF11E0A1B11AE1`), **30 neither**
(pre-OLE Word 1/2 and BIFF2-5 files and fuzzer output; listed in the run's `corpus.txt`). Extension counts differ from
the ticket's estimate for that reason: of the 1,379 `.doc/.xls/.ppt` files 1,345 are compound files, 5 are zip archives (3 `.xls`,
2 `.doc`) and 29 are neither (the 30 neither-files are 15 `.doc`, 12 `.xls`, 2 `.ppt`, 1 `.docx`); 6 `.docx`, 2 `.xlsx`, 1 `.docm`
and 1 `.pptx` are compound files (Office-encrypted packages). 6 zip files cannot be read by the independent reference (5 do not open:
truncated62886.docx, Divino_Revelado.pptx, deep-data.xlsx, an `.xlsb.xls`, ofz9934-1.docx; ofz4825-1.docx has no decodable entry); the
writer scenarios count them as "both reject".

Hostile inputs are generated in code (`src/hostile.rs`, nothing checked in): 986 zip cases around a 330-byte archive (truncations at 14 offsets, bad CRCs,
lying sizes, duplicate/traversal/absolute/backslash/drive/control/non-UTF-8 names, symlink, encryption flag, unsupported methods, ZIP64
sentinels, data descriptors, junk prefix and suffix, end-record comment containing a signature, wrong counts, 65,534-65,535-66,000 entries,
300 MiB deflate bombs with honest and understated sizes, a single-byte flip at every offset, every 4-byte header field set to
0 / 0xFFFFFFFF / 0x7FFFFFF0) and 11,594 compound-file cases (truncations, short tails, every header field set to 8 extremes on three base
files in v3/v4, sector-shift and version lies, per-byte flips over the first 4.6 KiB, directory-entry loops/kinds/sizes, FAT cycles and
bad links, a self-looping DIFAT, the 2,048-sector repeated-FAT file wordcraft's guard was written for, mini-stream cutoff 0/max). The same zip generators are run around a valid `.pcraft` bundle for craft-doc (2,792 cases).

## Scenarios

Each scenario runs the old and the new side on the same bytes (one thread each, 120 s timeout, panics caught) and judges the pair.
The judge folds every difference it finds into one metric (a sorted `+`-joined list of difference classes), so a registry entry names
exactly the classes it excuses and nothing else.

| scenario | compares |
|---|---|
| `pdfcraft-write`, `effectcraft-write`, `vectorcraft-write-store`, `vectorcraft-write-deflate` | each corpus archive's decoded entries are re-written by the old and the new writer; the output is read back by the independent reference: entry names and order, decoded bytes, CRC, sizes, method, UTF-8 flag, other flags, DOS time/date, attributes, versions; identical compressed bytes are reported separately (`archive_bytes`) |
| `vectorcraft-read`, `effectcraft-read` | old and new reader on the corpus archive itself: open/reject, `contains`, `read` of every entry name (and its upper-case form), bytes exact |
| `vectorcraft-read-legacy-store/-deflate`, `effectcraft-read-legacy-store` | the same readers on archives as the *pre-swap writers* produced them (what users' saved files look like) |
| `craftdoc-raw`, `craftdoc-wrapped` | `sniff`/`info`/`renditions`/`outline` of craft-doc old vs new on the corpus file, and on a valid `.pcraft` bundle carrying that file's first 16 entries as payload |
| `gridcraft-sniff`, `gridcraft-is-xls`, `gridcraft-streams`, `deckcraft-sniff`, `wordcraft-sniff`, `wordcraft-streams` | `is_encrypted`, `is_xls`, and every root stream's presence, size cap and bytes, as each app's call sites compute them |
| `filmcraft-tree` | the whole tree by path: kind, size, CLSID, creation/modification times, state bits, start sector, children, every stream's bytes, header |
| `reference-vs-craft-cfb` | the crates.io `cfb` crate against `craft-cfb`: every reachable entry by path, kind, size, bytes |

The `.hostile` suffix names the same scenarios run on the generated cases.

## Results

Outcome columns are craft-oracle's: *identical*; *within* (equal decoded content, only wording of an error or compressed bytes differ);
*expected* (a registered difference); *REGRESSION*; *both reject*; *only old/new fails*; timeout; panic.

### Counts per scenario

#### Corpus, zip scenarios

| scenario | files | identical | within | expected | REGRESSION | both reject | only old fails | only new fails | timeout | panic |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| pdfcraft-write | 4446 | 0 | 0 | 4440 | 0 | 6 | 0 | 0 | 0 | 0 |
| effectcraft-write | 4446 | 0 | 0 | 4440 | 0 | 6 | 0 | 0 | 0 | 0 |
| vectorcraft-write-store | 4446 | 0 | 0 | 4440 | 0 | 6 | 0 | 0 | 0 | 0 |
| vectorcraft-write-deflate | 4446 | 0 | 0 | 4440 | 0 | 6 | 0 | 0 | 0 | 0 |
| vectorcraft-read | 4446 | 4434 | 4 | 8 | 0 | 0 | 0 | 0 | 0 | 0 |
| effectcraft-read | 4446 | 18 | 0 | 4428 | 0 | 0 | 0 | 0 | 0 | 0 |
| vectorcraft-read-legacy-store | 4446 | 4437 | 0 | 3 | 0 | 6 | 0 | 0 | 0 | 0 |
| vectorcraft-read-legacy-deflate | 4446 | 4437 | 0 | 3 | 0 | 6 | 0 | 0 | 0 | 0 |
| effectcraft-read-legacy-store | 4446 | 4436 | 0 | 4 | 0 | 6 | 0 | 0 | 0 | 0 |
| craftdoc-raw | 4446 | 4446 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| craftdoc-wrapped | 4446 | 4437 | 0 | 3 | 0 | 6 | 0 | 0 | 0 | 0 |

#### Corpus, compound-file scenarios

| scenario | files | identical | within | expected | REGRESSION | both reject | only old fails | only new fails | timeout | panic |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| gridcraft-sniff | 1355 | 1355 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| gridcraft-is-xls | 1355 | 1355 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| gridcraft-streams | 1355 | 1297 | 54 | 4 | 0 | 0 | 0 | 0 | 0 | 0 |
| deckcraft-sniff | 1355 | 1355 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| wordcraft-sniff | 1355 | 1351 | 0 | 4 | 0 | 0 | 0 | 0 | 0 | 0 |
| wordcraft-streams | 1355 | 1287 | 49 | 19 | 0 | 0 | 0 | 0 | 0 | 0 |
| filmcraft-tree | 1355 | 252 | 37 | 1066 | 0 | 0 | 0 | 0 | 0 | 0 |
| reference-vs-craft-cfb | 1355 | 1284 | 52 | 19 | 0 | 0 | 0 | 0 | 0 | 0 |

#### Hostile, zip scenarios

| scenario | files | identical | within | expected | REGRESSION | both reject | only old fails | only new fails | timeout | panic |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| pdfcraft-write.hostile | 986 | 1 | 0 | 778 | 0 | 207 | 0 | 0 | 0 | 0 |
| effectcraft-write.hostile | 986 | 1 | 0 | 778 | 0 | 207 | 0 | 0 | 0 | 0 |
| vectorcraft-write-store.hostile | 986 | 1 | 1 | 777 | 0 | 207 | 0 | 0 | 0 | 0 |
| vectorcraft-write-deflate.hostile | 986 | 1 | 1 | 777 | 0 | 207 | 0 | 0 | 0 | 0 |
| vectorcraft-read.hostile | 986 | 680 | 134 | 172 | 0 | 0 | 0 | 0 | 0 | 0 |
| effectcraft-read.hostile | 986 | 52 | 0 | 934 | 0 | 0 | 0 | 0 | 0 | 0 |
| vectorcraft-read-legacy-store.hostile | 986 | 735 | 0 | 43 | 0 | 208 | 0 | 0 | 0 | 0 |
| vectorcraft-read-legacy-deflate.hostile | 986 | 735 | 0 | 43 | 0 | 208 | 0 | 0 | 0 | 0 |
| effectcraft-read-legacy-store.hostile | 986 | 734 | 0 | 45 | 0 | 207 | 0 | 0 | 0 | 0 |
| craftdoc-raw.hostile | 986 | 986 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| craftdoc-pcraft.hostile | 2792 | 750 | 1859 | 183 | 0 | 0 | 0 | 0 | 0 | 0 |

#### Hostile, compound-file scenarios

| scenario | files | identical | within | expected | REGRESSION | both reject | only old fails | only new fails | timeout | panic |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| gridcraft-sniff.hostile | 11594 | 11588 | 0 | 6 | 0 | 0 | 0 | 0 | 0 | 0 |
| gridcraft-is-xls.hostile | 11594 | 10036 | 0 | 1558 | 0 | 0 | 0 | 0 | 0 | 0 |
| gridcraft-streams.hostile | 11594 | 5212 | 4019 | 2363 | 0 | 0 | 0 | 0 | 0 | 0 |
| deckcraft-sniff.hostile | 11594 | 11588 | 0 | 6 | 0 | 0 | 0 | 0 | 0 | 0 |
| wordcraft-sniff.hostile | 11594 | 8875 | 0 | 2719 | 0 | 0 | 0 | 0 | 0 | 0 |
| wordcraft-streams.hostile | 11594 | 4816 | 2787 | 3991 | 0 | 0 | 0 | 0 | 0 | 0 |
| filmcraft-tree.hostile | 11594 | 0 | 642 | 10952 | 0 | 0 | 0 | 0 | 0 | 0 |
| reference-vs-craft-cfb.hostile | 11594 | 4821 | 2782 | 3991 | 0 | 0 | 0 | 0 | 0 | 0 |

### Counts per format (corpus)

#### Corpus zip: per extension, cells are identical+within / expected / both reject (REGRESSION is 0 everywhere)

| scenario | docx (2628) | xlsx (982) | pptx (778) | xlsm (29) | docm (20) | pptm (4) | xls (3) | doc (2) |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| pdfcraft-write | 0 / 2625 / 3 | 0 / 981 / 1 | 0 / 777 / 1 | 0 / 29 / 0 | 0 / 20 / 0 | 0 / 4 / 0 | 0 / 2 / 1 | 0 / 2 / 0 |
| effectcraft-write | 0 / 2625 / 3 | 0 / 981 / 1 | 0 / 777 / 1 | 0 / 29 / 0 | 0 / 20 / 0 | 0 / 4 / 0 | 0 / 2 / 1 | 0 / 2 / 0 |
| vectorcraft-write-store | 0 / 2625 / 3 | 0 / 981 / 1 | 0 / 777 / 1 | 0 / 29 / 0 | 0 / 20 / 0 | 0 / 4 / 0 | 0 / 2 / 1 | 0 / 2 / 0 |
| vectorcraft-write-deflate | 0 / 2625 / 3 | 0 / 981 / 1 | 0 / 777 / 1 | 0 / 29 / 0 | 0 / 20 / 0 | 0 / 4 / 0 | 0 / 2 / 1 | 0 / 2 / 0 |
| vectorcraft-read | 2625 / 3 / 0 | 977 / 5 / 0 | 778 / 0 / 0 | 29 / 0 / 0 | 20 / 0 / 0 | 4 / 0 / 0 | 3 / 0 / 0 | 2 / 0 / 0 |
| effectcraft-read | 9 / 2619 / 0 | 7 / 975 / 0 | 1 / 777 / 0 | 0 / 29 / 0 | 0 / 20 / 0 | 0 / 4 / 0 | 1 / 2 / 0 | 0 / 2 / 0 |
| vectorcraft-read-legacy-store | 2625 / 0 / 3 | 978 / 3 / 1 | 777 / 0 / 1 | 29 / 0 / 0 | 20 / 0 / 0 | 4 / 0 / 0 | 2 / 0 / 1 | 2 / 0 / 0 |
| vectorcraft-read-legacy-deflate | 2625 / 0 / 3 | 978 / 3 / 1 | 777 / 0 / 1 | 29 / 0 / 0 | 20 / 0 / 0 | 4 / 0 / 0 | 2 / 0 / 1 | 2 / 0 / 0 |
| effectcraft-read-legacy-store | 2624 / 1 / 3 | 978 / 3 / 1 | 777 / 0 / 1 | 29 / 0 / 0 | 20 / 0 / 0 | 4 / 0 / 0 | 2 / 0 / 1 | 2 / 0 / 0 |
| craftdoc-raw | 2628 / 0 / 0 | 982 / 0 / 0 | 778 / 0 / 0 | 29 / 0 / 0 | 20 / 0 / 0 | 4 / 0 / 0 | 3 / 0 / 0 | 2 / 0 / 0 |
| craftdoc-wrapped | 2625 / 0 / 3 | 978 / 3 / 1 | 777 / 0 / 1 | 29 / 0 / 0 | 20 / 0 / 0 | 4 / 0 / 0 | 2 / 0 / 1 | 2 / 0 / 0 |

#### Corpus cfb: per extension, cells are identical+within / expected / both reject (REGRESSION is 0 everywhere)

| scenario | xls (710) | doc (443) | ppt (192) | docx (6) | xlsx (2) | docm (1) | pptx (1) |
|---|---:|---:|---:|---:|---:|---:|---:|
| gridcraft-sniff | 710 / 0 / 0 | 443 / 0 / 0 | 192 / 0 / 0 | 6 / 0 / 0 | 2 / 0 / 0 | 1 / 0 / 0 | 1 / 0 / 0 |
| gridcraft-is-xls | 710 / 0 / 0 | 443 / 0 / 0 | 192 / 0 / 0 | 6 / 0 / 0 | 2 / 0 / 0 | 1 / 0 / 0 | 1 / 0 / 0 |
| gridcraft-streams | 710 / 0 / 0 | 442 / 1 / 0 | 189 / 3 / 0 | 6 / 0 / 0 | 2 / 0 / 0 | 1 / 0 / 0 | 1 / 0 / 0 |
| deckcraft-sniff | 710 / 0 / 0 | 443 / 0 / 0 | 192 / 0 / 0 | 6 / 0 / 0 | 2 / 0 / 0 | 1 / 0 / 0 | 1 / 0 / 0 |
| wordcraft-sniff | 710 / 0 / 0 | 443 / 0 / 0 | 192 / 0 / 0 | 3 / 3 / 0 | 2 / 0 / 0 | 1 / 0 / 0 | 0 / 1 / 0 |
| wordcraft-streams | 710 / 0 / 0 | 433 / 10 / 0 | 187 / 5 / 0 | 3 / 3 / 0 | 2 / 0 / 0 | 1 / 0 / 0 | 0 / 1 / 0 |
| filmcraft-tree | 253 / 457 / 0 | 24 / 419 / 0 | 12 / 180 / 0 | 0 / 6 / 0 | 0 / 2 / 0 | 0 / 1 / 0 | 0 / 1 / 0 |
| reference-vs-craft-cfb | 710 / 0 / 0 | 433 / 10 / 0 | 187 / 5 / 0 | 3 / 3 / 0 | 2 / 0 / 0 | 1 / 0 / 0 | 0 / 1 / 0 |

Every file of every extension is accounted for; the zero REGRESSION column is the claim. "Expected" is large for the writers and for
`filmcraft-tree` because those classes cover nearly every file (UTF-8 flag; entry order), see below.

## Bugs found and fixed (red then green)

Before the fixes `craft-cfb` rejected 27 corpus compound files that gridcraft's old reader (and in most cases filmcraft's and the crates.io `cfb`
crate) opened: eight real `.xls` files for which `is_xls` was false, and four Office-encrypted packages (plus tdf166030.ppt) that could not be
opened. Each class has a test in `crates/cfb/tests/containers.rs` that failed before the change (five of the six tests failed together on
`7d46713`; the sixth, for class 5, was written after the hostile header sweep exposed it) and passes after; `cargo test -p craft-cfb`: 20 passed.

| # | root cause in `craft-cfb` | corpus files | test |
|---|---|---|---|
| 1 | `parse_directory` validated the name of **every** used entry, including the root and stale entries no tree link reaches; 18 files carry a root name of length 2 with a non-NUL unit, or a path such as `Y:\Desktop`. Names are now decoded only for entries the walk reaches. | 18: test-fields.doc, n652364/n750255/n757118.doc, 43781/45776/54111/numbers/numbers2/numbers3/bug60345 x2 `.ppt`, 48180/51461/atp/FormatChoiceTests/SimpleWithImages-mac/moodle... `.xls` | `root_entry_name_is_not_validated`, `unreachable_directory_entries_are_not_validated` |
| 2 | A header whose first DIFAT sector is FREESECT (0xFFFFFFFF) instead of ENDOFCHAIN, with no DIFAT sectors (POI writes this), failed with "inconsistent DIFAT chain or FAT count". | 3: 56260.ppt, ppt_with_png_encrypted.ppt, WrongFormulaRecordType.xls | `a_free_sector_marker_in_place_of_a_difat_start_means_no_difat` |
| 3 | The bound `fat_count <= ceil(sectors / 128) + 1` (taken from wordcraft's guard, which protected the `cfb` crate's per-listed-sector allocation) rejected encrypted OOXML packages that list more FAT sectors than strictly needed (3 in a 57-sector file, 9 in 542). In `craft-cfb` every listed sector must be a distinct sector of the file, so the FAT cannot outgrow the file; the bound is now the sector count. | 5: bug53475-password-is-pass.docx, encrypted_pptx.pptx, Encrypted_MSO2010_abc.docx, Encrypted_MSO2013_abc.docx, tdf166030.ppt | `a_fat_list_longer_than_the_file_strictly_needs_is_accepted` |
| 4 | A mini-FAT count of 1 with a mini-FAT start of ENDOFCHAIN failed with "sector chain ends early", although the workbook is a regular stream. | 1: 46904.xls | `a_mini_fat_count_without_a_start_sector_is_ignored` |
| 5 | The mini FAT was read as exactly "count" sectors; every older reader follows the chain from its start sector. A header with count 0 (or 2) and a valid chain made mini streams unreadable. | 0 in the corpus; found by the hostile header sweep (header-062/064) | `the_mini_fat_is_read_from_its_chain_not_from_the_declared_count` |
| 6 | `craft-archive` `ZipGuard::read_entry_bounded` verified the CRC but not that the entry decoded to its declared size; the pre-swap vectorcraft and craft-doc readers refused such an entry (a stream shorter than the size the directory states, with a valid CRC, was returned as if sound). | 0 in the corpus; hostile `zip/size-overstated-deflated` | `an_entry_that_decodes_to_other_than_its_declared_size_is_rejected` (`crates/archive/src/tests.rs`; `cargo test -p craft-archive`: 21 passed) |

`craft-doc` and `craft-ooxml` tests pass with the archive change; `cargo clippy -p craft-cfb -p craft-archive --all-targets -- -D warnings` and
`cargo fmt` are clean.

## Intended differences (registered; every entry in `expected.toml`)

Counts are in the appendix. Corpus entries are per file except the classes that cover most of the corpus.

### Zip (#120)

| class | where | root cause | evidence |
|---|---|---|---|
| `utf8_flag` | all four writer scenarios: 4,437 of 4,446 files each | The old writers set general-purpose bit 11 (UTF-8 names) on every entry; craft-archive sets it only for non-ASCII names. Everything else (names, order, decoded bytes, CRC, sizes, method, DOS time/date, attributes, versions) is equal, since the registered metric is exactly this one class. | APPNOTE 4.4.4; #120 pdfcraft comment ("ASCII names no longer carry the UTF-8 flag") |
| names refused by the writers | writers: 3 corpus files (49609.xlsx, tdf131575.xlsx, tdf76115.xlsx) and 43 hostile name cases | `ZipWriter` validates names: backslash, `..`, absolute, drive, control characters, empty segments, over-long, duplicates. The 3 files store names such as `xl\\styles.xml`. | #184 / VERIFICATION C31 note ("Backslash names now rejected by ZipGuard"); `NameMode::NormalizeBackslash` is the opt-in |
| same 3 names refused by the readers | `vectorcraft-read(-legacy)`, `effectcraft-read-legacy-store`, `craftdoc-wrapped` | `ZipGuard` rejects a backslash in any name at open | same |
| duplicate names | `vectorcraft-read`: duplicate-filename.xlsx, ofz9934-1.docx | `ZipGuard` rejects colliding names; the old reader returned the first | #120 ("Reading now rejects unsafe/duplicate names") |
| deflated entries now read | `effectcraft-read`: 4,423 files | the old `read_stored` skipped deflated entries (and all of a data-descriptor archive); the new one reads them. The class is exactly `new_adds_entries`: every entry the old reader returned is returned with identical bytes. | #120 effectcraft comment |
| damaged archives return nothing | `effectcraft-read`: Divino_Revelado.pptx, an `.xlsb.xls`, forcepoint107.xlsx, ofz35597-1.pptx, LIBREOFFICE-DBDAZAEC.docx | not a zip, or entries with bad CRCs (python `zipfile.testzip` names the same entries). The old reader returned the stored bytes unchecked; the new one verifies CRCs and returns an empty list on any failure (all or nothing). | #120 |
| entry cap | `effectcraft-read-legacy-store`: fdo76316.docx (1,714 entries) | lottie's `MAX_ENTRIES` is 1,024 | `crates/lottie/src/zip.rs` |
| ZIP64 | `vectorcraft-read`: tdf82984_zip64XLSXImport.xlsx | the old reader refuses ZIP64; craft-archive reads it (every entry CRC-verified) | #120 vectorcraft comment |
| Unicode Path extra field | `vectorcraft-read`: unicode-path.docx | the file carries Info-ZIP Unicode Path fields (0x7075) that swap the names of `word/document.xml` and `zip-test-additional-file`; the zip crate honours them (APPNOTE 4.6.9), the old reader used the header names. A POI test built for exactly this. | APPNOTE 4.6.9 |
| CP437 names | `vectorcraft-read`: ofz4825-1.docx | invalid UTF-8 in a name: the zip crate decodes names without the UTF-8 flag as CP437, the old reader lossily as UTF-8; the entry is damaged either way | APPNOTE 4.4.17.1 |
| hostile zip, writers | `*-write.hostile` | as above (names) | |
| hostile zip, readers | `vectorcraft-read.hostile`, `effectcraft-read.hostile`, legacy variants | stricter: `ZipGuard` validates all names, sizes and the end record at open, CRCs and sizes per read; effectcraft's old reader trusted local headers and the entry count it scanned, so a damaged local header lost or garbled entries that the central directory describes correctly. More tolerant: junk before the first header, a zeroed directory offset, header bytes the central directory makes irrelevant. Error class changes: unsupported compression methods now report as `Damaged` in vectorcraft's adapter (was `Method`). | #120 |
| hostile zip, craft-doc | `craftdoc-pcraft.hostile` | same two directions on corrupted `.pcraft` bundles (see the registry reason for the named cases) | `crates/doc/tests/hostile.rs` |

### Compound files (#123)

| class | where | root cause | evidence |
|---|---|---|---|
| truncated directory sector | `gridcraft-streams`: cf5f6fde...ppt, hang-13.ppt, ofz14989-1.ppt, ofz57592-1.doc | `craft-cfb` keeps directory/FAT/mini-FAT sectors strict (only a stream's last sector may be short, #123 short-tail); gridcraft's old reader accepted a cut-short directory sector. The crates.io `cfb` crate and filmcraft's old reader reject all four. | `truncated_metadata_sectors_remain_strict`; decision recorded in `crates/cfb/README.md` |
| encrypted packages detected | `wordcraft-sniff`: 4 files | the old wordcraft guard refused them ("the DIFAT lists 3 FAT sectors in a file of 57 sectors"), so `is_encrypted` was false for real encrypted Office packages; fixed by bug 3 above | red test 3 |
| lenient where the crates.io reader is strict | `wordcraft-streams`, `reference-vs-craft-cfb`: 19 files each (47950_*.doc, hang-*, ofz21531-1.ppt, forcepoint50/92, tdf166030.ppt, the encrypted packages) | the `cfb` crate (behind wordcraft's guard) rejects truncated files whose FAT is larger than the file, FATs that point twice, mini-FAT references past the table; `craft-cfb` reads lazily and bounded and returns the same bytes as gridcraft's old reader, which opens all 19 (10 identical stream by stream; in the other 9 only the wording of an error on a stream both fail to read differs) | `regular_stream_allows_truncated_final_padding` family |
| filmcraft index shape | `filmcraft-tree`: 1,066 corpus files | `CompoundFile.entries` is in `craft-cfb` tree order and no longer lists directory slots the tree does not reach (952 files have free or stale slots); paths, kinds, sizes, CLSIDs, times, state bits, start sectors, child names and all stream bytes are identical. Callers obtain ids through `find` / `child` (`crates/cfb/README.md` of filmcraft), never by slot. | filmcraft `crates/cfb/README.md` |
| filmcraft reads more | `filmcraft-tree`: 47245_test.xls, 47251.xls, 50426.xls, 53984.xls, ex45698-22488.xls | old filmcraft failed ("sector 410 is outside the file") on streams whose last sector is cut short (unpadded writers); `craft-cfb` reads them (#123 short-tail follow-up a96f03c) | `regular_stream_allows_truncated_final_padding` |
| fuzzer files | `filmcraft-tree`: hang-3/4/5/6/8/9/11/15/17/19.ppt, hang-2.doc | LibreOffice "hang" fuzz files with inconsistent FAT counts or a mini-FAT cycle that filmcraft's old reader followed; gridcraft's old reader and the `cfb` crate also reject hang-11 and hang-15 | `hostile_sector_and_directory_inputs_are_rejected` |
| hostile compound files | all `.hostile` CFB scenarios | `craft-cfb` rejects what the spec forbids and the old readers tolerated: version/sector shift, byte order, mini cutoff other than 4096, FAT/DIFAT counts above the file or not matching the listed sectors, reserved markers in the FAT list, DIFAT loops, duplicate FAT sectors, chains that loop or end early, directory loops, stream entries with children, bad name lengths on reachable entries. It accepts, lazily, damage no requested stream touches. Stream-level disagreements are on mutated FAT entries and on a root size above 1 GiB. No byte mismatch between two readers that both return a stream exists anywhere (class `bytes` never occurs). | `[MS-CFB]` 2.2, 2.3, 2.6; `hostile_sector_and_directory_inputs_are_rejected` |

## Open items (not fixed here)

1. **Parser differential on an end record inside the archive comment** (zip, hostile `zip/eocd-comment-with-signature`; registered). The zip crate
   takes the last end-record signature in the file, so a comment that contains one hides the real directory (`a.txt` found by the old reader, absent
   in the new). craft-doc's old parser and vectorcraft's old reader chose the record whose comment ends the file. No corpus file triggers it; fixing it
   needs `ZipReader::open` to pre-validate the candidate and hand the zip crate a view that excludes the comment. Left for a follow-up ticket.
2. **vectorcraft `Archive::read` reopens the archive on every call** (the swap's adapter builds a `ZipReader`, which validates every entry, per read).
   Measured by `cargo run --release --example readcost` (open + 3 reads, median of 5): 10 entries 2 us vs 11 us; 100 entries 4 vs 48 us; 1,000 entries
   19 vs 436 us; 10,000 entries 179 us vs 4.6 ms; 65,534 entries 1.2 ms vs 31.6 ms (5x to 27x). No effect on behaviour; it matters only if an app reads
   many entries of a large archive. Keeping one `ZipReader` in `Archive` would remove it.
3. **`read_stored` is all-or-nothing** (effectcraft): one bad entry empties the result; the old reader returned what it could.
4. Four truncated-directory files that gridcraft's old reader opened are now refused (table above): a deliberate craft-cfb policy; if gridcraft must open
   such files the tolerance belongs behind an option.
5. **vectorcraft adapter reports unsupported compression methods as `Damaged`** (old: `Method(name, id)`; hostile `zip/method-1/6/9/12/14/93/95/99`,
   registered). The mapping in the swap branch's `read_error` expects the zip crate's `CompressionMethodNotSupported`, which zip 9 does not raise on read.
   It lives in vectorcraft, not in craft-archive; diagnostics only.
6. The registry has 224 entries because the hostile sweeps cross many scenarios; hostile entries are scenario-wide per metric. A new hostile case
   that lands in an existing metric is silently excused, so the case list in `src/hostile.rs` is part of the contract.

## Reproduce

```text
ARTCRAFT_CORPUS=/Users/emm/ArtCraft/corpus CONTAINER_ORACLE_OUT=/tmp/container-oracle \
  cargo test --release --manifest-path tools/container-oracle/Cargo.toml -- --include-ignored --nocapture
```

`hostile_inputs_old_and_new_agree_or_are_registered` needs no corpus and runs by default; the whole suite takes about 100 s in release mode. Output: `report.md`, `report.json`,
`details.txt` (every non-identical file with its outcome and metric tallies), `corpus.txt`. `CONTAINER_ORACLE_ONLY=gridcraft,filmcraft` runs only
scenarios whose name contains one of the words. Debug helpers: `cargo run --release --example probe -- <scenario> <file | hostile:<case>>` prints what
each side saw; `--example rejects <corpus>` lists compound files `craft-cfb` rejects that another reader opens. The harness never saves inputs (a
first version did and wrote 37 GB).

## Appendix: registered differences hit

#### Registered differences hit (corpus files, hostile cases)

| entry (per-file entries grouped) | corpus files | hostile cases |
|---|---:|---:|
| craftdoc-pcraft.hostile:info-err-variant-outline-err-variant-renditions-err-variant-sniff-err-variant | 0 | 23 |
| craftdoc-pcraft.hostile:info-old-err-new-ok-outline-old-err-new-ok | 0 | 32 |
| craftdoc-pcraft.hostile:info-old-err-new-ok-outline-old-err-new-ok-renditions-err-variant-sniff-err-variant | 0 | 48 |
| craftdoc-pcraft.hostile:info-old-err-new-ok-outline-old-err-new-ok-renditions-old-err-new-ok | 0 | 29 |
| craftdoc-pcraft.hostile:info-old-err-new-ok-outline-old-err-new-ok-renditions-old-err-new-ok-sniff-err-variant | 0 | 8 |
| craftdoc-pcraft.hostile:info-old-ok-new-err-outline-old-ok-new-err | 0 | 27 |
| craftdoc-pcraft.hostile:info-old-ok-new-err-outline-old-ok-new-err-renditions-old-ok-new-err | 0 | 16 |
| craftdoc-wrapped:info-old-ok-new-err-outline-old-ok-new-err-renditions-old-ok-new-err | 3 | 0 |
| deckcraft-sniff.hostile:sniff | 0 | 6 |
| effectcraft-read-legacy-store.hostile:new-rejects-archive | 0 | 45 |
| effectcraft-read-legacy-store:new-rejects-archive | 4 | 0 |
| effectcraft-read.hostile:bytes-new-adds-entries | 0 | 7 |
| effectcraft-read.hostile:new-adds-entries | 0 | 368 |
| effectcraft-read.hostile:new-adds-entries-old-entries-lost | 0 | 17 |
| effectcraft-read.hostile:new-rejects-archive | 0 | 540 |
| effectcraft-read.hostile:old-entries-lost | 0 | 2 |
| effectcraft-read:new-adds-entries | 4423 | 0 |
| effectcraft-read:new-rejects-archive | 5 | 0 |
| effectcraft-write.hostile:new-rejects-store-returned-an-empty-archive | 0 | 43 |
| effectcraft-write.hostile:utf8-flag | 0 | 735 |
| effectcraft-write:new-rejects-store-returned-an-empty-archive | 3 | 0 |
| effectcraft-write:utf8-flag | 4437 | 0 |
| filmcraft-tree.hostile:index-order | 0 | 1805 |
| filmcraft-tree.hostile:new-accepts-bad-sector-xfffffffc-in-a-chain | 0 | 1 |
| filmcraft-tree.hostile:new-accepts-bad-sector-xfffffffd-in-a-chain | 0 | 1 |
| filmcraft-tree.hostile:new-accepts-bad-sector-xffffffff-in-a-chain | 0 | 1 |
| filmcraft-tree.hostile:new-accepts-sector-is-outside-the-file | 0 | 19 |
| filmcraft-tree.hostile:new-accepts-stream-is-truncated | 0 | 10 |
| filmcraft-tree.hostile:new-rejects-difat-chain-loops-or-ends-early | 0 | 7 |
| filmcraft-tree.hostile:new-rejects-directory-loop-or-invalid-entry | 0 | 19 |
| filmcraft-tree.hostile:new-rejects-duplicate-fat-difat-sector | 0 | 1 |
| filmcraft-tree.hostile:new-rejects-fat-difat-count-exceeds-file | 0 | 46 |
| filmcraft-tree.hostile:new-rejects-inconsistent-difat-chain-or-fat-count | 0 | 4134 |
| filmcraft-tree.hostile:new-rejects-invalid-directory-name-length | 0 | 32 |
| filmcraft-tree.hostile:new-rejects-invalid-fat-sector-marker | 0 | 972 |
| filmcraft-tree.hostile:new-rejects-mini-fat-exceeds-file | 0 | 40 |
| filmcraft-tree.hostile:new-rejects-sector-chain-loops-or-uses-marker | 0 | 3 |
| filmcraft-tree.hostile:new-rejects-sector-outside-file | 0 | 1 |
| filmcraft-tree.hostile:new-rejects-stream-has-children | 0 | 17 |
| filmcraft-tree.hostile:new-rejects-unsupported-cfb-version-or-sector-size | 0 | 48 |
| filmcraft-tree.hostile:new-rejects-unsupported-mini-stream-cutoff | 0 | 62 |
| filmcraft-tree.hostile:old-err-new-ok-unreachable-entries-dropped | 0 | 22 |
| filmcraft-tree.hostile:old-ok-new-err | 0 | 12 |
| filmcraft-tree.hostile:old-ok-new-err-unreachable-entries-dropped | 0 | 11 |
| filmcraft-tree.hostile:unreachable-entries-dropped | 0 | 3688 |
| filmcraft-tree:index-order | 95 | 0 |
| filmcraft-tree:new-accepts-sector-is-outside-the-file | 2 | 0 |
| filmcraft-tree:new-rejects-inconsistent-difat-chain-or-fat-count | 11 | 0 |
| filmcraft-tree:old-err-new-ok-unreachable-entries-dropped | 5 | 0 |
| filmcraft-tree:old-ok-new-err-unreachable-entries-dropped | 1 | 0 |
| filmcraft-tree:unreachable-entries-dropped | 952 | 0 |
| gridcraft-is-xls.hostile:sniff | 0 | 1558 |
| gridcraft-sniff.hostile:sniff | 0 | 6 |
| gridcraft-streams.hostile:new-rejects-difat-chain-loops-or-ends-early | 0 | 7 |
| gridcraft-streams.hostile:new-rejects-directory-index-out-of-range | 0 | 17 |
| gridcraft-streams.hostile:new-rejects-directory-loop-or-invalid-entry | 0 | 43 |
| gridcraft-streams.hostile:new-rejects-duplicate-fat-difat-sector | 0 | 1 |
| gridcraft-streams.hostile:new-rejects-fat-difat-count-exceeds-file | 0 | 88 |
| gridcraft-streams.hostile:new-rejects-inconsistent-difat-chain-or-fat-count | 0 | 987 |
| gridcraft-streams.hostile:new-rejects-invalid-directory-name-length | 0 | 32 |
| gridcraft-streams.hostile:new-rejects-invalid-fat-sector-marker | 0 | 972 |
| gridcraft-streams.hostile:new-rejects-mini-fat-exceeds-file | 0 | 40 |
| gridcraft-streams.hostile:new-rejects-sector-chain-loops-or-uses-marker | 0 | 6 |
| gridcraft-streams.hostile:new-rejects-stream-has-children | 0 | 17 |
| gridcraft-streams.hostile:new-rejects-unsupported-byte-order | 0 | 30 |
| gridcraft-streams.hostile:new-rejects-unsupported-cfb-version-or-sector-size | 0 | 48 |
| gridcraft-streams.hostile:new-rejects-unsupported-mini-stream-cutoff | 0 | 62 |
| gridcraft-streams.hostile:old-err-new-ok | 0 | 8 |
| gridcraft-streams.hostile:old-ok-new-err | 0 | 5 |
| gridcraft-streams:new-rejects-sector-outside-file | 4 | 0 |
| pdfcraft-write.hostile:new-rejects-invalid-zip-entry-name | 0 | 43 |
| pdfcraft-write.hostile:utf8-flag | 0 | 735 |
| pdfcraft-write:new-rejects-invalid-zip-entry-name | 3 | 0 |
| pdfcraft-write:utf8-flag | 4437 | 0 |
| reference-vs-craft-cfb.hostile:new-accepts-found-reference-to-sector-but-fat-has-only-entries | 0 | 3 |
| reference-vs-craft-cfb.hostile:new-accepts-malformed-directory-entry-invalid-color | 0 | 1 |
| reference-vs-craft-cfb.hostile:new-accepts-malformed-directory-entry-name-length-too-large | 0 | 4 |
| reference-vs-craft-cfb.hostile:new-accepts-malformed-directory-entry-odd-name-length | 0 | 3 |
| reference-vs-craft-cfb.hostile:new-accepts-malformed-directory-left-sibling-index-is-but-directory-entry-count-is | 0 | 2 |
| reference-vs-craft-cfb.hostile:new-accepts-malformed-directory-name-ordering-root-entry-vs-root-entry | 0 | 6 |
| reference-vs-craft-cfb.hostile:new-accepts-malformed-directory-right-sibling-index-is-but-directory-entry-count-is | 0 | 1 |
| reference-vs-craft-cfb.hostile:new-accepts-malformed-directory-root-stream-len-is-but-should-be-multiple-of | 0 | 3 |
| reference-vs-craft-cfb.hostile:new-accepts-malformed-fat-fat-has-entries-but-file-has-only-sectors | 0 | 313 |
| reference-vs-craft-cfb.hostile:new-accepts-malformed-fat-sector-pointed-to-twice | 0 | 54 |
| reference-vs-craft-cfb.hostile:new-accepts-malformed-minifat-minifat-has-entries-but-mini-sector-points-to | 0 | 7 |
| reference-vs-craft-cfb.hostile:new-rejects-difat-chain-loops-or-ends-early | 0 | 7 |
| reference-vs-craft-cfb.hostile:new-rejects-fat-difat-count-exceeds-file | 0 | 88 |
| reference-vs-craft-cfb.hostile:new-rejects-inconsistent-difat-chain-or-fat-count | 0 | 2438 |
| reference-vs-craft-cfb.hostile:new-rejects-invalid-directory-name-length | 0 | 4 |
| reference-vs-craft-cfb.hostile:new-rejects-invalid-fat-sector-marker | 0 | 963 |
| reference-vs-craft-cfb.hostile:new-rejects-mini-fat-exceeds-file | 0 | 40 |
| reference-vs-craft-cfb.hostile:new-rejects-sector-missing-from-fat | 0 | 12 |
| reference-vs-craft-cfb.hostile:ref-err-craft-cfb-ok | 0 | 40 |
| reference-vs-craft-cfb.hostile:ref-ok-craft-cfb-err | 0 | 2 |
| reference-vs-craft-cfb:new-accepts-malformed-fat-fat-has-entries-but-file-has-only-sectors | 8 | 0 |
| reference-vs-craft-cfb:new-accepts-malformed-fat-sector-pointed-to-twice | 1 | 0 |
| reference-vs-craft-cfb:new-accepts-malformed-minifat-mini-sector-pointed-to-twice | 1 | 0 |
| reference-vs-craft-cfb:new-accepts-malformed-minifat-minifat-has-entries-but-mini-sector-points-to | 4 | 0 |
| reference-vs-craft-cfb:new-accepts-the-difat-lists-fat-sectors-in-a-file-of-sectors | 5 | 0 |
| vectorcraft-read-legacy-deflate.hostile:new-rejects-damaged | 0 | 43 |
| vectorcraft-read-legacy-deflate:new-rejects-damaged | 3 | 0 |
| vectorcraft-read-legacy-store.hostile:new-rejects-damaged | 0 | 43 |
| vectorcraft-read-legacy-store:new-rejects-damaged | 3 | 0 |
| vectorcraft-read.hostile:bytes-contains | 0 | 47 |
| vectorcraft-read.hostile:contains-old-err-new-ok | 0 | 5 |
| vectorcraft-read.hostile:err-variant | 0 | 25 |
| vectorcraft-read.hostile:new-accepts-damaged | 0 | 5 |
| vectorcraft-read.hostile:new-accepts-zip | 0 | 1 |
| vectorcraft-read.hostile:new-rejects-damaged | 0 | 74 |
| vectorcraft-read.hostile:new-rejects-toolarge | 0 | 6 |
| vectorcraft-read.hostile:old-err-new-ok | 0 | 9 |
| vectorcraft-read:bytes | 1 | 0 |
| vectorcraft-read:contains-old-err-new-ok | 1 | 0 |
| vectorcraft-read:new-rejects-damaged | 5 | 0 |
| vectorcraft-read:old-err-new-ok | 1 | 0 |
| vectorcraft-write-deflate.hostile:new-rejects-invalid-zip-entry-name | 0 | 43 |
| vectorcraft-write-deflate.hostile:utf8-flag | 0 | 734 |
| vectorcraft-write-deflate:new-rejects-invalid-zip-entry-name | 3 | 0 |
| vectorcraft-write-deflate:utf8-flag | 4437 | 0 |
| vectorcraft-write-store.hostile:new-rejects-invalid-zip-entry-name | 0 | 43 |
| vectorcraft-write-store.hostile:utf8-flag | 0 | 734 |
| vectorcraft-write-store:new-rejects-invalid-zip-entry-name | 3 | 0 |
| vectorcraft-write-store:utf8-flag | 4437 | 0 |
| wordcraft-sniff.hostile:sniff | 0 | 2719 |
| wordcraft-sniff:sniff | 4 | 0 |
| wordcraft-streams.hostile:new-accepts-found-reference-to-sector-but-fat-has-only-entries | 0 | 3 |
| wordcraft-streams.hostile:new-accepts-malformed-directory-entry-invalid-color | 0 | 1 |
| wordcraft-streams.hostile:new-accepts-malformed-directory-entry-name-length-too-large | 0 | 4 |
| wordcraft-streams.hostile:new-accepts-malformed-directory-entry-odd-name-length | 0 | 3 |
| wordcraft-streams.hostile:new-accepts-malformed-directory-left-sibling-index-is-but-directory-entry-count-is | 0 | 2 |
| wordcraft-streams.hostile:new-accepts-malformed-directory-name-ordering-root-entry-vs-root-entry | 0 | 6 |
| wordcraft-streams.hostile:new-accepts-malformed-directory-right-sibling-index-is-but-directory-entry-count-is | 0 | 1 |
| wordcraft-streams.hostile:new-accepts-malformed-directory-root-stream-len-is-but-should-be-multiple-of | 0 | 3 |
| wordcraft-streams.hostile:new-accepts-malformed-fat-fat-has-entries-but-file-has-only-sectors | 0 | 313 |
| wordcraft-streams.hostile:new-accepts-malformed-fat-sector-pointed-to-twice | 0 | 54 |
| wordcraft-streams.hostile:new-accepts-malformed-minifat-minifat-has-entries-but-mini-sector-points-to | 0 | 7 |
| wordcraft-streams.hostile:new-rejects-difat-chain-loops-or-ends-early | 0 | 7 |
| wordcraft-streams.hostile:new-rejects-fat-difat-count-exceeds-file | 0 | 88 |
| wordcraft-streams.hostile:new-rejects-inconsistent-difat-chain-or-fat-count | 0 | 2438 |
| wordcraft-streams.hostile:new-rejects-invalid-directory-name-length | 0 | 4 |
| wordcraft-streams.hostile:new-rejects-invalid-fat-sector-marker | 0 | 963 |
| wordcraft-streams.hostile:new-rejects-mini-fat-exceeds-file | 0 | 40 |
| wordcraft-streams.hostile:new-rejects-sector-missing-from-fat | 0 | 12 |
| wordcraft-streams.hostile:old-err-new-ok | 0 | 40 |
| wordcraft-streams.hostile:old-ok-new-err | 0 | 2 |
| wordcraft-streams:new-accepts-malformed-fat-fat-has-entries-but-file-has-only-sectors | 8 | 0 |
| wordcraft-streams:new-accepts-malformed-fat-sector-pointed-to-twice | 1 | 0 |
| wordcraft-streams:new-accepts-malformed-minifat-mini-sector-pointed-to-twice | 1 | 0 |
| wordcraft-streams:new-accepts-malformed-minifat-minifat-has-entries-but-mini-sector-points-to | 4 | 0 |
| wordcraft-streams:new-accepts-the-difat-lists-fat-sectors-in-a-file-of-sectors | 5 | 0 |

