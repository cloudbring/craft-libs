# Shared xtask survey

Read-only survey on 2026-11-03 of `xtask/src/` in all seventeen requested repositories.
All source material is first-party, explicitly authorized for movement in ticket #70.
No third-party implementation source was consulted. Sizes below are source line counts,
including tests; similarly sized files may differ in comments or fixture names.

| Task | Repositories and variant sizes (lines) | Configuration versus logic |
|---|---|---|
| version | lightcraft, vectorcraft, designcraft, deckcraft, cadcraft, wordcraft, gridcraft, craft-launcher: 152, byte-identical; photocraft: 168; texturecraft/materialcraft: 148; soundcraft: 217; filmcraft: 149; effectcraft: 152; pdfcraft: 194 | Root/command plumbing and staging paths are configuration. Photo/Sound strict prerelease validation, Sound bounded numeric components/rollback, and PDF workspace path pins are logic now selected by config. |
| stats | photocraft/lightcraft/vectorcraft/designcraft/deckcraft/wordcraft: 118, byte-identical; cadcraft: 175; gridcraft: 139 | Shared report/count logic; CAD entry budget/no symlinks and Grid canonical-directory dedup/directory budget are traversal policies. Shared traversal is bounded and cycle-safe. |
| ico | vectorcraft/craft-anything: 64; photocraft/lightcraft/designcraft/deckcraft/cadcraft/filmcraft/effectcraft/wordcraft/gridcraft: 104; soundcraft: 106; craft-launcher: 107; texturecraft/materialcraft: 173 | Paths, ordered sizes and format are config. Sound changes argument types/test root; most 104-line variants change test asset paths. Launcher adds checked fields; Texture/Material add ICNS. Weak 64-line validation is not preserved for malformed inputs. |
| icns | craft-launcher: 117; texturecraft/materialcraft: included in 173-line ico | Same slot mapping and packing, selected by format. |
| layers | photocraft: 400; lightcraft: 351; texturecraft: 220; vectorcraft: 363; figcraft: 76; designcraft: 340; deckcraft: 352; cadcraft: 334; wordcraft: 337; pdfcraft: 361; gridcraft: 331; materialcraft: 209 | Layer tables, allowed/dev edges, standalone/testkit rules, GPU/network restrictions, format allowlists and unknown-crate policies are configuration over a shared checker. Twelve implementations exist now, not six. |
| parity | designcraft/deckcraft: 99; wordcraft: 22; lightcraft: 544; cadcraft: 153; filmcraft: 754; pdfcraft: 296; gridcraft: 152 | Design/Deck weighted tables differ only in descriptive wording; Word invokes a report generator. Remaining apps implement domain-specific inventory/evidence logic, not a configurable weighted checklist. |
| pdf_deps | vectorcraft: 106; designcraft: 118; deckcraft: 228; effectcraft: 117 | Revision/package lists, exact/required features, forbidden active/production dependencies and target feature sets are config. Four files exist now, not five. Shared graph traversal includes normal/build edges, excludes dev-only edges. |
| SHA-256 | photocraft/sha256.rs: 66; vectorcraft/sha256.rs: 58; also pdfcraft/assets.rs and craft-launcher/local.rs wrappers | Two handwritten hashes, plus sha2 and platform-command wrappers, replaced by the existing streaming craft-hash API. |

FigCraft has no version task; Craft Anything has only ICO among these named files.
SoundCraft, FilmCraft, EffectCraft, Craft Anything and Craft Launcher have no layers.rs.
Absent tasks are not silently assigned another app's policy.

## Coverage boundary

All surveyed version, stats, icon, layer and PDF-dependency policies are expressible through
the shared structs, except intentionally unsafe/unbounded malformed-input behavior.
Tag/changelog support is opt-in; none of the identical originals modifies these files.

LightCraft parity validates reference evidence, CAD inspects a menu catalog, Grid inspects
engine registration, Film inventories menus/effects/transitions, and PDF validates TOML
features/evidence. These remain app-owned: moving their app-domain models into craft-libs
would violate the no-app-code rule. They can retain an app dispatcher arm or expose a
report command through `ParityConfig.command`; that hook does not transplant their logic.
No app repositories are edited in this library step.

## Provenance

Production modules name source repositories/files in their leading comments. Oracles retain
the first-party implementation with a provenance preamble. Layer fixture README records
source HEADs and metadata capture details. LightCraft fixture manifests/lockfile are from
HEAD `06c1b9ae0775464ce83ef0a9588be770337600f1`; launcher ICNS is from
`craft-launcher/xtask/src/icns.rs`, and parity from `designcraft/xtask/src/parity.rs`.
Snapshots reflect working-tree files, not a claim that every source checkout was pristine.
