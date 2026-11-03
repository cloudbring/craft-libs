#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
//! Persistent, chunked dense tile directory for fixed `tiles_x × tiles_y` grids.
//!
//! Provenance: moved and generalized over `V` from TextureCraft `crates/texel/src/tile.rs`
//! (`6aa37518d3c031e1f64869d74642871f36104ddd`).
//!
//! Cloning shares the entire directory with one `Arc` bump, independent of tile count. The first
//! write after sharing copies the outer chunk-handle vector and at most `CHUNK` (default 64) slot
//! handles; untouched chunks and tile payloads remain shared.

use std::fmt;
use std::sync::Arc;

use crate::{TileCoord, TileDirectory};

/// Default number of tile slots per persistent chunk in [`DenseTileDir`].
pub const DEFAULT_CHUNK_SIZE: usize = 64;

/// Persistent chunked tile directory over a fixed `tiles_x × tiles_y` grid (`0 <= tx < tiles_x`,
/// `0 <= ty < tiles_y`).
///
/// Slots are stored in row-major order (`ty * tiles_x + tx`) partitioned into `Arc<[Option<V>]>`
/// chunks of `CHUNK` slots (default [`DEFAULT_CHUNK_SIZE`] = 64).
///
/// [`DenseTileDir::iter`] (and [`TileDirectory::iter`]) yields occupied `((tx, ty), &V)` entries
/// in ascending lexicographic `(tx, ty)` order (`tx` first, then `ty`), matching
/// [`crate::SparseTileDir`] and PhotoCraft's `TileCoord::Ord`. Use
/// [`DenseTileDir::iter_row_major`] to walk occupied entries in linear `(ty, tx)` chunk order, or
/// [`DenseTileDir::slots`] to inspect all `Option<V>` slots linearly.
pub struct DenseTileDir<V, const CHUNK: usize = DEFAULT_CHUNK_SIZE> {
    chunks: Arc<Vec<Arc<[Option<V>]>>>,
    tiles_x: u32,
    tiles_y: u32,
    len: usize,
}

impl<V, const CHUNK: usize> Clone for DenseTileDir<V, CHUNK> {
    fn clone(&self) -> Self {
        Self { chunks: Arc::clone(&self.chunks), tiles_x: self.tiles_x, tiles_y: self.tiles_y, len: self.len }
    }
}

impl<V, const CHUNK: usize> Default for DenseTileDir<V, CHUNK> {
    fn default() -> Self {
        Self::with_chunk_size(0, 0)
    }
}

impl<V> DenseTileDir<V, DEFAULT_CHUNK_SIZE> {
    /// Creates an empty `tiles_x × tiles_y` dense tile directory with 64-slot chunks.
    #[must_use]
    pub fn new(tiles_x: u32, tiles_y: u32) -> Self {
        Self::with_chunk_size(tiles_x, tiles_y)
    }
}

impl<V: fmt::Debug, const CHUNK: usize> fmt::Debug for DenseTileDir<V, CHUNK> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_list().entries(self.slots()).finish()
    }
}

impl<V: PartialEq, const CHUNK: usize> PartialEq for DenseTileDir<V, CHUNK> {
    fn eq(&self, other: &Self) -> bool {
        self.tiles_x == other.tiles_x
            && self.tiles_y == other.tiles_y
            && self.len == other.len
            && (Arc::ptr_eq(&self.chunks, &other.chunks)
                || self.chunks.iter().zip(other.chunks.iter()).all(|(left, right)| Arc::ptr_eq(left, right) || left == right))
    }
}

impl<V: Eq, const CHUNK: usize> Eq for DenseTileDir<V, CHUNK> {}

impl<V, const CHUNK: usize> DenseTileDir<V, CHUNK> {
    const fn effective_chunk_size() -> usize {
        if CHUNK == 0 { DEFAULT_CHUNK_SIZE } else { CHUNK }
    }

    /// Creates an empty `tiles_x × tiles_y` dense tile directory with chunk size `CHUNK`.
    #[must_use]
    pub fn with_chunk_size(tiles_x: u32, tiles_y: u32) -> Self {
        let total = (tiles_x as usize).saturating_mul(tiles_y as usize);
        let step = Self::effective_chunk_size();
        let chunks = (0..total)
            .step_by(step)
            .map(|start| {
                let count = step.min(total - start);
                let mut vec = Vec::with_capacity(count);
                vec.resize_with(count, || None);
                Arc::from(vec)
            })
            .collect();
        Self { chunks: Arc::new(chunks), tiles_x, tiles_y, len: 0 }
    }

