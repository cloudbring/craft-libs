use std::collections::HashMap;
use std::hash::Hash;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Debug)]
struct BudgetState {
    limit: usize,
    used: AtomicUsize,
}

/// A shareable, thread-safe reservation pool. It does not evict from other caches.
#[derive(Clone, Debug)]
pub struct Budget(Arc<BudgetState>);
impl Budget {
    /// Create a budget with a fixed maximum cost.
    pub fn new(limit: usize) -> Self {
        Self(Arc::new(BudgetState { limit, used: AtomicUsize::new(0) }))
    }
    /// Maximum total reserved cost.
    pub fn limit(&self) -> usize {
        self.0.limit
    }
    /// Current total reserved cost across all caches sharing this budget.
    pub fn used(&self) -> usize {
        self.0.used.load(Ordering::Relaxed)
    }
    #[allow(deprecated)]
    fn reserve(&self, cost: usize) -> bool {
        self.0.used.fetch_update(Ordering::AcqRel, Ordering::Relaxed, |used| used.checked_add(cost).filter(|total| *total <= self.0.limit)).is_ok()
    }
    fn release(&self, cost: usize) {
        self.0.used.fetch_sub(cost, Ordering::AcqRel);
    }
}

/// Cumulative lookup and eviction counters, plus current occupancy.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CacheStats {
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
    pub entries: usize,
    pub cost: usize,
}

/// An insertion that could not reserve its cost. The key and value are returned.
#[derive(Debug, PartialEq, Eq)]
pub struct InsertError<K, V> {
    pub key: K,
    pub value: V,
    pub cost: usize,
}

struct Entry<K, V> {
    key: K,
    value: V,
    cost: usize,
    pinned: bool,
    prev: [Option<usize>; 2],
    next: [Option<usize>; 2],
}

/// O(1) expected lookup, touch, and eviction. List 0 tracks every entry;
/// list 1 tracks unpinned entries only, making pin-skipping constant time.
pub struct CostLru<K, V> {
    budget: Budget,
    map: HashMap<K, usize>,
    slots: Vec<Option<Entry<K, V>>>,
    free: Vec<usize>,
    head: [Option<usize>; 2],
    tail: [Option<usize>; 2],
    stats: CacheStats,
}

