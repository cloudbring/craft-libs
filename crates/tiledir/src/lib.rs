#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
//! Persistent (structurally shared, `O(1)`-clone) tile directories for undo history and
//! copy-on-write tiled canvases.
//!
//! Unifies two first-party implementations:
//! - [`SparseTileDir`], derived from PhotoCraft `crates/raster/src/tile_dir.rs`
//!   (`934bdc745033786975366063d0a55f8e6d552f51`): a persistent 16-way bitmap-indexed radix tree
//!   keyed by biased `(tx, ty)` `i32` coordinates over an unbounded plane.
//! - [`DenseTileDir`], derived from TextureCraft `crates/texel/src/tile.rs`
//!   (`6aa37518d3c031e1f64869d74642871f36104ddd`): a persistent chunked vector (`Arc` outer table
//!   of 64-slot `Arc` chunks) over a bounded `tiles_x × tiles_y` grid.
//!
//! Both implement [`TileDirectory<V>`], store `V` directly (applications typically pass `Arc<Tile>`
//! so tile payloads are independently copy-on-write), clone in `O(1)` via a single top-level `Arc`
//! bump, short-circuit [`PartialEq`] on shared `Arc` pointers before comparing elements, and use
//! `Arc::make_mut` path-copying on mutations (`insert`, `remove`, `get_mut`, `retain`,
//! `for_each_mut`) so untouched branches and chunks stay shared across snapshots.
//!
//! # Iteration order
//!
//! [`TileDirectory::iter`], [`SparseTileDir::iter`], and [`DenseTileDir::iter`] yield
//! `((tx, ty), &V)` in ascending lexicographic `(tx, ty)` order (`tx` first, then `ty`), matching
//! PhotoCraft's `TileCoord::Ord` and `BTreeMap<(i32, i32), V>`. For callers that prefer linear
//! row-major order (`ty` first, then `tx`, walking contiguous chunks directly), [`DenseTileDir`]
//! also provides [`DenseTileDir::iter_row_major`] and [`DenseTileDir::slots`].

mod dense;
mod sparse;

pub use dense::{DEFAULT_CHUNK_SIZE, DenseIter, DenseRowMajorIter, DenseTileDir};
pub use sparse::{SparseIter, SparseTileDir};

/// A 2D tile coordinate `(tx, ty)` in signed tile-grid indices.
pub type TileCoord = (i32, i32);

/// Persistent 2D tile directory supporting `O(1)` snapshot clones and path-copying mutations.
///
/// Iteration yields `((tx, ty), &V)` in ascending lexicographic `(tx, ty)` order (`tx` first,
/// then `ty`), matching PhotoCraft's `TileCoord::Ord` and `BTreeMap<(i32, i32), V>`.
pub trait TileDirectory<V>: Clone {
    /// Number of occupied tiles currently stored in the directory.
    fn len(&self) -> usize;

    /// Returns `true` when the directory holds no occupied tiles.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Borrows the tile at `(tx, ty)`, if present.
    fn get(&self, tx: i32, ty: i32) -> Option<&V>;

    /// Returns `true` if a tile is stored at `(tx, ty)`.
    fn contains_key(&self, tx: i32, ty: i32) -> bool {
        self.get(tx, ty).is_some()
    }

    /// Mutably borrows the tile at `(tx, ty)`, path-copying only the nodes or chunk along the
    /// path to `(tx, ty)`. Absent or out-of-bounds coordinates return `None` without unsharing.
    fn get_mut(&mut self, tx: i32, ty: i32) -> Option<&mut V>;

    /// Inserts `value` at `(tx, ty)` and returns the previous value if one was present.
    ///
    /// For bounded directories ([`DenseTileDir`]), out-of-bounds coordinates are ignored and
    /// return `None` without unsharing the directory.
    fn insert(&mut self, tx: i32, ty: i32, value: V) -> Option<V>;

    /// Removes and returns the tile at `(tx, ty)` if present.
    ///
    /// Removing an absent or out-of-bounds coordinate is a no-op that preserves structural
    /// sharing.
    fn remove(&mut self, tx: i32, ty: i32) -> Option<V>;

    /// Iterates over occupied `((tx, ty), &V)` entries in ascending lexicographic `(tx, ty)`
    /// order (`tx` first, then `ty`), matching PhotoCraft's `TileCoord::Ord`.
    fn iter<'a>(&'a self) -> impl Iterator<Item = (TileCoord, &'a V)>
    where
        V: 'a;

    /// Returns `true` when both directories share the same root handle (`O(1)` pointer check).
    fn ptr_eq(&self, other: &Self) -> bool;

    /// Returns `true` when `self` and `other` share their root handle or any internal branch or
    /// chunk allocation.
    fn shared_with(&self, other: &Self) -> bool;

    /// Retains only occupied tiles for which `keep((tx, ty), &value)` returns `true`.
    ///
    /// Uses path-copy semantics: `Arc::make_mut` is invoked only along paths or chunks where at
    /// least one entry is removed. If all entries are kept, the root handle stays shared.
    fn retain(&mut self, keep: impl FnMut(TileCoord, &V) -> bool);

    /// Applies `f((tx, ty), &mut value)` to every occupied tile in ascending `(tx, ty)` order.
    ///
    /// Uses path-copy semantics: `Arc::make_mut` is invoked only on nodes or chunks containing
    /// occupied tiles. Empty directories and empty chunks remain shared.
    fn for_each_mut(&mut self, f: impl FnMut(TileCoord, &mut V));
}
