//! Old-vs-new differential for the #48 codec de-fork (cloudbring/artcraft-algorithms#187; evidence
//! standard research/VERIFICATION.md section 1.1; results docs/verification/codec-oracle.md).
//!
//! OLD: the apps' forked codec crates at each app's pre-swap commit (git dependencies renamed in
//! Cargo.toml). NEW: the craft-{h264,hevc,av1,vp8,vp9,opus} workspace crates.
#![allow(clippy::all)]

pub mod containers;
pub mod inputs;
pub mod judge;
pub mod matrix;
mod matrix_data;
pub mod opus_quality;
pub mod report;
pub mod sides;
pub mod synth_h264;
pub mod synth_hevc;
pub mod trace;