impl<K: Eq + Hash + Clone, V> CostLru<K, V> {
    /// Create a cache with a private budget.
    pub fn new(limit: usize) -> Self {
        Self::with_budget(Budget::new(limit))
    }
    /// Create a cache drawing from a shared budget.
    pub fn with_budget(budget: Budget) -> Self {
        Self { budget, map: HashMap::new(), slots: Vec::new(), free: Vec::new(), head: [None; 2], tail: [None; 2], stats: CacheStats::default() }
    }
    /// Access the shared budget.
    pub fn budget(&self) -> &Budget {
        &self.budget
    }
    /// Current occupancy and cumulative counters.
    pub fn stats(&self) -> CacheStats {
        self.stats
    }
    /// Number of entries.
    pub fn len(&self) -> usize {
        self.map.len()
    }
    /// Whether the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
    fn unlink(&mut self, index: usize, list: usize) {
        let Some(entry) = self.slots[index].as_ref() else { return };
        let (prev, next) = (entry.prev[list], entry.next[list]);
        if let Some(prev) = prev {
            if let Some(entry) = self.slots[prev].as_mut() {
                entry.next[list] = next;
            }
        } else {
            self.head[list] = next;
        }
        if let Some(next) = next {
            if let Some(entry) = self.slots[next].as_mut() {
                entry.prev[list] = prev;
            }
        } else {
            self.tail[list] = prev;
        }
        if let Some(entry) = self.slots[index].as_mut() {
            entry.prev[list] = None;
            entry.next[list] = None;
        }
    }
    fn append(&mut self, index: usize, list: usize) {
        let tail = self.tail[list];
        if let Some(entry) = self.slots[index].as_mut() {
            entry.prev[list] = tail;
            entry.next[list] = None;
        }
        if let Some(tail) = tail {
            if let Some(entry) = self.slots[tail].as_mut() {
                entry.next[list] = Some(index);
            }
        } else {
            self.head[list] = Some(index);
        }
        self.tail[list] = Some(index);
    }
    fn touch(&mut self, index: usize) {
        let pinned = self.slots[index].as_ref().is_some_and(|entry| entry.pinned);
        self.unlink(index, 0);
        self.append(index, 0);
        if !pinned {
            self.unlink(index, 1);
            self.append(index, 1);
        }
    }
    /// Look up an entry and make it most recently used.
    pub fn get(&mut self, key: &K) -> Option<&V> {
        let Some(&index) = self.map.get(key) else {
            self.stats.misses += 1;
            return None;
        };
        self.stats.hits += 1;
        self.touch(index);
        self.slots[index].as_ref().map(|entry| &entry.value)
    }
    /// Look up without updating recency or hit/miss counters.
    pub fn peek(&self, key: &K) -> Option<&V> {
        self.map.get(key).and_then(|index| self.slots[*index].as_ref()).map(|entry| &entry.value)
    }
    fn remove_index(&mut self, index: usize) -> Option<(K, V)> {
        let pinned = self.slots[index].as_ref()?.pinned;
        self.unlink(index, 0);
        if !pinned {
            self.unlink(index, 1);
        }
        let entry = self.slots[index].take()?;
        self.map.remove(&entry.key);
        self.stats.entries -= 1;
        self.stats.cost -= entry.cost;
        self.budget.release(entry.cost);
        self.free.push(index);
        Some((entry.key, entry.value))
    }
    /// Remove an entry, including a pinned entry.
    pub fn remove(&mut self, key: &K) -> Option<V> {
        let index = *self.map.get(key)?;
        self.remove_index(index).map(|(_, value)| value)
    }
    /// Pin or unpin an entry. Pinning does not change its recency.
    /// Unpinning makes it most recently used so both lists remain ordered.
    pub fn set_pinned(&mut self, key: &K, pinned: bool) -> bool {
        let Some(&index) = self.map.get(key) else { return false };
        let was_pinned = self.slots[index].as_ref().is_some_and(|entry| entry.pinned);
        if was_pinned == pinned {
            return true;
        }
        if pinned {
            self.unlink(index, 1);
        } else {
            self.unlink(index, 0);
            self.append(index, 0);
            self.append(index, 1);
        }
        if let Some(entry) = self.slots[index].as_mut() {
            entry.pinned = pinned;
        }
        true
    }
    /// Insert as unpinned, replacing any existing entry.
    ///
    /// On failure, the new key/value are returned. A replaced entry is removed,
    /// and local unpinned entries may already have been evicted. Reservations in
    /// other caches and pinned entries are never evicted by this cache.
    pub fn insert(&mut self, key: K, value: V, cost: usize) -> Result<(), InsertError<K, V>> {
        self.remove(&key);
        if cost > self.budget.limit() {
            return Err(InsertError { key, value, cost });
        }
        while !self.budget.reserve(cost) {
            let Some(index) = self.head[1] else { return Err(InsertError { key, value, cost }) };
            self.remove_index(index);
            self.stats.evictions += 1;
        }
        let entry = Entry { key: key.clone(), value, cost, pinned: false, prev: [None; 2], next: [None; 2] };
        let index = if let Some(index) = self.free.pop() {
            self.slots[index] = Some(entry);
            index
        } else {
            self.slots.push(Some(entry));
            self.slots.len() - 1
        };
        self.map.insert(key, index);
        self.stats.entries += 1;
        self.stats.cost += cost;
        self.append(index, 0);
        self.append(index, 1);
        Ok(())
    }
    /// Iterate from least to most recently used, without touching entries.
    pub fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        let mut current = self.head[0];
        std::iter::from_fn(move || {
            let entry = self.slots.get(current?)?.as_ref()?;
            current = entry.next[0];
            Some((&entry.key, &entry.value))
        })
    }
    /// Remove all entries, including pinned entries.
    pub fn clear(&mut self) {
        while let Some(index) = self.head[0] {
            self.remove_index(index);
        }
    }
}
impl<K, V> Drop for CostLru<K, V> {
    fn drop(&mut self) {
        self.budget.release(self.stats.cost);
    }
}