    /// Grid width in tiles.
    #[must_use]
    pub fn tiles_x(&self) -> u32 {
        self.tiles_x
    }

    /// Grid height in tiles.
    #[must_use]
    pub fn tiles_y(&self) -> u32 {
        self.tiles_y
    }

    /// Grid dimensions `(tiles_x, tiles_y)`.
    #[must_use]
    pub fn dimensions(&self) -> (u32, u32) {
        (self.tiles_x, self.tiles_y)
    }

    /// Total number of addressable tile slots (`tiles_x * tiles_y`).
    #[must_use]
    pub fn capacity(&self) -> usize {
        (self.tiles_x as usize).saturating_mul(self.tiles_y as usize)
    }

    /// Number of occupied (`Some`) tiles currently stored in the directory.
    #[must_use]
    pub fn len(&self) -> usize {
        self.len
    }

    /// Returns `true` when no tiles are currently occupied.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Number of chunks in the outer directory table.
    #[must_use]
    pub fn chunk_count(&self) -> usize {
        self.chunks.len()
    }

    /// Number of slots in chunk `chunk_index`, if in range.
    #[must_use]
    pub fn chunk_len(&self, chunk_index: usize) -> Option<usize> {
        self.chunks.get(chunk_index).map(|chunk| chunk.len())
    }

    /// Returns `true` if `chunk_index` exists in both directories and shares the same `Arc` slice.
    #[must_use]
    pub fn chunk_ptr_eq(&self, other: &Self, chunk_index: usize) -> bool {
        match (self.chunks.get(chunk_index), other.chunks.get(chunk_index)) {
            (Some(left), Some(right)) => Arc::ptr_eq(left, right),
            _ => false,
        }
    }

    /// Returns `true` if `(tx, ty)` lies within `0..tiles_x` and `0..tiles_y`.
    #[must_use]
    pub fn in_bounds(&self, tx: i32, ty: i32) -> bool {
        self.tile_index(tx, ty).is_some()
    }

    /// Converts `(tx, ty)` into its row-major slot index `ty * tiles_x + tx` if in bounds.
    #[must_use]
    pub fn tile_index(&self, tx: i32, ty: i32) -> Option<usize> {
        let tx = u32::try_from(tx).ok()?;
        let ty = u32::try_from(ty).ok()?;
        if tx >= self.tiles_x || ty >= self.tiles_y {
            return None;
        }
        (ty as usize).checked_mul(self.tiles_x as usize)?.checked_add(tx as usize)
    }

    fn slot_at(&self, index: usize) -> Option<&Option<V>> {
        let step = Self::effective_chunk_size();
        self.chunks.get(index / step)?.get(index % step)
    }

    /// Borrows the tile at `(tx, ty)`, if in bounds and occupied.
    #[must_use]
    pub fn get(&self, tx: i32, ty: i32) -> Option<&V> {
        let index = self.tile_index(tx, ty)?;
        self.slot_at(index)?.as_ref()
    }

    /// Returns `true` if `(tx, ty)` is in bounds and occupied.
    #[must_use]
    pub fn contains_key(&self, tx: i32, ty: i32) -> bool {
        self.get(tx, ty).is_some()
    }

    /// Returns `true` when both directories share the same outer chunk-vector `Arc`.
    #[must_use]
    pub fn ptr_eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.chunks, &other.chunks)
    }

    /// Returns `true` when `self` and `other` share the outer chunk table or at least one inner
    /// chunk `Arc`.
    #[must_use]
    pub fn shared_with(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.chunks, &other.chunks) || self.chunks.iter().zip(other.chunks.iter()).any(|(left, right)| Arc::ptr_eq(left, right))
    }

    /// Iterates over all `tiles_x * tiles_y` `Option<V>` slots in row-major order.
    pub fn slots(&self) -> impl Iterator<Item = &Option<V>> + '_ {
        self.chunks.iter().flat_map(|chunk| chunk.iter())
    }

    /// Iterates over occupied `((tx, ty), &V)` entries in ascending lexicographic `(tx, ty)` order
    /// (`tx` first, then `ty`), matching [`crate::SparseTileDir::iter`] and PhotoCraft's
    /// `TileCoord::Ord`.
    #[must_use]
    pub fn iter(&self) -> DenseIter<'_, V, CHUNK> {
        DenseIter { dir: self, tx: 0, ty: 0, remaining: self.len }
    }

    /// Iterates over occupied `((tx, ty), &V)` entries in row-major `(ty, tx)` order (`ty` first,
    /// then `tx`), walking contiguous chunks linearly as in TextureCraft's `TileImage::allocated`.
    #[must_use]
    pub fn iter_row_major(&self) -> DenseRowMajorIter<'_, V, CHUNK> {
        DenseRowMajorIter { chunks: self.chunks.iter(), current_chunk: [].iter(), index: 0, tiles_x: self.tiles_x, remaining: self.len }
    }

    /// Iterates over occupied `(tx, ty)` coordinates in ascending lexicographic `(tx, ty)` order.
    pub fn keys(&self) -> impl Iterator<Item = TileCoord> + '_ {
        self.iter().map(|(coord, _)| coord)
    }

    /// Iterates over occupied tile values `&V` in ascending lexicographic `(tx, ty)` order.
    pub fn values(&self) -> impl Iterator<Item = &V> + '_ {
        self.iter().map(|(_, value)| value)
    }
}

