#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
//! Persistent, bitmap-indexed radix directory over signed `(tx, ty)` tile coordinates.
//!
//! Provenance: moved and generalized over `V` from PhotoCraft `crates/raster/src/tile_dir.rs`
//! (`934bdc745033786975366063d0a55f8e6d552f51`).
//!
//! Signed coordinates are biased by `0x8000_0000` and packed `x`-first (`tx` in the upper 32 bits,
//! `ty` in the lower 32 bits); most-significant 4-bit nibbles select branches, so a depth-first
//! walk yields entries in ascending `(tx, ty)` order without sorting. Leaves terminate early.
//! Paths have at most 16 branches, independent of input, and `Arc::make_mut` copies only shared
//! paths, never untouched subtrees.

use std::fmt;
use std::sync::Arc;

use crate::{TileCoord, TileDirectory};

/// Persistent sparse radix-tree tile directory keyed by signed `(tx, ty)` coordinates.
///
/// Cloning is `O(1)` (a single `Option<Arc<_>>` clone). Mutations copy only the `Arc` nodes on the
/// root-to-leaf path of modified coordinates (`<= 16` levels). Iteration is in ascending
/// lexicographic `(tx, ty)` order (`tx` first, then `ty`), matching PhotoCraft's `TileCoord::Ord`
/// and `BTreeMap<(i32, i32), V>`.
pub struct SparseTileDir<V> {
    root: Option<Arc<Node<V>>>,
    len: usize,
}

#[derive(Clone)]
enum Node<V> {
    Leaf(TileCoord, V),
    Branch { bitmap: u16, children: Vec<Arc<Node<V>>> },
}

impl<V> Clone for SparseTileDir<V> {
    fn clone(&self) -> Self {
        Self { root: self.root.clone(), len: self.len }
    }
}

impl<V> Default for SparseTileDir<V> {
    fn default() -> Self {
        Self { root: None, len: 0 }
    }
}

impl<V: fmt::Debug> fmt::Debug for SparseTileDir<V> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_map().entries(self.iter()).finish()
    }
}

fn packed((tx, ty): TileCoord) -> u64 {
    (u64::from((tx as u32) ^ 0x8000_0000) << 32) | u64::from((ty as u32) ^ 0x8000_0000)
}

fn bit(key: u64, shift: u32) -> u16 {
    1 << ((key >> shift) & 15)
}

fn rank(bitmap: u16, b: u16) -> usize {
    (bitmap & (b - 1)).count_ones() as usize
}

impl<V> Node<V> {
    fn get(&self, coord: TileCoord, key: u64, shift: u32) -> Option<&V> {
        match self {
            Self::Leaf(stored, value) => (*stored == coord).then_some(value),
            Self::Branch { bitmap, children } => {
                let b = bit(key, shift);
                if bitmap & b == 0 {
                    return None;
                }
                children.get(rank(*bitmap, b))?.get(coord, key, shift.saturating_sub(4))
            }
        }
    }

    fn shared_with(left: &Arc<Self>, right: &Arc<Self>) -> bool {
        if Arc::ptr_eq(left, right) {
            return true;
        }
        match (left.as_ref(), right.as_ref()) {
            (Self::Branch { bitmap: left_map, children: left_children }, Self::Branch { bitmap: right_map, children: right_children }) => {
                let common = *left_map & *right_map;
                if common == 0 {
                    return false;
                }
                for nibble in 0..16u32 {
                    let b = 1u16 << nibble;
                    if common & b != 0 {
                        let l_idx = rank(*left_map, b);
                        let r_idx = rank(*right_map, b);
                        if let (Some(l_child), Some(r_child)) = (left_children.get(l_idx), right_children.get(r_idx))
                            && Self::shared_with(l_child, r_child)
                        {
                            return true;
                        }
                    }
                }
                false
            }
            _ => false,
        }
    }
}

