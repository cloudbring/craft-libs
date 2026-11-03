#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

//! Shared, bounded OOXML syntax and Open Packaging Conventions; app part models stay in apps.

pub mod dom;
pub mod opc;
pub mod write;
