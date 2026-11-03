# Ticket #124 — `craft-sheet` Extraction Survey & Design Report

## Decision: STOP (Design Report Only)

Per the ticket #124 stop rule:
> Proceed to extraction only if gridcraft's three readers can be split at the boundary found in step 1 such that the shared crate's output is sufficient for gridcraft to rebuild its model without re-parsing (i.e. the neutral model carries every field gridcraft currently extracts, or the missing fields are listed and ≤ 5 and additive). If gridcraft's parsing is interleaved with model construction so deeply that the split would be a rewrite, commit ONLY the design document (`crates/sheet/DESIGN.md` with the survey, the proposed model, and the per-reader split plan with line references) and report that.

GridCraft's readers construct `gridcraft_model::Workbook` and `gridcraft_model::Sheet` directly during streaming XML / BIFF8 record traversal, interleaving parsing with expression AST construction (`gridcraft_formula::parse` / `shift_relative`), dynamic-array and Python-cell spill resolution (`python_part::cell_code`, `record_python_spill`, `settle_python_spills_past_budget`), rich-data cell pictures (`vm` index lookup), conditional formatting with `x14` extension back-patching (`cf_ext_ids`), drawing/chart/OMML/ink/slicer/pivot part traversal, and unmodelled-part passthrough graph closure (`passthrough::KeptReader`). Furthermore, the neutral model specified in Step 1 leaves out **>30 fields** across `Workbook`, `Sheet`, `Cell`, and `Style` that GridCraft currently extracts (far exceeding the `≤ 5` additive threshold). Extracting a shared crate that satisfies GridCraft without re-parsing would require either moving almost all of `gridcraft-model` and `gridcraft-formula` into `craft-sheet` or rewriting GridCraft's readers into a two-pass architecture.

---

## 1. Format Survey

### Summary Table

