Craft App Libraries
===================

<a href="https://sonarcloud.io/project/overview?id=storytold_craft-libs"><img alt="Scanned by SonarCloud" src="https://img.shields.io/badge/Scanned_by-SonarCloud-4b9fd5?style=flat-square&logo=sonarqubecloud&logoColor=white"></a>

This repo contains libraries used by the various Craft Apps. It's a Rust monorepo workspace, and the libraries can be included by any Craft Apps that need them.

In the future this may grow to include non-Rust library code.

If any libraries grow beyond Craft App usage, eg. RAW format support, they may be promoted out and into their own libraries for the broader ecosystem to take advantage of.


## Layout

A Cargo workspace (edition 2024, resolver 3). Each library is a crate under `crates/<name>`, named
`craft-<name>`, with `version`, `edition`, `license` and `repository` inherited from the workspace
(`license.workspace = true`, and so on) and `[lints] workspace = true`. Register each crate in
`[workspace.dependencies]` in the root `Cargo.toml`.

Apps depend on a crate by git:

```toml
craft-<name> = { git = "https://github.com/storytold/craft-libs", rev = "<commit>" }
```

## Development-tool layering

`app xtask → craft-xtask → craft-hash`. `craft-xtask` is a dev-tool dependency for apps'
`xtask` crates only; runtime libraries and application binaries must not depend on it.
Its process/filesystem tooling is native-only and is not part of the per-crate wasm checks.

## Libraries

- [`craft-xtask`](crates/xtask-lib/README.md): shared native version, statistics, ICO/ICNS,
  layer, parity, PDF dependency and SHA-256 build-tool tasks.

- [`craft-ai-core`](crates/ai-core/README.md): the data model and contract for AI in the Craft Apps (connections, models, tools, requests, jobs, consent, history, route spec). Pure, no I/O, wasm-safe. Contract in [docs/ai-shared-layer.md](docs/ai-shared-layer.md).
- [`craft-archive`](crates/archive/README.md): bounded raw/zlib/gzip DEFLATE decoding with byte accounting, validated ZIP entries and CRC-checked PNG chunks.
- [`craft-ooxml`](crates/ooxml/README.md): bounded XML tree, escaping writer, and deterministic OPC ZIP packages with content types and relationships; app part models stay in apps.
- [`craft-atomic`](crates/atomic/README.md): Crash-safe atomic file writing and replacement with same-directory staging, directory fsync, Apple `F_FULLFSYNC` fallback, transient lock backoff, and fault injection for the Craft suite.
- [`craft-audio-rt`](crates/audio-rt/README.md): allocation-free SPSC queues, owned snapshot handoff, atomic parameter ramps, panic silencing and UI-side stall detection; device glue stays in apps.
- [`craft-av1`](crates/av1/README.md): clean-room pure-Rust AV1 decoder from FilmCraft,
  including film grain and optional threading; ffmpeg/libsvtav1/libaom/libdav1d are test oracles only.
- [`craft-bitstream`](crates/bitstream/README.md): shared bit-level reader/writer from `craft-media` and the allocation-free VP8/VP9 boolean decoder from FilmCraft.
- [`craft-blend`](crates/blend/README.md): W3C separable blending and straight-alpha source-over, checked exact rationals, Lean 4 bounds and identity proofs, and Lean-generated conformance vectors (Lean is not run in CI yet).
- [`craft-blur`](crates/blur/README.md): separable floating-point Gaussian and running-sum box filters with explicit exact, recursive and box approximation contracts, edge policies, cancellation and reusable worker scratch.
- [`craft-bvh`](crates/bvh/README.md): binned surface-area-heuristic mesh BVH (12 bins, leaf size 4) over `[f32; 3]` triangles: closest-hit and any-hit rays, nearest point and box queries. No dependencies. Each split is scored from a prefix/suffix sweep of the bin bounds.
- [`craft-cache`](crates/cache/README.md): cost-bounded LRU, frame-sweep caching, shared budgets and stable fingerprints.
- [`craft-cfb`](crates/cfb/README.md): bounded, read-only OLE2/CFB directory and stream reader with FAT, DIFAT, and mini-stream support.
- [`craft-cms`](crates/cms/README.md): pure-Rust ICC v2/v4 colour management: profile parsing
  and writing, transforms, rendering intents, black point compensation, soft proofing, gamut
  checks, 3D LUTs and built-in profiles (sRGB, Display P3, synthetic coated CMYK and others).
