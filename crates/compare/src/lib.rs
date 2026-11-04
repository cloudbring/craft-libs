#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]
//! craft-compare: shared comparison metrics, so a verdict means the same thing in every app.
//!
//! One module per output kind ([`bytes`], [`image`], [`audio`], [`xml`], [`text`],
//! [`numbers`]); each returns plain metric values. [`verdict`] turns those into a uniform
//! [`Verdict`] against a limit, which is what an oracle harness consumes.
//!
//! Provenance (ported, not depended on): image error metrics from vectorcraft
//! `testkit::raster` and filmcraft's render tolerance; word F1 and reading-order score from
//! pdfcraft's `xtask` text oracle.

pub mod audio;
pub mod bytes;
pub mod image;
pub mod numbers;
pub mod text;
pub mod verdict;
pub mod xml;

pub use bytes::{BytesDiff, compare_bytes};
pub use image::{Image, ImageReport, ImageTolerance};
pub use verdict::{Tolerance, Verdict};

/// Inputs that cannot be compared at all (as opposed to compared and found different).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CompareError {
    /// Image dimensions or channel counts differ, or the buffer does not match them.
    Shape(String),
    /// Audio buffers differ in length.
    Length { a: usize, b: usize },
    /// An XML document could not be parsed.
    Xml(String),
    /// PNG encoding failed.
    Png(String),
}

impl std::fmt::Display for CompareError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Shape(s) => write!(f, "shape mismatch: {s}"),
            Self::Length { a, b } => write!(f, "length mismatch: {a} vs {b}"),
            Self::Xml(s) => write!(f, "invalid XML: {s}"),
            Self::Png(s) => write!(f, "PNG error: {s}"),
        }
    }
}

impl std::error::Error for CompareError {}