| Format | GridCraft Reader (`gridcraft/crates/xlsx/src/`) | GridCraft Emits (`gridcraft-model`) | WordCraft Reader (`wordcraft/crates/formats/src/sheet/`) | WordCraft Needs | Parsing vs. Model Construction Boundary |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **XLS (BIFF8)** | `xls.rs` (880 lines)<br>`xls/records.rs` (223)<br>`xls/formula.rs` (551)<br>`xls/ftab.rs` (376)<br>`xls/styles.rs` (405) | `Workbook` (`sheets`, `names`, `styles`, `cell_styles`, `date_system`, `calc`, `active_sheet`), `Sheet` (`cells` with `Value`/`Formula` AST/`StyleId`, `cols`, `rows`, `default_col_width`, `default_row_height`, `merges`, `visibility`, `show_formulas`, `show_gridlines`, `show_headings`, `show_zeros`, `right_to_left`, `print.print_area`, `print.title_rows/cols`) | `xls.rs` (748 lines; lines 1–438 reader, 439–524 test fixture writer `build`) | Sheet names (`Vec<String>`) and per-sheet non-blank rows of formatted display strings (`Vec<Vec<String>>`), using `date1904` + XF `DateKind` to format serial numbers as ISO dates/times or General numbers | `xls/records.rs` (lines 14–160) & `xls.rs::globals` (lines 204–323) parse raw BIFF8 records into `Globals` (`BoundSheet`, `StyleRecords`, `sst`, `Links`, `Lbl`). Interleaving begins at `styles::build` (`xls/styles.rs:372–404`), `worksheet`/`sheet_record` (`xls.rs:536–800`), `formula_cell` (`xls.rs:802–844`), `xls/formula.rs::decode` (lines 1–551, builds `gridcraft_formula::Expr` AST), and `defined_names` (`xls.rs:846–880`). |
| **ODS** | `ods.rs` (660 lines) | `Workbook` (`sheets`, `styles` with derived `NumFmt` for dates/times/percentages), `Sheet` (`name`, `cells` with `Value` + `StyleId`, `merges`), `ReadReport` warnings | `ods.rs` (189 lines) | Sheet names (`Vec<String>`) and per-sheet rows of display strings (`Vec<Vec<String>>`), formatting `float`/`percentage`/`currency` via `number()`, `date`/`time` via `iso_datetime()`/`iso_time()`, `boolean` as `TRUE`/`FALSE`, and collapsing `<text:p>`/`<text:s>`/`<text:tab>`/`<text:line-break>` | `ods.rs::Import::parse` & `start`/`append`/`end` (lines 247–454) stream ODF XML into `SheetDraft`/`RowDraft`/`CellDraft`. Model construction begins at `finish_cell` & `convert` (`ods.rs:456–529`, interns `NumFmt` into `wb.styles` and builds `gridcraft_model::Cell`) and `finish_row` (`ods.rs:531–576`, populates `sheet.cells` and `sheet.merges`). |
| **XLSX** | `read.rs` (503 lines)<br>`sheet_read.rs` (787)<br>`sheet_read/parts.rs` (273)<br>`sheet_read/rules.rs` (181)<br>`styles.rs` (443 read)<br>`theme.rs` (254)<br>`fmla.rs` (474)<br>`python_part.rs` (336)<br>`richdata.rs` (368)<br>`drawing.rs` (661 + 247)<br>`chart.rs` (898)<br>`chartex.rs` (399)<br>`omml.rs` (932)<br>`pivot/read.rs` (385)<br>`passthrough/read.rs` (688)<br>`package.rs` (266)<br>`xml.rs` (750) | Full `Workbook` (`sheets`, `names`, `styles`, `cell_styles`, `theme`, `date_system`, `calc`, `show_objects`, `active_sheet`, `view`, `props`, `protected_structure`, `structure_password`, `phonetics`, `vba_project`, `preserved`) and full `Sheet` (all 34 fields including `cells`, `cell_pictures`, `spill`, `spill_ranges`, `cols`, `rows`, `merges`, `freeze`, `tab_color`, `visibility`, view flags, `cond_formats`, `validations`, `tables`, `autofilter`, `comments`, `hyperlinks`, `charts`, `images`, `shapes`, `sparklines`, `slicers`, `pivots`, `protection`, `edit_ranges`, `print`, `preserved`) | `xlsx.rs` (313 lines) | Sheet names (`Vec<String>`) and per-sheet rows of display strings (`Vec<Vec<String>>`), resolving `xl/sharedStrings.xml` (ignoring `<rPh>`), `xl/styles.xml` (`numFmt` + `cellXfs` → `DateKind`), `workbookPr@date1904`, and formatting `<c>` values as strings | `package.rs` (lines 1–266) and `xml.rs` (lines 1–750) provide ZIP/OPC and DOM/streaming XML primitives. Model construction is deeply interleaved across `read.rs::read_xlsx` (lines 240–453) and `sheet_read.rs::read_sheet` (lines 160–340), where streaming `<row>` callbacks (`read_row` lines 421–475, `read_cell` lines 477–592, `read_formula_text` lines 599–653) directly parse `gridcraft_formula::Expr`, shift shared formulas, resolve `PY()` scripts and dynamic array spills, and attach `CellPicture`s. |

---

## 2. Proposed Neutral Model (`craft-sheet`)

```rust
pub struct Limits {
    pub max_entry_bytes: u64,
    pub max_total_bytes: u64,
    pub max_sheets: usize,
    pub max_rows: u32,
    pub max_cols: u32,
    pub max_cells: usize,
    pub max_strings: usize,
    pub max_text_bytes: usize,
    pub max_merges: usize,
    pub max_styles: usize,
    pub max_records: usize,
    pub max_xml_depth: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum SheetError {
    #[error("unsupported or invalid spreadsheet format: {0}")]
    Format(String),
    #[error("spreadsheet is encrypted or password-protected")]
    Encrypted,
    #[error("XML parse error: {0}")]
    Xml(String),
    #[error("spreadsheet exceeds resource limits: {0}")]
    TooLarge(String),
    #[error("archive error: {0}")]
    Archive(#[from] craft_archive::ArchiveError),
    #[error("compound file error: {0}")]
    Cfb(#[from] craft_cfb::CfbError),
}

pub struct Workbook {
    pub sheets: Vec<Sheet>,
    pub styles: Vec<Style>,
    pub named_styles: Vec<(String, StyleId)>,
    pub names: Vec<DefinedName>,
    pub shared_strings: Vec<Arc<str>>,
    pub date1904: bool,
    pub warnings: Vec<String>,
}

pub struct Sheet {
    pub name: String,
    pub visibility: Visibility,
    pub cells: BTreeMap<(u32, u32), Cell>,
    pub merges: Vec<Range>,
    pub cols: BTreeMap<u32, LineMetrics>,
    pub rows: BTreeMap<u32, LineMetrics>,
    pub default_col_width: f32,
    pub default_row_height: f32,
}

pub struct Cell {
    pub value: Value,
    pub formula: Option<CellFormula>,
    pub style: Option<StyleId>,
}

pub enum Value {
    Empty,
    Bool(bool),
    Number(f64),
    Text(Arc<str>),
    Error(CellError),
}

pub struct CellFormula {
    pub text: String,
    pub array_range: Option<Range>,
    pub dynamic: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StyleId(pub u32);

pub struct Style {
    pub number_format: String,
    pub font: FontSummary,
    pub fill: FillSummary,
    pub borders: BorderSummary,
    pub alignment: AlignmentSummary,
    pub protection: ProtectionSummary,
}

pub struct DefinedName {
    pub name: String,
    pub scope: Option<usize>,
    pub formula: String,
    pub comment: String,
    pub hidden: bool,
}
```