- [`craft-crw`](crates/crw/README.md): clean-room Canon CRW (CIFF) raw decoder, from LightCraft:
  the heap container, the entropy coding derived by black-box analysis (frozen code tables), sensor
  black and white levels and as-shot white balance. The complete derivation record (plan, sources
  log, scripts, verification) is in `crates/crw/clean-room/`. Colour stays in each app.
- [`craft-dialogs`](crates/dialogs/README.md): native file and message dialogs that never block
  the event loop: queued requests shown at the end of the frame and answered on a later frame,
  free-standing Open and Folder panels and Save sheets on macOS, case-insensitive filters. What
  to do with the answer stays in each app.
- [`craft-diff`](crates/diff/README.md): the anchored, budgeted sequence diff behind pdfcraft's and
  wordcraft's compare — common edge trim, bounded saved-frontier Myers, patience and
  longest-common-run anchors, recursion per gap under depth and work budgets, deterministic
  replace fallback — generic over any `Hash + Eq` token. No dependencies.
- [`craft-distortion`](crates/distortion/README.md): shared 8-bit SAD, SATD and SSE for the H.264,
  HEVC, VP9 and AV1 encoders. Blocks up to 64×64. 128-bit SIMD through `wide` (NEON, SSE2,
  optional wasm SIMD128) with a scalar fallback and no `unsafe`.
- [`craft-doc`](crates/doc/README.md): reads PhotoCraft `.pcraft`, VectorCraft `.vectorcraft`
  and EffectCraft `.ecproj` documents without the apps: the kind and version, the previews and
  PDF they carry, and their pages and layers. Rendering and editing stay in each app.
- [`craft-docpath`](crates/docpath/src/lib.rs): decides whether a path or `file:` URL stored in a
  document may be opened. Paths stored in documents are followed only when they are local or on the
  document's own network share; other network paths, device paths and non-file URLs are refused.
- [`craft-ecproj-format`](crates/ecproj-format/README.md): the envelope of EffectCraft `.ecproj`
  project files: `savedBy`, the schema version and its check, and the file object with the
  project and its saved view as type parameters. The project model stays in EffectCraft.
- [`craft-fft`](crates/fft/README.md): cached f32/f64 complex and real FFTs, FFT-based MDCT/IMDCT with sine/KBD TDAC windows, and allocation-free STFT overlap-add.
- [`craft-font-catalog`](crates/font-catalog/README.md): the installed fonts on every platform
  (font folders, fontconfig and Flatpak host folders, CoreText, DirectWrite, the font store),
  lookup by family, style, localized, legacy, full and PostScript names, family loads shared
  between threads, and rescans when fonts are installed or removed.
- [`craft-font-manager`](crates/font-manager/README.md): Craft Font Manager as a library: search
  and filters over the Google Fonts index, installed fonts by family, the font store interface,
  and (`egui` feature) `FontsPanel`, the whole interface as one panel for the launcher's Fonts tab
  and the standalone app.
- [`craft-font-store`](crates/font-store/README.md): the suite's font store: the pinned Google
  Fonts and Noto indexes plus indexes the user adds (never curated), exact lookup by family and
  style or PostScript name, and installs from each source's URLs (jsDelivr, then raw GitHub)
  checked by size, sha256 and parsing, with the licence, a manifest, and removal. Off until the
  user turns downloading on; agents can't.