impl<V: Clone> Node<V> {
    fn get_mut(node: &mut Arc<Self>, coord: TileCoord, key: u64, shift: u32) -> Option<&mut V> {
        match Arc::make_mut(node) {
            Self::Leaf(stored, value) => (*stored == coord).then_some(value),
            Self::Branch { bitmap, children } => {
                let b = bit(key, shift);
                if *bitmap & b == 0 {
                    return None;
                }
                Self::get_mut(children.get_mut(rank(*bitmap, b))?, coord, key, shift.saturating_sub(4))
            }
        }
    }

    /// A branch's bitmap and compact children always have matching cardinality.
    /// Rank is therefore in bounds after inserting a missing child. Distinct full
    /// keys must diverge by the final nibble, bounding splitting to 16 levels.
    fn entry<'a>(node: &'a mut Arc<Self>, coord: TileCoord, key: u64, shift: u32, create: &mut impl FnMut() -> V) -> (&'a mut V, bool) {
        if let Self::Leaf(stored, _) = node.as_ref()
            && *stored != coord
        {
            *node = Arc::new(Self::Branch { bitmap: bit(packed(*stored), shift), children: vec![Arc::clone(node)] });
        }
        match Arc::make_mut(node) {
            Self::Leaf(_, value) => (value, false),
            Self::Branch { bitmap, children } => {
                let b = bit(key, shift);
                let index = rank(*bitmap, b);
                if *bitmap & b == 0 {
                    children.insert(index, Arc::new(Self::Leaf(coord, create())));
                    *bitmap |= b;
                    let (value, _) = Self::entry(&mut children[index], coord, key, shift.saturating_sub(4), create);
                    (value, true)
                } else {
                    Self::entry(&mut children[index], coord, key, shift.saturating_sub(4), create)
                }
            }
        }
    }

    fn insert(node: &mut Arc<Self>, coord: TileCoord, key: u64, shift: u32, value: V) -> (Option<V>, bool) {
        if let Self::Leaf(stored, _) = node.as_ref()
            && *stored != coord
        {
            *node = Arc::new(Self::Branch { bitmap: bit(packed(*stored), shift), children: vec![Arc::clone(node)] });
        }
        match Arc::make_mut(node) {
            Self::Leaf(_, stored) => (Some(std::mem::replace(stored, value)), false),
            Self::Branch { bitmap, children } => {
                let b = bit(key, shift);
                let index = rank(*bitmap, b);
                if *bitmap & b == 0 {
                    children.insert(index, Arc::new(Self::Leaf(coord, value)));
                    *bitmap |= b;
                    (None, true)
                } else if let Some(child) = children.get_mut(index) {
                    Self::insert(child, coord, key, shift.saturating_sub(4), value)
                } else {
                    (None, false)
                }
            }
        }
    }

    fn remove(node: &mut Arc<Self>, coord: TileCoord, key: u64, shift: u32) -> (Option<V>, bool) {
        match Arc::make_mut(node) {
            Self::Leaf(stored, value) => {
                let matched = *stored == coord;
                (matched.then(|| value.clone()), matched)
            }
            Self::Branch { bitmap, children } => {
                let b = bit(key, shift);
                if *bitmap & b == 0 {
                    return (None, false);
                }
                let index = rank(*bitmap, b);
                let Some(child) = children.get_mut(index) else {
                    return (None, false);
                };
                let (removed, empty) = Self::remove(child, coord, key, shift.saturating_sub(4));
                if empty {
                    children.remove(index);
                    *bitmap &= !b;
                }
                let empty = children.is_empty();
                if let [only_child] = children.as_slice()
                    && matches!(only_child.as_ref(), Self::Leaf(..))
                {
                    *node = Arc::clone(only_child);
                }
                (removed, empty)
            }
        }
    }

    fn for_each_mut(node: &mut Arc<Self>, f: &mut impl FnMut(TileCoord, &mut V)) {
        match Arc::make_mut(node) {
            Self::Leaf(coord, value) => f(*coord, value),
            Self::Branch { children, .. } => {
                for child in children {
                    Self::for_each_mut(child, f);
                }
            }
        }
    }
}

