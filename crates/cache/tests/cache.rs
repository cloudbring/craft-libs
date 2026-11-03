#![allow(clippy::unwrap_used, clippy::expect_used)]
use craft_cache::{Budget, CostLru, Fingerprint, FingerprintHasher, FrameSweep};
use proptest::prelude::*;

#[derive(Debug, Clone)]
struct Item {
    key: u8,
    value: u8,
    cost: usize,
    pinned: bool,
}

proptest! {
    #[test]
    fn lru_matches_vec(operations in prop::collection::vec((0u8..5, 0u8..8, 0u8..8), 0..500)) {
        let mut cache = CostLru::new(12);
        let mut model: Vec<Item> = Vec::new();
        let mut hits = 0;
        let mut misses = 0;
        let mut evictions = 0;
        for (op, key, value) in operations {
            let position = model.iter().position(|entry| entry.key == key);
            match op {
                0 => {
                    let actual = cache.get(&key).copied();
                    let expected = position.map(|index| { let entry = model.remove(index); let value = entry.value; model.push(entry); value });
                    if expected.is_some() { hits += 1; } else { misses += 1; }
                    prop_assert_eq!(actual, expected);
                }
                1 => {
                    let cost = usize::from(value);
                    if let Some(index) = position { model.remove(index); }
                    while model.iter().map(|entry| entry.cost).sum::<usize>() + cost > 12 {
                        if let Some(index) = model.iter().position(|entry| !entry.pinned) { model.remove(index); evictions += 1; } else { break; }
                    }
                    let admitted = model.iter().map(|entry| entry.cost).sum::<usize>() + cost <= 12;
                    prop_assert_eq!(cache.insert(key, value, cost).is_ok(), admitted);
                    if admitted { model.push(Item { key, value, cost, pinned: false }); }
                }
                2 => { prop_assert_eq!(cache.remove(&key), position.map(|index| model.remove(index).value)); }
                3 => {
                    let pinned = value % 2 == 0;
                    prop_assert_eq!(cache.set_pinned(&key, pinned), position.is_some());
                    if let Some(index) = position && model[index].pinned != pinned {
                        model[index].pinned = pinned;
                        if !pinned { let entry = model.remove(index); model.push(entry); }
                    }
                }
                _ => { prop_assert_eq!(cache.peek(&key).copied(), position.map(|index| model[index].value)); }
            }
            prop_assert_eq!(cache.iter().map(|(key, value)| (*key, *value)).collect::<Vec<_>>(), model.iter().map(|entry| (entry.key, entry.value)).collect::<Vec<_>>());
            let stats = cache.stats();
            prop_assert_eq!((stats.hits, stats.misses, stats.evictions, stats.entries, stats.cost), (hits, misses, evictions, model.len(), model.iter().map(|entry| entry.cost).sum()));
            prop_assert!(cache.budget().used() <= 12);
        }
    }
}

#[test]
fn shared_budget_and_pin() {
    let budget = Budget::new(5);
    let mut first = CostLru::with_budget(budget.clone());
    let mut second = CostLru::with_budget(budget.clone());
    first.insert(1, 1, 3).unwrap();
    first.set_pinned(&1, true);
    assert!(second.insert(2, 2, 3).is_err());
    assert!(first.insert(3, 3, 3).is_err());
    assert_eq!(first.peek(&1), Some(&1));
    drop(first);
    assert_eq!(budget.used(), 0);
    second.insert(2, 2, 3).unwrap();
    second.clear();
    assert_eq!(budget.used(), 0);
}

#[test]
fn frame_sweep_marks_only_access_and_insert() {
    let mut cache = FrameSweep::new();
    cache.insert(1, "one");
    cache.insert(2, "two");
    cache.end_frame();
    assert_eq!(cache.get(&1), Some(&"one"));
    assert_eq!(cache.peek(&2), Some(&"two"));
    cache.end_frame();
    assert_eq!(cache.len(), 1);
    assert_eq!(cache.peek(&1), Some(&"one"));
    cache.end_frame();
    assert!(cache.is_empty());
}

#[derive(Fingerprint)]
struct Key {
    a: u32,
    text: String,
    extra: Option<u8>,
}
#[derive(Fingerprint)]
struct Extended {
    a: u32,
    text: String,
    extra: Option<u8>,
    new_field: u8,
}

#[test]
fn stable_vectors_and_derived_fields() {
    let mut hasher = FingerprintHasher::default();
    hasher.write(b"hello");
    assert_eq!(hasher.finish(), 0xa430d84680aabd0b);
    assert_eq!(0x12345678_u32.fingerprint(), 0xcccfd053e47c3365);
    let key = Key { a: 7, text: "hi".into(), extra: Some(9) };
    let extended = Extended { a: 7, text: "hi".into(), extra: Some(9), new_field: 1 };
    assert_ne!(key.fingerprint(), extended.fingerprint());
}

#[test]
#[ignore = "manual release benchmark: 1M inserts into 10k entry budget"]
fn bench_insert_vs_sort() {
    use std::time::Instant;
    let start = Instant::now();
    let mut cache = CostLru::new(10_000);
    for key in 0..1_000_000 {
        assert!(cache.insert(std::hint::black_box(key), key, 1).is_ok());
    }
    println!("intrusive: {:?}", start.elapsed());
    // Inventory: filmcraft/crates/media/src/cache.rs:61 before #104.
    let start = Instant::now();
    let mut entries = std::collections::HashMap::new();
    for key in 0..1_000_000 {
        entries.insert(std::hint::black_box(key), key);
        if entries.len() > 10_000 {
            let mut stamps: Vec<_> = entries.iter().map(|(key, stamp)| (*stamp, *key)).collect();
            stamps.sort_unstable_by_key(|entry| entry.0);
            entries.remove(&stamps[0].1);
        }
    }
    println!("sort per insert: {:?}", start.elapsed());
    assert_eq!(entries.len(), 10_000);
}