- [`craft-fonts-build`](crates/fonts-build/README.md): embeds the optional craft-fonts faces
  from an app's `build.rs` (`CRAFT_FONTS_DIR`, `CRAFT_FONTS_REQUIRED`, script filters) as a table
  of `EmbeddedFace`s. No dependencies; the app picks the faces.
- [`craft-fuzzy`](crates/fuzzy/README.md): cached Unicode-aware command-palette matching and highlighting.
- [`craft-h264`](crates/h264/README.md): clean-room pure-Rust H.264/AVC decoder from FilmCraft,
  with CAVLC/CABAC and optional frame threading; ffmpeg is a test oracle, not a dependency.
- [`craft-hash`](crates/hash/README.md): CRC-32 (ISO-HDLC and MPEG-2), Adler-32, FNV-1a 32/64, a
  stable 128-bit content hash (FNV-1a 128) and lowercase SHA-256. Streaming and one-shot, for
  checksums and cache keys. SHA-256 is the only cryptographic hash.
- [`craft-hevc`](crates/hevc/README.md): clean-room pure-Rust H.265/HEVC Main / Main 10 decoder
  from FilmCraft, with optional frame threading; ffmpeg/libx265 are external test oracles only.
- [`craft-i18n`](crates/i18n/README.md): translations over the embedded craft-i18n catalogs:
  lookup with fallback (`pt-br` → `pt` → English), plurals, language names, the system locale and
  a pseudo locale. The catalogs stay a fully optional build input; apps keep `tl!` and their
  language setting.
- [`craft-io`](crates/io/README.md): bounded reads of regular files and generic readers, with
  nonblocking Unix opens, handle-based file checks, and structured overflow errors.
- [`craft-jpeg`](crates/jpeg/README.md): clean-room decoder for 12-bit and arithmetic-coded
  JPEG (T.81 SOF0/SOF1 at 12 bits, SOF9, SOF10), from PhotoCraft, and a frame probe for routing.
  Metadata stays in each app.
- [`craft-keymap`](crates/keymap/README.md): keyboard bindings: chords and their text forms,
  scopes, conflicts, user overrides stored through craft-settings and import of other
  applications' shortcut sets. Command catalogs, file readers and the shortcut editor UI stay
  in each app.
- [`craft-log`](crates/log/README.md): asynchronous rotating desktop logging with a diagnostics ring.
- [`craft-media`](crates/media/README.md): bit-level I/O (`bitstream`), clean-room MP4/MOV
  (`isobmff`) and Matroska/WebM (`matroska`) demuxers and muxers, from FilmCraft, and an ISO BMFF
  box reader. Containers only; codecs stay in each app.
- [`craft-menu`](crates/menu/README.md): an app's menus as a macOS menu bar: the Mac layout
  (app menu, Window, Help, system-key clashes), a platform backend trait, and key equivalents
  fed back to egui as the input egui-winit would have made. Commands and their enabled state
  stay in each app.
- [`craft-metafile`](crates/metafile/README.md): bounded EMF/WMF/EMF+ reading into a neutral scene
  (curves, text, tiles, bitmaps and clips); shared format/algorithm layer below app document
  adapters and renderers.
- [`craft-opus`](crates/opus/README.md): clean-room Opus decoder (SILK, CELT, hybrid and
  multistream surround) from FilmCraft, with shared `craft-fft` MDCTs; downloaded RFC vectors
  and external ffmpeg/libopus are test-only oracles.
- [`craft-pcraft-format`](crates/pcraft-format/README.md): the envelope of PhotoCraft's `.pcraft`
  bundle: `Manifest<D>` with the document as a type parameter, `TileRef`, the entry paths, the
  manifest decode limits and the migration runner. The document model stays in PhotoCraft.
