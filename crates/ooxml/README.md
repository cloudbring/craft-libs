# craft-ooxml

Shared OOXML syntax for deckcraft, designcraft, wordcraft, and gridcraft. `dom::parse` limits
input, depth, element count, aggregate decoded text and attributes. Element names use local
names with the original prefix retained; attribute keys remain qualified by default. Set
`Limits { read_mode: ReadMode::Lenient, namespaces: Namespaces::Uri, ..Limits::default() }`
to recover truncated elements, stray ampersands, unknown/invalid references, duplicate attributes,
and mismatched closing tags, and to canonicalize wordcraft's namespace aliases by URI (including
Strict OOXML URIs). XML BOM and UTF-16 input are accepted in both modes; lenient UTF-16 decoding
replaces damaged surrogate pairs. `write::XmlWriter` writes escaped
UTF-8 XML. `opc::Package` reads guarded ZIP entries using `craft-archive::ZipGuard`, parses
content types and resolves relationship targets; `opc::PackageWriter` emits canonical metadata,
ordered ZIP entries and fixed ZIP timestamps. `PackageWriter::with_policy` selects insertion,
sorted, glob or ranked part order, per-entry compression, generated or preserved content types,
relationship position, and a DOS timestamp. `WritePolicy::{deckcraft,wordcraft,gridcraft,designcraft}`
provide writer presets. Consumers whose app builds content types itself can set
`ContentTypesPolicy::Preserve(bytes)`; the ZIP writer places these bytes first.
App-specific slide/document/sheet models stay out.

Defaults match the strictest common depth (256), tree size (gridcraft: 1 million elements),
entry count (wordcraft: 20,000), single part (wordcraft: 256 MiB), total unpacked bytes
(wordcraft: 512 MiB), and deckcraft's XML part cap (96 MiB). The 1 MiB decoded-attribute
budget is an additional safeguard; each element also accepts at most 1024 attributes (wordcraft).
Callers can supply different `Limits` or `OpcLimits` for their workloads. Whole-part DOM parsing
is not a replacement for gridcraft's row/shared-string streaming.

Escaping follows deckcraft's control-character policy: text CR becomes `&#13;`, attribute
TAB/LF/CR become references; illegal XML 1.0 characters are dropped. This writer also emits
`&apos;` in attributes; `El::to_xml` uses `XmlWriter::deckcraft()` to leave the apostrophe literal
as deckcraft does. It leaves quotes/apostrophes
literal in text. Wordcraft encodes all three whitespace characters even in text, designcraft
encodes quotes/apostrophes in text but leaves whitespace literal, and gridcraft uses separate
rich-text and SpreadsheetML `_xHHHH_` routines. Those routines depend on
gridcraft's phonetic models and are not generic XML or OPC operations, so they stay there.
Generic relationship types are strings; the application-owned catalog
of relationship kinds and part naming conventions stays with each app.

No small checked-in `.pptx`, `.docx`, or `.xlsx` test fixtures were found in the four app test
directories during the ticket survey; generated package tests exercise the shared layer instead.