impl<V: Clone, const CHUNK: usize> DenseTileDir<V, CHUNK> {
    fn slot_mut(&mut self, index: usize) -> Option<&mut Option<V>> {
        self.slot_at(index)?;
        let step = Self::effective_chunk_size();
        let chunk = Arc::make_mut(&mut self.chunks).get_mut(index / step)?;
        Arc::make_mut(chunk).get_mut(index % step)
    }

    /// Mutably borrows the tile at `(tx, ty)`, unsharing only the outer chunk vector and the chunk
    /// containing `(tx, ty)`. Returns `None` without unsharing if `(tx, ty)` is absent or out of
    /// bounds.
    pub fn get_mut(&mut self, tx: i32, ty: i32) -> Option<&mut V> {
        let index = self.tile_index(tx, ty)?;
        if self.slot_at(index)?.is_none() {
            return None;
        }
        self.slot_mut(index)?.as_mut()
    }

    /// Mutably borrows the tile at `(tx, ty)`, inserting `create()` if the in-bounds slot is
    /// currently `None`. Returns `None` without unsharing if `(tx, ty)` is out of bounds.
    pub fn get_or_insert_with(&mut self, tx: i32, ty: i32, create: impl FnOnce() -> V) -> Option<&mut V> {
        let index = self.tile_index(tx, ty)?;
        let is_new = self.slot_at(index)?.is_none();
        let step = Self::effective_chunk_size();
        let chunk = Arc::make_mut(&mut self.chunks).get_mut(index / step)?;
        let slot = Arc::make_mut(chunk).get_mut(index % step)?;
        let value = slot.get_or_insert_with(create);
        self.len += usize::from(is_new);
        Some(value)
    }

    /// Inserts `value` at `(tx, ty)` and returns the previous value if one was present.
    ///
    /// Returns `None` without unsharing if `(tx, ty)` is out of bounds.
    pub fn insert(&mut self, tx: i32, ty: i32, value: V) -> Option<V> {
        let index = self.tile_index(tx, ty)?;
        let slot = self.slot_mut(index)?;
        let old = slot.replace(value);
        if old.is_none() {
            self.len += 1;
        }
        old
    }

    /// Removes and returns the tile at `(tx, ty)` if in bounds and occupied.
    ///
    /// Returns `None` without unsharing if `(tx, ty)` is out of bounds or already `None`.
    pub fn remove(&mut self, tx: i32, ty: i32) -> Option<V> {
        let index = self.tile_index(tx, ty)?;
        if self.slot_at(index)?.is_none() {
            return None;
        }
        let removed = self.slot_mut(index)?.take();
        self.len -= usize::from(removed.is_some());
        removed
    }

    /// Retains only occupied tiles for which `keep((tx, ty), &value)` returns `true`.
    ///
    /// Visits entries in ascending `(tx, ty)` order and unshares only chunks from which at least
    /// one tile is removed.
    pub fn retain(&mut self, mut keep: impl FnMut(TileCoord, &V) -> bool) {
        if self.len == 0 {
            return;
        }
        for tx in 0..self.tiles_x {
            for ty in 0..self.tiles_y {
                let coord = (tx as i32, ty as i32);
                let should_remove = self.get(coord.0, coord.1).is_some_and(|value| !keep(coord, value));
                if should_remove {
                    self.remove(coord.0, coord.1);
                }
            }
        }
    }

    /// Calls `f((tx, ty), &mut value)` for each occupied tile in ascending `(tx, ty)` order,
    /// unsharing only chunks that contain at least one occupied tile.
    pub fn for_each_mut(&mut self, mut f: impl FnMut(TileCoord, &mut V)) {
        if self.len == 0 {
            return;
        }
        for tx in 0..self.tiles_x {
            for ty in 0..self.tiles_y {
                let coord = (tx as i32, ty as i32);
                if let Some(value) = self.get_mut(coord.0, coord.1) {
                    f(coord, value);
                }
            }
        }
    }
}