---

## 3. Why the Stop Rule Triggers: Missing GridCraft Fields & Interleaving

### A. Fields Extracted by GridCraft's Readers Not Carried by the Neutral Model (>30 fields)

1. **Workbook-level fields (12 fields):**
   - `theme: Theme` (`read.rs:146–149`, `theme.rs:47–178`: 12 color slots, major/minor Latin/EA/CS fonts, per-script font mappings)
   - `calc: CalcSettings` (`read.rs:206–219`, `xls.rs:171, 697–706`: `mode`, `iterative`, `max_iterations`, `max_change`, `precision_as_displayed`)
   - `show_objects: ShowObjects` (`read.rs:199–203`)
   - `active_sheet: ActiveSheet` (`read.rs:285, 360`, `xls.rs:180`)
   - `view: WorkbookView` (`read.rs:286–295`: `show_sheet_tabs`, `show_horizontal_scroll`, `show_vertical_scroll`, `tab_ratio`)
   - `props: DocProps` (`read.rs:416–430`: `title`, `subject`, `author`, `keywords`, `description`, `company`)
   - `protected_structure: bool` & `structure_password: Option<PasswordHash>` (`read.rs:220–223`)
   - `cell_styles: Vec<(String, Style)>` (`read.rs:169`, `xls/styles.rs:384–402`)
   - `phonetics: Vec<PhoneticString>` (`read.rs:117–139, 449`: furigana `<rPh>` runs and `<phoneticPr>` on shared strings)
   - `vba_project: Option<Arc<Vec<u8>>>` (`read.rs:434–443`: raw `xl/vbaProject.bin` preservation)
   - `preserved: Option<Arc<Preserved>>` (`read.rs:265, 444`: unmodelled OPC parts, chartsheets, external links, custom XML, calc features)