impl<V: PartialEq> PartialEq for Node<V> {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Leaf(coord, value), Self::Leaf(other_coord, other_value)) => coord == other_coord && value == other_value,
            (Self::Branch { bitmap, children }, Self::Branch { bitmap: other_bitmap, children: other_children }) => {
                bitmap == other_bitmap && children.iter().zip(other_children).all(|(left, right)| Arc::ptr_eq(left, right) || left == right)
            }
            _ => false,
        }
    }
}

impl<V: Eq> Eq for Node<V> {}

impl<V: PartialEq> PartialEq for SparseTileDir<V> {
    fn eq(&self, other: &Self) -> bool {
        self.len == other.len
            && match (&self.root, &other.root) {
                (None, None) => true,
                (Some(left), Some(right)) => Arc::ptr_eq(left, right) || left == right,
                _ => false,
            }
    }
}

impl<V: Eq> Eq for SparseTileDir<V> {}

impl<V> SparseTileDir<V> {
    /// Creates an empty sparse tile directory.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of tiles currently stored in the directory.
    #[must_use]
    pub fn len(&self) -> usize {
        self.len
    }

    /// Returns `true` if the directory holds no tiles.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Borrows the tile at `(tx, ty)`, if present.
    #[must_use]
    pub fn get(&self, tx: i32, ty: i32) -> Option<&V> {
        let coord = (tx, ty);
        self.root.as_ref()?.get(coord, packed(coord), 60)
    }

    /// Returns `true` if a tile is stored at `(tx, ty)`.
    #[must_use]
    pub fn contains_key(&self, tx: i32, ty: i32) -> bool {
        self.get(tx, ty).is_some()
    }

    /// Returns `true` when both directories share the same root `Arc` (or are both empty).
    #[must_use]
    pub fn ptr_eq(&self, other: &Self) -> bool {
        match (&self.root, &other.root) {
            (None, None) => true,
            (Some(left), Some(right)) => Arc::ptr_eq(left, right),
            _ => false,
        }
    }

    /// Returns `true` when `self` and `other` share their root handle or any internal branch/leaf
    /// `Arc` node.
    #[must_use]
    pub fn shared_with(&self, other: &Self) -> bool {
        match (&self.root, &other.root) {
            (None, None) => true,
            (Some(left), Some(right)) => Node::shared_with(left, right),
            _ => false,
        }
    }

    /// Iterates over `((tx, ty), &V)` in ascending lexicographic `(tx, ty)` order (`tx` first,
    /// then `ty`).
    #[must_use]
    pub fn iter(&self) -> SparseIter<'_, V> {
        SparseIter { next: self.root.as_deref(), parents: std::array::from_fn(|_| [].iter()), depth: 0, remaining: self.len }
    }

    /// Iterates over `(tx, ty)` coordinates in ascending lexicographic `(tx, ty)` order.
    pub fn keys(&self) -> impl Iterator<Item = TileCoord> + '_ {
        self.iter().map(|(coord, _)| coord)
    }

    /// Iterates over tile values `&V` in ascending lexicographic `(tx, ty)` order.
    pub fn values(&self) -> impl Iterator<Item = &V> + '_ {
        self.iter().map(|(_, value)| value)
    }
}

impl<V: Clone> SparseTileDir<V> {
    /// Mutably borrows the tile at `(tx, ty)`, path-copying only the nodes along the path to
    /// `(tx, ty)`. Returns `None` without modifying the tree if `(tx, ty)` is absent.
    pub fn get_mut(&mut self, tx: i32, ty: i32) -> Option<&mut V> {
        self.get(tx, ty)?;
        let coord = (tx, ty);
        Node::get_mut(self.root.as_mut()?, coord, packed(coord), 60)
    }

    /// Mutably borrows the tile at `(tx, ty)`, inserting `create()` if absent and path-copying
    /// only the nodes along the path to `(tx, ty)`.
    pub fn get_or_insert_with(&mut self, tx: i32, ty: i32, mut create: impl FnMut() -> V) -> &mut V {
        let coord = (tx, ty);
        let root = self.root.get_or_insert_with(|| {
            self.len += 1;
            Arc::new(Node::Leaf(coord, create()))
        });
        let (value, inserted) = Node::entry(root, coord, packed(coord), 60, &mut create);
        self.len += usize::from(inserted);
        value
    }