- [`craft-pdf`](crates/pdf/README.md): independently optional PDF parsing, interpretation, rasterization and guarded authoring. Apps retain layout, fonts, editing and export policy; writer-only consumers do not enable the shared reader.
- [`craft-platform`](crates/platform/README.md): optional native dialog lifecycle, Linux/BSD desktop appearance observation and browser download ownership; application policy stays in the apps.
- [`craft-print`](crates/print/README.md): PDF submission to CUPS and Windows, printer discovery,
  duplex/copies/colour and driver preferences. PDF generation stays in each app.
- [`craft-proof`](crates/proof/README.md): proofing per language (spelling, suggestions, grammar,
  hyphenation, a Hunspell reader, user words, Language packs). The word lists and patterns are
  craft-i18n's `language-data/`, embedded at build time from `CRAFT_I18N_DIR`: English and the
  carry-over locales by default, the set `CRAFT_PROOF_EMBED` names otherwise (`en` for English
  only); other languages come from installed Language packs.
- [`craft-quit`](crates/quit/src/lib.rs): native macOS quit, egui close handling and an optional
  app menu. Unsaved-document decisions and save prompts stay in each app.
- [`craft-scopes`](crates/scopes/README.md): reusable CPU waveform, parade, histogram and vectorscope count buffers, false-colour exposure bands, and caller-driven line parallelism; no dependencies or rendering.
- [`craft-settings`](crates/settings/README.md): `config.toml` settings storage for every app: platform folders, portable mode and overrides, atomic saves while running, backup, damaged-file recovery and schema migrations. Policy in [docs/settings-storage.md](docs/settings-storage.md).
- [`craft-text`](crates/text/README.md): text processing below the composer. Its `hyphen` module finds
  hyphenation points, with a bundled US English dictionary and patterns and optional hypher patterns
  for 47 more languages (one cargo feature each).
- [`craft-tiledir`](crates/tiledir/README.md): persistent (structurally shared, O(1)-clone) sparse radix-tree and dense 64-slot chunked tile directories for undo history and copy-on-write canvases.
- [`craft-time`](crates/time/README.md): exact rational media time, frame rates, SMPTE drop-frame
  timecode and sample-position conversions. App-specific timeline and musical-meter policy stays in each app.
- [`craft-ui`](crates/ui/README.md): SVG catalogs, icon buttons, tabs, pane controls, transactional docking, print settings and theme helpers for egui frontends;
- [`craft-view`](crates/view/README.md): canvas transforms and anchored zoom policies using emath.
  assets, themes, localization and command handling stay in each app. Requires Rust 1.95 or later.
- [`craft-vectorcraft-format`](crates/vectorcraft-format/README.md): the envelope of VectorCraft's
  `.vectorcraft` file: its members, versions, embedded images and the data after the JSON, with the
  document as a type parameter. The document and its migrations stay in VectorCraft.
- [`craft-vp8`](crates/vp8/README.md): clean-room pure-Rust VP8 decoder from FilmCraft; ffmpeg/libvpx generate local conformance fixtures and provide the test oracle.
- [`craft-vp9`](crates/vp9/README.md): clean-room pure-Rust VP9 decoder from FilmCraft, with optional threading; ffmpeg/libvpx-vp9 generate local fixtures and provide the test oracle.
- [Shared UI component audit](docs/ui-components-audit.md): source inventory, extraction priorities
  and compatibility boundaries across the Craft suite.
- [Theme infrastructure](docs/theme-infrastructure.md): eight-app comparison and the small shared
  widget-state styling boundary.
- [Panel docking](docs/ui-docking.md): validated panel moves, floating groups, workspace adapters
  and an optional egui docking surface.

## License and credits

craft-libs is dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.
Copyright (c) 2026 ArtCraft Team and the craft-libs contributors. Required notices are in [NOTICE](NOTICE).

Bundled fonts, icons, images and other assets keep their own open licenses; each one is listed
with its author, source and license in [ATTRIBUTION.md](ATTRIBUTION.md).
