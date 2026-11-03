#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![forbid(unsafe_code)]

//! Integer-exact 8-bit SAD, SATD and SSE for encoder search and mode decision.
//!
//! H.264, HEVC, VP9 and AV1 can share one implementation. Blocks are at most
//! [`MAX_SIDE`] by [`MAX_SIDE`]. SAD and SSE accept any positive size in that
//! square, including rectangles. SATD sizes are multiples of four.
//!
//! SIMD is 128-bit only, through the `wide` crate: NEON on AArch64, SSE2 on
//! x86-64, wasm SIMD128 when the target is built with `+simd128`, and `wide`'s
//! scalar fallback otherwise. This crate has no `unsafe`. See the README for
//! why `wide` is the dependency that satisfies the workspace rule.
//!
//! An invalid or unsupported geometry returns [`u32::MAX`] (or [`u64::MAX`] for
//! SSE), never a partial-block score.
//!
//! # SATD conventions
//!
//! * [`satd`] is the H.264 score used by FilmCraft: every block, including 8×8
//!   and larger, is a sum of 4×4 Walsh-Hadamard absolute values, divided by 2
//!   (truncating). Replacing an 8×8 with one 8×8 transform would change encoder
//!   decisions.
//! * [`satd_av1`] matches EffectCraft `av1enc`: a block whose sides are both
//!   multiples of 8 is tiled with 8×8 Hadamards, each divided by 4. Any other
//!   multiple of 4 is the H.264 4×4 score (divide by 2). The 4×4 absolute sum
//!   is always even, so that divide matches a per-tile truncating half.
//! * [`satd_hevc`] matches EffectCraft `hevcenc` / x265. Multiples of 8 use
//!   8×8 tiles and `(sum + 2) >> 2` on each tile. Anything else uses the 4×4
//!   score. HEVC writes that 4×4 scale as `(sum + 1) >> 1`; it equals the
//!   H.264 score because the 4×4 absolute sum is even.
//! * [`satd_vp9`] follows libvpx `vpx_hadamard_8x8` / `vpx_hadamard_16x16` /
//!   `vpx_hadamard_32x32` plus `vpx_satd` (the raw sum of absolute coefficients,
//!   shifts included). EffectCraft's VP9 encoder does not call SATD; it uses
//!   [`sad`] and [`sse`]. libvpx has no 64×64 Hadamard, so a 64-multiple is a
//!   sum of 32×32 scores. A block that is not a multiple of 8 uses 4×4
//!   Hadamards with no divide, which is twice [`satd`].
//!
//! ```
//! use craft_distortion::{sad, satd, satd_av1, sse};
//!
//! let source = [10u8; 64];
//! let prediction = [0u8; 64];
//! assert_eq!(sad(&source, 8, &prediction, 8, 8, 8), 640);
//! assert_eq!(sse(&source, 8, &prediction, 8, 8, 8), 6_400);
//! assert_eq!(satd(&source, 8, &prediction, 8, 8, 8), 320);
//! assert_eq!(satd_av1(&source, 8, &prediction, 8, 8, 8), 160);
//! ```

#[cfg(all(feature = "simd128", target_arch = "wasm32", not(target_feature = "simd128")))]
compile_error!("craft-distortion feature `simd128` requires RUSTFLAGS='-C target-feature=+simd128' so `wide` enables wasm SIMD128");

mod kernel;

pub use kernel::{sad, sad8, sad16, satd, satd_av1, satd_hevc, satd_vp9, sse};

/// Largest width or height accepted by every kernel, in samples.
pub const MAX_SIDE: usize = 64;

#[cfg(test)]
mod tests;