2. **Sheet-level & Cell-level fields (24 fields):**
   - `file_id: u32` (`read.rs:348`: original `sheetId` attribute for round-trip relationship stability)
   - `cell_pictures: BTreeMap<CellRef, Arc<CellPicture>>` (`read.rs:266–272`, `sheet_read.rs:527–543`, `richdata.rs`: in-cell `vm` rich-data PNG/JPEG images)
   - ` py: Option<Arc<PyCode>>` on `Cell`, plus `spill: BTreeMap<CellRef, Value>` and `spill_ranges: BTreeMap<CellRef, RangeRef>` (`read.rs:274`, `sheet_read.rs:96–121, 549–587`, `python_part.rs`: `xl/python.xml` script resolution and spill settling)
   - `Formula { expr: Option<Arc<Expr>>, array: Option<RangeRef>, dynamic: bool }` (`sheet_read.rs:544–545, 594–653`, `xls.rs:444–534`: parsed `gridcraft_formula::Expr` AST, shared-formula relative coordinate shifting, and legacy/dynamic array range blanking)
   - `freeze: Option<(u32, u32)>`, `view_active: CellRef`, `view_top_left: CellRef`, `view: SheetViewMode`, `zoom: u16` (`sheet_read.rs:354–387`)
   - `tab_color: Option<Color>` (`sheet_read.rs:186–188`)
   - `show_gridlines`, `show_headings`, `show_formulas`, `show_zeros`, `show_outline_symbols`, `summary_below`, `summary_right`, `right_to_left` (`sheet_read.rs:189–192, 356–361`, `xls.rs:781–790`)
   - `cond_formats: Vec<CondFormat>` & `dxfs: Vec<Style>` (`sheet_read.rs:204, 712–726`, `sheet_read/rules.rs:49–136`: conditional formatting rules and `x14` dataBar extension merging)
   - `validations: Vec<Validation>` (`sheet_read.rs:205–209, 691–697`, `sheet_read/rules.rs:139–192`)
   - `tables: Vec<Table>` (`sheet_read.rs:288–299`, `sheet_read/parts.rs:106–164`: structured tables, column totals, XML column properties)
   - `autofilter: Option<AutoFilter>` (`sheet_read.rs:211`, `sheet_read/parts.rs:48–104`)
   - `comments: BTreeMap<CellRef, Comment>` (`sheet_read.rs:306–329`, `sheet_read/parts.rs:166–273`: legacy VML note boxes + modern threaded comments with `persons` resolution)
   - `hyperlinks: BTreeMap<CellRef, Hyperlink>` (`sheet_read.rs:210`, `sheet_read/parts.rs:6–37`)
   - `charts: Vec<Chart>`, `images: Vec<Image>`, `shapes: Vec<Shape>`, `slicers: Vec<Slicer>` (`sheet_read.rs:281–287`, `drawing.rs`, `drawing/ink.rs`, `chart.rs`, `chartex.rs`, `omml.rs`, plus `_xlchart.*` defined-name resolution in `read.rs:363–413`)
   - `sparklines: Vec<Sparkline>` (`sheet_read.rs:698–711`)
   - `pivots: Vec<PivotTable>` (`read.rs:341–346`, `pivot/read.rs:1–385`)
   - `protection: Option<SheetProtection>` & `edit_ranges: Vec<EditRange>` (`sheet_read.rs:263–280, 301, 342–352`)
   - `print: PrintSettings` (`sheet_read.rs:212–262`, `read.rs:380–390`, `xls.rs:863–874`: orientation, paper size, scale, fit-to-page, margins, odd/even/first headers & footers, row/col breaks, print area, print titles)
   - `preserved: Option<Arc<Preserved>>` (`read.rs:347`, `passthrough/read.rs`)

### B. Deep Interleaving of Parsing and Model Construction

1. **XLSX Cell & Formula Streaming (`sheet_read.rs:477–653`):**
   - `read_sheet` (`sheet_read.rs:160–179`) streams `<row>` elements via `xml::parse_streaming`. Inside `read_cell` (`477–592`), before a cell's `Value` is even stored:
     - If `vm` is present (`527–543`), it indexes into `cx.cell_pictures` (parsed from `xl/richData/*` in `richdata.rs`) and writes to `sheet.cell_pictures` instead of `sheet.cells`.
     - Formulas (`read_formula_text`, `599–653`) call `fmla::from_file` (which parses the formula into `gridcraft_formula::Expr` to strip `_xlfn.`/`_xlpm.` prefixes and normalize structured refs), optionally unwrap Google Sheets wrapper formulas using the caller's `known_function: Option<&dyn Fn(&str) -> bool>` callback (`600–603`), and for `<f t="shared">` parse the master formula into `gridcraft_formula::Expr` and call `gridcraft_formula::adjust::shift_relative(e.clone(), dr, dc)` (`619`) for follower cells.
     - Next, `python_part::cell_code` (`550–565`) inspects the parsed `Formula` AST against `xl/python.xml` (`cx.python`) to convert `_xlfn._xlws.PY(...)` formulas into `PyCode` objects and record or forget dynamic spill ranges (`record_python_spill`, `forget_dynamic`).
     - Finally, `dynamic_range` (`566–588`) checks whether the current cell falls inside a previously seen array or Python spill range and either blanks the cached value (`Value::Empty`) or routes it into `sheet.spill`.
2. **XLS BIFF8 Formula & Style Construction (`xls.rs:444–534`, `xls/formula.rs:1–551`, `xls/styles.rs:372–404`):**
   - BIFF8 stores formulas as reverse-Polish `Ptg` byte streams (`rgce`/`rgcb`), not text. `xls/formula.rs::decode` decodes `Ptg` tokens directly into `gridcraft_formula::Expr` AST nodes and formats them via `gridcraft_formula::print(&e)` (`xls.rs:452, 856`).
   - Shared and array formulas (`SHRFMLA` `0x04BC`, `ARRAY` `0x0221`) follow their first `FORMULA` (`0x0006`) record and are linked via `PtgExp` (`0x01`) anchor coordinates; `resolve_pending` (`xls.rs:479–518`) decodes each shared template with relative offsets (`Mode::Shared`) per follower cell and `blank_array_followers` (`523–534`) clears cached values across the array range.
