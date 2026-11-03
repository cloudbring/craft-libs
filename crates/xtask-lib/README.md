# craft-xtask

A normal library crate containing shared native build tasks. **A dev-tool dependency for
apps' `xtask` crates only**, never an app runtime dependency. No binary entrypoint or wasm
support. Workspace membership comes from `crates/*`; the dependency is registered explicitly.

## Dispatcher

Add `craft-xtask` to the app's **xtask crate** dependencies, then configure its policies:

```rust,no_run
use craft_xtask::{Configs, dispatch};

fn main() -> std::process::ExitCode {
    let mut args = std::env::args().skip(1);
    let task = args.next().unwrap_or_else(|| "help".into());
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent().unwrap_or(std::path::Path::new("."));
    let configs = Configs::for_root(root);
    // Set the app's layer policy and PDF pins before enabling those tasks.
    match dispatch(&task, &configs, &args.collect::<Vec<_>>()) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            std::process::ExitCode::FAILURE
        }
    }
}
```

Each module exposes `Config` (also named `VersionConfig`, `StatsConfig`, `IcoConfig`,
`LayersConfig`, `ParityConfig`, `PdfDepsConfig`), `run(&Config, &[String]) -> Result<()>`,
and `cli_help()`. Errors are strings, matching the original first-party tasks.
`dispatch` also accepts `icns`, `pdf-deps`, and help aliases. Commands inherit the caller's
environment, including `CARGO_TARGET_DIR`, and never change the process working directory.

## Configuration and behavior

| Task | Configuration and reusable functions |
|---|---|
| version | Manifest paths, staging directory, offline/online lockfile refresh, rollback, strict prereleases, bounded components, path-pin updates; `read`, `replace`, `rewrite`, `validate`. Defaults preserve the eight identical tasks, including their permissive parser and failed-refresh behavior. Optional changelog and tag-format support is additive, not inferred from original behavior. `tag` prints a tag; it does not create it. |
| stats | Root, excluded directory globs, symlink traversal, entry/directory budgets and error/truncate policy; `count`, `count_tests`, `report`. `--exact` counts tests through Cargo's test harness. Default traversal does not follow symlinks; opt in with cycle detection. |
| ico / icns | Ordered pre-sized source PNG paths, output path, expected sizes and format; `pack_ico` (`pack`), `pack_icns`, `png_size`, `run_icns`. No resampling. ICO accepts square 1–256 px images; ICNS accepts 16/32/64/128/256/512/1024 px and emits both applicable density slots. |
| layers | Ordered named layers with full crate-name globs, allowed and dev-only edges, standalone/exempt/testkit policies, external dependency rules and workspace allowlists; `check`, `from_metadata`. TOML policies and real graph fixtures cover all twelve currently present implementations. |
| parity | Weighted checklist markers/path or an app-owned Cargo report generator; `rows`, `summary`, `rewrite`. `--check` detects drift without writing. App-specific inventories and evidence validation remain app-owned. |
| pdf_deps | Exact package versions/source strings, required or exact feature sets, forbidden active packages, normal/build production reachability, native/wasm feature policies; `validate`, `validate_target_features`, `git_source`. Configure pins explicitly; empty pins are an error. |
| SHA-256 | `sha256_hex(&Path)` streams through sibling `craft-hash::sha256_file`; no new hashing implementation or direct sha2 dependency. |

Version updates stage manifests individually, not as a multi-file transaction; rollback is
an opt-in best-effort restoration of manifests after lockfile-refresh failure. Do not run
concurrent release updates. Original weak icon variants accepted malformed images; the
shared implementation intentionally rejects empty, zero-sized and nonsquare inputs.

## Evidence and tests

See [SURVEY.md](SURVEY.md) for source variants and provenance. Vendored first-party oracles
are compiled only in tests. Golden tests compare real LightCraft manifest/lockfile edits,
statistics text and ICO bytes, plus launcher ICNS and DesignCraft parity output. Layer
fixtures capture current workspace graphs, each with a forbidden-edge negative control.
Tests also cover command failures, traversal cycles/budgets, malformed graphs and PDF pins.

The version oracle runs real Cargo lockfile updates offline first; a clean machine needs
the fixture's registry/git dependencies cached or network access for the original online
fallback. No sibling app checkout is required to run tests. Native CI runs tests, clippy,
formatting and rustdoc; no wasm job is registered for this crate.