    /// Inserts `value` at `(tx, ty)` and returns the previous value if one was present.
    pub fn insert(&mut self, tx: i32, ty: i32, value: V) -> Option<V> {
        let coord = (tx, ty);
        let Some(root) = self.root.as_mut() else {
            self.root = Some(Arc::new(Node::Leaf(coord, value)));
            self.len = 1;
            return None;
        };
        let (old, inserted) = Node::insert(root, coord, packed(coord), 60, value);
        self.len += usize::from(inserted);
        old
    }

    /// Removes and returns the tile at `(tx, ty)`, collapsing single-leaf branches back into
    /// canonical form. Returns `None` without unsharing if `(tx, ty)` is absent.
    pub fn remove(&mut self, tx: i32, ty: i32) -> Option<V> {
        self.get(tx, ty)?;
        let coord = (tx, ty);
        let (removed, empty) = Node::remove(self.root.as_mut()?, coord, packed(coord), 60);
        self.len -= usize::from(removed.is_some());
        if empty {
            self.root = None;
        }
        removed
    }

    /// Retains only entries for which `keep((tx, ty), &value)` returns `true`.
    ///
    /// Only paths to removed entries are path-copied; untouched branches (and the root if all
    /// entries are retained) remain `Arc`-shared.
    pub fn retain(&mut self, mut keep: impl FnMut(TileCoord, &V) -> bool) {
        let mut removed = Vec::new();
        for (coord, value) in self.iter() {
            if !keep(coord, value) {
                removed.push(coord);
            }
        }
        for (tx, ty) in removed {
            self.remove(tx, ty);
        }
    }

    /// Calls `f((tx, ty), &mut value)` for each entry in ascending `(tx, ty)` order, path-copying
    /// nodes along visited paths via `Arc::make_mut`.
    pub fn for_each_mut(&mut self, mut f: impl FnMut(TileCoord, &mut V)) {
        if let Some(root) = self.root.as_mut() {
            Node::for_each_mut(root, &mut f);
        }
    }
}

impl<V: Clone> TileDirectory<V> for SparseTileDir<V> {
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

/// Iterator over occupied `((tx, ty), &V)` entries of a [`SparseTileDir`] in ascending
/// lexicographic `(tx, ty)` order.
pub struct SparseIter<'a, V> {
    next: Option<&'a Node<V>>,
    parents: [std::slice::Iter<'a, Arc<Node<V>>>; 16],
    depth: usize,
    remaining: usize,
}

impl<'a, V> Iterator for SparseIter<'a, V> {
    type Item = (TileCoord, &'a V);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            match self.next.take() {
                Some(Node::Leaf(coord, value)) => {
                    self.remaining = self.remaining.saturating_sub(1);
                    return Some((*coord, value));
                }
                Some(Node::Branch { children, .. }) => {
                    *self.parents.get_mut(self.depth)? = children.iter();
                    self.depth += 1;
                }
                None => {}
            }
            while self.depth > 0 {
                self.next = self.parents.get_mut(self.depth - 1)?.next().map(Arc::as_ref);
                if self.next.is_some() {
                    break;
                }
                self.depth -= 1;
            }
            self.next?;
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.remaining, Some(self.remaining))
    }
}

impl<V> ExactSizeIterator for SparseIter<'_, V> {}

impl<'a, V> IntoIterator for &'a SparseTileDir<V> {
    type Item = (TileCoord, &'a V);
    type IntoIter = SparseIter<'a, V>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<V: Clone> FromIterator<(TileCoord, V)> for SparseTileDir<V> {
    fn from_iter<I: IntoIterator<Item = (TileCoord, V)>>(iter: I) -> Self {
        let mut directory = Self::new();
        for ((tx, ty), value) in iter {
            directory.insert(tx, ty, value);
        }
        directory
    }
}
