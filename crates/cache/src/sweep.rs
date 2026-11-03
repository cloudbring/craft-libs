use std::collections::HashMap;
use std::hash::Hash;

/// Cache whose entries survive a sweep only when used since the previous sweep.
#[derive(Debug)]
pub struct FrameSweep<K, V> {
    entries: HashMap<K, (V, bool)>,
}

impl<K, V> Default for FrameSweep<K, V> {
    fn default() -> Self {
        Self { entries: HashMap::new() }
    }
}
impl<K: Eq + Hash, V> FrameSweep<K, V> {
    /// Create an empty cache.
    pub fn new() -> Self {
        Self::default()
    }
    /// Look up and mark an entry as used.
    pub fn get(&mut self, key: &K) -> Option<&V> {
        self.entries.get_mut(key).map(|(value, used)| {
            *used = true;
            &*value
        })
    }
    /// Insert or replace an entry and mark it as used.
    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        self.entries.insert(key, (value, true)).map(|(old, _)| old)
    }
    /// Remove untouched entries and reset the marks for the next frame.
    pub fn end_frame(&mut self) {
        self.entries.retain(|_, (_, used)| {
            let keep = *used;
            *used = false;
            keep
        });
    }
    /// Return an entry without marking it used.
    pub fn peek(&self, key: &K) -> Option<&V> {
        self.entries.get(key).map(|(value, _)| value)
    }
    /// Number of entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    /// Whether the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    /// Remove all entries.
    pub fn clear(&mut self) {
        self.entries.clear();
    }
}
