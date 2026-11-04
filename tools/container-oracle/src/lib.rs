//! Old-vs-new differential for two shared-crate swaps, over the Office corpus and hostile
//! synthetic inputs (cloudbring/artcraft-algorithms#186; evidence standard
//! research/VERIFICATION.md section 1.1; results docs/verification/container-oracle.md):
//!
//! - #120 craft-archive: the zip writers of pdfcraft, effectcraft and vectorcraft, the zip readers of
//!   effectcraft and vectorcraft, and craft-doc's `.pcraft` reader;
//! - #123 craft-cfb: the compound-file readers of gridcraft, deckcraft, filmcraft and wordcraft.
//!
//! OLD is each app's own reader/writer at the swap's merge-base (vendored under `old/<app>/` when it
//! is a private module of a heavy crate; a renamed git dependency when it is a public light crate);
//! NEW is the swap branch's adapter over the workspace `craft-archive` / `craft-cfb` (vendored under
//! `new/<app>/` or a git dependency whose `craft-cfb` is patched to this checkout).
#![allow(dead_code, unused, clippy::all)]

#[path = "../old/pdfcraft/zip.rs"]
pub mod old_pdfcraft_zip;
#[path = "../old/effectcraft/zip.rs"]
pub mod old_effectcraft_zip;
#[path = "../old/vectorcraft/zip.rs"]
pub mod old_vectorcraft_zip;
#[path = "../old/gridcraft/cfb.rs"]
pub mod old_gridcraft_cfb;
#[path = "../old/deckcraft/cfb.rs"]
pub mod old_deckcraft_cfb;
#[path = "../old/wordcraft/cfb_guard.rs"]
pub mod old_wordcraft_cfb_guard;

#[path = "../new/pdfcraft/zip.rs"]
pub mod new_pdfcraft_zip;
#[path = "../new/effectcraft/zip.rs"]
pub mod new_effectcraft_zip;
#[path = "../new/vectorcraft/zip.rs"]
pub mod new_vectorcraft_zip;
#[path = "../new/gridcraft/cfb.rs"]
pub mod new_gridcraft_cfb;
#[path = "../new/deckcraft/cfb.rs"]
pub mod new_deckcraft_cfb;

pub mod cfbside;
pub mod docside;
pub mod harness;
pub mod hostile;
pub mod zipside;
