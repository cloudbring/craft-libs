# craft-distortion

Shared integer-exact 8-bit SAD, SATD and SSE for the H.264, HEVC, VP9 and AV1
encoders. Blocks run from 1×1 through 64×64, rectangles included. SATD sizes
are multiples of four. No `unsafe`.

```toml
craft-distortion = { git = "https://github.com/storytold/craft-libs", rev = "<commit>" }
```

## Dependency policy

`AGENTS.md` forbids `unsafe` (`unsafe_code = "forbid"` in the workspace lints)
and does not keep a separate crate allow-list. `wide` 1.7.1
(`MIT OR Apache-2.0 OR Zlib`, `default-features = false`) is a safe 128-bit
facade: NEON on AArch64, SSE2 on x86-64, wasm SIMD128 only when the target has
`+simd128`, and a scalar fallback otherwise. Calling `core::arch` here would
require `unsafe` in this crate, which the workspace lint rejects. SIMD inside
`wide` stays in that dependency, as it does for `rustfft` and `sha2`.

The `simd128` cargo feature documents the wasm build. On `wasm32-unknown-unknown`
it must be paired with `RUSTFLAGS="-C target-feature=+simd128"`; the crate
fails to compile if the feature is on and the target feature is off. Other
targets ignore the feature.

## SATD

| Function | Convention |
|---|---|
| `satd` | H.264 / FilmCraft. 4×4 Hadamards only, even for 8×8 and above. Truncating divide by 2. |
| `satd_av1` | EffectCraft `av1enc` (`decide.rs`). 8×8 tiles, divide by 4, when both sides are multiples of 8. Otherwise the H.264 4×4 score. |
| `satd_hevc` | EffectCraft `hevcenc` (`frame.rs`) / x265. 8×8 tiles use `(sum + 2) >> 2`. Other multiples of 4 use the 4×4 score, which matches `(sum + 1) >> 1` because that absolute sum is even. |
| `satd_vp9` | libvpx `vpx_hadamard_8x8`, `vpx_hadamard_16x16`, `vpx_hadamard_32x32` and `vpx_satd` (raw absolute sum, with libvpx's combining shifts). EffectCraft's VP9 encoder uses SAD and SSE only. libvpx stops at 32×32, so a multiple of 64 is the sum of 32×32 scores. A block that is not a multiple of 8 is twice `satd` (4×4, no divide). |

SAD and SSE have no codec-specific scale. Invalid geometry returns `u32::MAX`,
or `u64::MAX` for SSE, and never a partial block.

The H.264 4×4 / 8×8 / 16×16 path is the FilmCraft kernel, including the halved
4×4 Hadamard. Taller blocks are bands of 16 rows because the kernel's `u16`
SAD accumulator is exactly full at 16×16 (`16 * 16 * 255 = 65280`).

## Provenance

Extracted from FilmCraft `crates/h264enc/src/distortion.rs` at
`dc23d04795f5b7694f22c4c59b8cd8f972da3d2e` (`algo/99-simd-distortion`),
artcraft-algorithms issue 99. The verbatim kernel, minus its `mod tests` line,
is `tests/filmcraft_dc23d047_distortion.inc`. Sizes FilmCraft already scored
(SAD/SSE widths 4, 8 and 16 with height 1..=16, and SATD multiples of 4 up to
16×16) are tested against that copy. Larger sizes, the proper 8×8 Hadamard,
and the HEVC / VP9 / AV1 scales are extensions.

## Check

```
cargo test -p craft-distortion
cargo test -p craft-distortion --release -- --ignored --nocapture benchmark_kernels
cargo clippy -p craft-distortion --all-targets -- -D warnings
cargo fmt -p craft-distortion -- --check
cargo doc -p craft-distortion --no-deps
cargo check -p craft-distortion --target wasm32-unknown-unknown
RUSTFLAGS="-C target-feature=+simd128" cargo check -p craft-distortion --target wasm32-unknown-unknown --features simd128
```

The ignored benchmark prints microseconds per 10_000 blocks, scalar reference
against the SIMD kernel, for each encoder size.
