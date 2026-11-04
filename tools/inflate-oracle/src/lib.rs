//! Differential of craft-archive's inflate against lean-zip's verified pure-Lean decoder
//! (cloudbring/artcraft-algorithms#213; results docs/verification/inflate-oracle.md).
//!
//! The Lean side is a subprocess: `LEAN_ZIP_INFLATE=<path to lzinflate>`, built from the pinned
//! lean-zip commit around `Zip.Native.Inflate.inflate`, the production decoder whose accept set is
//! proved equal to the reference decoder behind `inflate_deflateRaw`. It links no system zlib.

pub mod corpus;
pub mod hostile;
pub mod sides;