3. **XLSX Multi-Part Cross-References (`read.rs:141–175, 258–453`):**
   - `read_workbook_styles` (`read.rs:141–175`) requires `theme1.xml` (`wb.theme.minor_font`) and `featurePropertyBag.xml` (`checkboxes`) before reading `styles.xml`.
   - `definedNames` (`read.rs:363–413`) splits `_xlnm.print_area` / `_xlnm.print_titles` into `sheet.print` and resolves `_xlchart.*` names directly into `sheet.charts[*].series[*]`.
   - `passthrough::KeptReader` (`read.rs:265, 347, 444`) walks the OPC relationship graph alongside the reader to preserve unmodelled XML elements and binary parts while renumbering indices via `file_to_model`.

---

## 4. Per-Reader Split Plan (Two-Layer Architecture)

To extract `craft-sheet` cleanly without rewriting GridCraft's feature readers or losing fidelity, the split should separate **low-level format/table readers (`craft-sheet`)** from **workbook feature/calculation modelling (`gridcraft-xlsx`)**:

### 1. XLS (`gridcraft/crates/xlsx/src/xls.rs` + `xls/*`)
- **Move to `craft-sheet::xls`:**
  - `xls/records.rs` (lines 1–223): BIFF8 record framing, `Continue` stitching, and Latin-1/UTF-16LE string continuation (`Records`, `Record`, `Cursor`).
  - `xls.rs` globals & cell-record parser (`lines 150–389, 536–844`), refactored to emit raw BIFF8 worksheet records + decoded `Globals` (`BoundSheet`, `sst`, `formats`, `StyleRecords`, `Links`, `Lbl`) and cell values (`RK`, `MULRK`, `NUMBER`, `LABELSST`, `LABEL`, `RSTRING`, `BOOLERR`, `FORMULA` cached values + `STRING` `0x0207`, `MERGECELLS` `0x00E5`, `COLINFO` `0x007D`, `ROW` `0x0208`, `DEFAULTROWHEIGHT` `0x0225`, `WINDOW2` `0x023E`).
  - `xls/styles.rs` (lines 1–405): convert `Font`, `Xf`, `Palette`, and `Style` records into `craft-sheet` neutral `Style` types.
  - **Formula handling in `craft-sheet`:** Either move the self-contained RPN token printer (`xls/formula.rs` lines 1–551 + `xls/ftab.rs` lines 1–376) into `craft-sheet::xls::formula` printing directly to formula text strings (replacing `gridcraft_formula::Expr` construction with a lightweight token-to-string formatter), or expose the raw `(rgce, rgcb, Links)` on `craft_sheet::CellFormula` / `DefinedName` so `gridcraft-xlsx` decodes them into `gridcraft_formula::Expr` while `wordcraft` uses the cached cell values directly.
- **Keep in `gridcraft-xlsx` (~150 lines adapter):**
  - Convert `craft_sheet::Workbook` styles/cells/merges/line metrics/print areas into `gridcraft_model::Workbook` and parse formula strings (or `rgce`) into `gridcraft_formula::Formula`.
- **Delete in `wordcraft` (`crates/formats/src/sheet/xls.rs`):**
  - Drop lines 1–438 (all of `is_xls`, `sheet_names`, `read_rows`, `workbook_stream`, `Pieces`, `globals`, `sheet_rows`), keeping only the 85-line test fixture builder `pub fn build` (lines 439–524) and tests.

### 2. ODS (`gridcraft/crates/xlsx/src/ods.rs`)
- **Move to `craft-sheet::ods`:**
  - Move `ods.rs` (lines 1–660) in full to `craft-sheet::ods`: `has_mimetype` (lines 33–45), `check_manifest` (lines 99–137), streaming `NsReader` state machine `Import` (lines 191–482), typed value & ISO date/duration conversion `convert` / `parse_date` / `parse_duration` (lines 484–659), and repeat/merge expansion budget enforcement `finish_row` (lines 531–576).
  - Also capture `table:formula` text (currently only checked via `.is_some()` at line 336) into `CellFormula` after stripping the `of:=` prefix, and preserve raw display text alongside `Value::Number` when needed.
