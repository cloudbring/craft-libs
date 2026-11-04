//! Old-vs-new differential harness (ticket artcraft-algorithms#178).
//!
//! A consumer links its pre-change code next to the current crate (see the README), wraps each
//! in a [`Side`], picks a [`Judge`], and runs [`Differential`] over corpus files. Every file
//! gets one [`Outcome`]; registered intended changes (`oracle/expected.toml`) become
//! [`Outcome::ExpectedDifference`]; the [`Report`] gives JSON, markdown and an exit-code policy.

mod judge;
mod outcome;
mod registry;
mod report;
mod runner;
mod side;

pub use judge::{BytesJudge, FnJudge, ImageJudge, Judge, XmlJudge};
pub use outcome::{Outcome, SideId};
pub use registry::{Expected, Registry, RegistryError};
pub use report::{FileResult, Policy, Report};
pub use runner::Differential;
pub use side::{FnSide, Side};
