# LightCraft version oracle fixture

First-party LightCraft root Cargo.toml, Cargo.lock and workspace member manifests captured
2026-11-03 at HEAD `06c1b9ae0775464ce83ef0a9588be770337600f1`.
The original `lightcraft/xtask/src/version.rs` edits the root workspace package version and
runs `cargo update --workspace`; it does not edit member manifests. The golden test creates
two temporary copies, synthesizes empty source targets for Cargo manifest discovery, and
runs both original and shared tasks. It compares the resulting root manifest and real
lockfile byte-for-byte and asserts member manifests remain unchanged. The temporary target
bodies contain no application implementation code. App dependencies are not compiled.