- **Keep in `gridcraft-xlsx` (~40 lines adapter):**
  - Map `craft_sheet::Workbook` sheets, cells, merges, and derived number-format styles (`0.00%`, `yyyy-mm-dd`, etc.) into `gridcraft_model::Workbook` and apply `unique_name` sheet-name sanitization (`ods.rs:302–309`).
- **Delete in `wordcraft` (`crates/formats/src/sheet/ods.rs`):**
  - Drop all 189 lines of `wordcraft/crates/formats/src/sheet/ods.rs`, replacing `ods_read` with a thin call to `craft_sheet::read_ods` + display-string formatting.

### 3. XLSX (`gridcraft/crates/xlsx/src/{package.rs, xml.rs, styles.rs, read.rs, sheet_read.rs}`)
- **Move to `craft-sheet::xlsx`:**
  - Package & XML layer: `package.rs` (lines 1–266, built on `craft-archive` and `craft-ooxml`) and `xml.rs` (lines 1–750, streaming + DOM parser, `rich_text`, `decode_xstring`, `phonetic_data`).
  - Styles & Theme reader: `theme.rs::read_theme` (lines 47–178), `feature_bags.rs::checkbox_complements` (lines 1–45), and `styles.rs::read_styles` (lines 1–443).
  - Core workbook & worksheet cell/grid reader:
    - `read.rs`: `read_shared_strings` (lines 108–139), `workbook_xml` (lines 178–194), `read_workbook_properties` (lines 197–224), `selected_workbook_view` (lines 228–231), sheet list & defined-name extraction (lines 298–395).
    - `sheet_read.rs`: `read_sheet_format` (lines 142–158), `read_views` (lines 354–387), `read_cols` (lines 389–419), `read_row` & `read_cell` value/formula extraction (lines 421–526, 594–688), and `<mergeCells>` (lines 197–203).
    - Expose parsed worksheet root `El` (with `<row>` elements stripped by `parse_streaming`, as `sheet_read.rs:165–175` already does!) and package/relationships handle on the `craft-sheet` output so `gridcraft-xlsx` can run its secondary feature passes without re-parsing the worksheet XML or re-inflating ZIP entries.
- **Keep in `gridcraft-xlsx` (~350 lines adapter + existing feature modules):**
  - Post-process cell formulas through `fmla::from_file`, `unwrap_google_sheets`, `gridcraft_formula::adjust::shift_relative` (for shared formulas), `python_part::cell_code`, and `richdata` (`vm` cell pictures).
  - Run existing worksheet/workbook feature readers over the retained root `El` and `Package`: `sheet_read/parts.rs` (hyperlinks, autofilter, tables, comments, threaded comments), `sheet_read/rules.rs` (conditional formatting, validations), `drawing.rs` / `chart.rs` / `chartex.rs` / `omml.rs`, `pivot/read.rs`, and `passthrough/read.rs`.
- **Delete in `wordcraft` (`crates/formats/src/sheet/xlsx.rs`):**
  - Drop all 313 lines of `wordcraft/crates/formats/src/sheet/xlsx.rs` (plus `Package` in `sheet.rs`), replacing `xlsx_sheets` and `xlsx_rows` with `craft_sheet::read_xlsx` and `date.rs` formatting.

---

## 5. Consumer Impact Estimate

| Consumer | Action | Size Estimate |
| :--- | :--- | :--- |
| **wordcraft** (`crates/formats/src/sheet/`) | Delete `xls.rs` reader (438 lines), `ods.rs` (189 lines), `xlsx.rs` (313 lines), and `Package`/XML walker in `sheet.rs` (~110 lines); keep `xls::build` (85 lines), `date.rs` (178 lines), and a ~90-line adapter over `craft-sheet`. | **~1,050 lines deleted**, ~90 lines added (net **-960 lines**). |
| **gridcraft** (`crates/xlsx/src/`) | Replace `xls.rs` + `xls/*`, `ods.rs`, `package.rs`, `xml.rs`, `styles.rs` (read half), and core cell/row/col/merge reading in `read.rs` & `sheet_read.rs` with `craft-sheet` + model/formula adapter. | **~3,400 lines moved** to `craft-sheet`; **~500–550 lines** of adapter in `gridcraft-xlsx`. |