impl<V: Clone, const CHUNK: usize> TileDirectory<V> for DenseTileDir<V, CHUNK> {
    fn len(&self) -> usize {
        Self::len(self)
    }

    fn is_empty(&self) -> bool {
        Self::is_empty(self)
    }

    fn get(&self, tx: i32, ty: i32) -> Option<&V> {
        Self::get(self, tx, ty)
    }

    fn contains_key(&self, tx: i32, ty: i32) -> bool {
        Self::contains_key(self, tx, ty)
    }

    fn get_mut(&mut self, tx: i32, ty: i32) -> Option<&mut V> {
        Self::get_mut(self, tx, ty)
    }

    fn insert(&mut self, tx: i32, ty: i32, value: V) -> Option<V> {
        Self::insert(self, tx, ty, value)
    }

    fn remove(&mut self, tx: i32, ty: i32) -> Option<V> {
        Self::remove(self, tx, ty)
    }

    fn iter<'a>(&'a self) -> impl Iterator<Item = (TileCoord, &'a V)>
    where
        V: 'a,
    {
        Self::iter(self)
    }

    fn ptr_eq(&self, other: &Self) -> bool {
        Self::ptr_eq(self, other)
    }

    fn shared_with(&self, other: &Self) -> bool {
        Self::shared_with(self, other)
    }

    fn retain(&mut self, keep: impl FnMut(TileCoord, &V) -> bool) {
        Self::retain(self, keep);
    }

    fn for_each_mut(&mut self, f: impl FnMut(TileCoord, &mut V)) {
        Self::for_each_mut(self, f);
    }
}

/// Iterator over occupied `((tx, ty), &V)` entries of a [`DenseTileDir`] in ascending
/// lexicographic `(tx, ty)` order (`tx` first, then `ty`).
pub struct DenseIter<'a, V, const CHUNK: usize = DEFAULT_CHUNK_SIZE> {
    dir: &'a DenseTileDir<V, CHUNK>,
    tx: u32,
    ty: u32,
    remaining: usize,
}

impl<'a, V, const CHUNK: usize> Iterator for DenseIter<'a, V, CHUNK> {
    type Item = (TileCoord, &'a V);

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }
        while self.tx < self.dir.tiles_x {
            while self.ty < self.dir.tiles_y {
                let (tx, ty) = (self.tx as i32, self.ty as i32);
                self.ty += 1;
                if let Some(value) = self.dir.get(tx, ty) {
                    self.remaining -= 1;
                    return Some(((tx, ty), value));
                }
            }
            self.ty = 0;
            self.tx += 1;
        }
        None
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.remaining, Some(self.remaining))
    }
}

impl<V, const CHUNK: usize> ExactSizeIterator for DenseIter<'_, V, CHUNK> {}

/// Iterator over occupied `((tx, ty), &V)` entries of a [`DenseTileDir`] in row-major `(ty, tx)`
/// order (`ty` first, then `tx`).
pub struct DenseRowMajorIter<'a, V, const CHUNK: usize = DEFAULT_CHUNK_SIZE> {
    chunks: std::slice::Iter<'a, Arc<[Option<V>]>>,
    current_chunk: std::slice::Iter<'a, Option<V>>,
    index: usize,
    tiles_x: u32,
    remaining: usize,
}

impl<'a, V, const CHUNK: usize> Iterator for DenseRowMajorIter<'a, V, CHUNK> {
    type Item = (TileCoord, &'a V);

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 || self.tiles_x == 0 {
            return None;
        }
        loop {
            if let Some(slot) = self.current_chunk.next() {
                let current = self.index;
                self.index += 1;
                if let Some(value) = slot.as_ref() {
                    self.remaining -= 1;
                    let cols = self.tiles_x as usize;
                    let tx = (current % cols) as i32;
                    let ty = (current / cols) as i32;
                    return Some(((tx, ty), value));
                }
            } else {
                self.current_chunk = self.chunks.next()?.iter();
            }
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.remaining, Some(self.remaining))
    }
}

impl<V, const CHUNK: usize> ExactSizeIterator for DenseRowMajorIter<'_, V, CHUNK> {}

impl<'a, V, const CHUNK: usize> IntoIterator for &'a DenseTileDir<V, CHUNK> {
    type Item = (TileCoord, &'a V);
    type IntoIter = DenseIter<'a, V, CHUNK>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}
