#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::sync::Arc;

use craft_tiledir::{DenseTileDir, SparseTileDir, TileCoord, TileDirectory};
use proptest::prelude::*;

fn byte_tile(value: u8) -> Arc<Vec<u8>> {
    Arc::new(vec![value; 16])
}

fn u16_tile(len: usize, value: u16) -> Arc<Vec<u16>> {
    Arc::new(vec![value; len])
}

fn coordinate() -> impl Strategy<Value = i32> {
    prop_oneof![
        4 => -20i32..20,
        1 => any::<i32>(),
        1 => prop::sample::select(vec![i32::MIN, i32::MAX, -1, 0, 1]),
    ]
}

fn check_dense_model(dir: &DenseTileDir<Arc<Vec<u16>>>, model: &[Option<Vec<u16>>]) {
    let columns = dir.tiles_x() as usize;
    let mut expected_row_major = Vec::new();
    let mut expected_col_major = BTreeMap::new();
    for (index, expected) in model.iter().enumerate() {
        let (tx, ty) = ((index % columns) as i32, (index / columns) as i32);
        assert_eq!(dir.get(tx, ty).map(|data| data.as_slice()), expected.as_deref());
        assert_eq!(dir.contains_key(tx, ty), expected.is_some());
        if let Some(data) = expected {
            expected_row_major.push(((tx, ty), data.as_slice()));
            expected_col_major.insert((tx, ty), data.as_slice());
        }
    }
    assert_eq!(dir.len(), expected_row_major.len());
    assert_eq!(dir.is_empty(), expected_row_major.is_empty());
    let actual_row_major: Vec<_> = dir.iter_row_major().map(|(coord, data)| (coord, data.as_slice())).collect();
    assert_eq!(actual_row_major, expected_row_major);
    let actual_sorted: Vec<_> = dir.iter().map(|(coord, data)| (coord, data.as_slice())).collect();
    let expected_sorted: Vec<_> = expected_col_major.into_iter().collect();
    assert_eq!(actual_sorted, expected_sorted);
}

#[test]
fn clones_share_roots_and_mutations_copy_only_the_path() {
    let mut directory: SparseTileDir<_> = [(i32::MIN, 0), (0, 0), (0, 1)].into_iter().map(|coord| (coord, byte_tile(1))).collect();
    let snapshot = directory.clone();
    assert!(directory.ptr_eq(&snapshot));
    assert!(directory.shared_with(&snapshot));
    let coord = (0, 1);
    assert_eq!(Arc::strong_count(directory.get(coord.0, coord.1).unwrap()), 1);
    Arc::make_mut(directory.get_mut(coord.0, coord.1).unwrap())[0] = 9;
    assert_eq!(snapshot.get(coord.0, coord.1).unwrap()[0], 1);
    assert_eq!(directory.get(coord.0, coord.1).unwrap()[0], 9);
    assert!(!directory.ptr_eq(&snapshot));
    assert!(directory.shared_with(&snapshot), "untouched negative-x branch remains shared");
    assert_ne!(directory, snapshot);
    directory.insert(coord.0, coord.1, byte_tile(1));
    assert_eq!(directory, snapshot);
}

#[test]
fn signed_extremes_and_last_nibble_splits_stay_sorted() {
    let coords = [i32::MIN, -1, 0, 1, 15, 16, i32::MAX];
    let model: BTreeMap<TileCoord, _> = coords.into_iter().flat_map(|tx| coords.into_iter().map(move |ty| ((tx, ty), byte_tile(7)))).collect();
    let mut directory: SparseTileDir<_> = model.iter().rev().map(|(coord, tile)| (*coord, Arc::clone(tile))).collect();
    assert!(directory.iter().eq(model.iter().map(|(k, v)| (*k, v))));
    assert_eq!(format!("{directory:?}"), format!("{model:?}"));
    let before = directory.clone();
    assert!(directory.get_mut(2, 3).is_none());
    assert!(directory.remove(2, 3).is_none());
    assert!(directory.ptr_eq(&before));
    for (tx, ty) in model.keys().copied().skip(1) {
        directory.remove(tx, ty);
    }
    let singleton: SparseTileDir<_> = model.into_iter().take(1).collect();
    assert_eq!(directory, singleton);
}

#[test]
fn sparse_for_each_mut_and_retain_path_copy_semantics() {
    let mut dir: SparseTileDir<_> = [(i32::MIN, 0), (0, 0), (0, 1)].into_iter().map(|coord| (coord, byte_tile(2))).collect();
    let snap = dir.clone();
    dir.retain(|_, tile| tile[0] == 2);
    assert!(dir.ptr_eq(&snap), "no-op retain must keep root shared");

    dir.retain(|(tx, _), _| tx >= 0);
    assert!(!dir.ptr_eq(&snap));
    assert!(dir.shared_with(&snap), "removing negative-x branch leaves positive-x branch shared before collapse or when multiple remain");
    assert_eq!(dir.len(), 2);

    let snap2 = dir.clone();
    dir.for_each_mut(|(_, ty), tile| {
        Arc::make_mut(tile)[0] = (ty + 10) as u8;
    });
    assert_eq!(dir.get(0, 0).unwrap()[0], 10);
    assert_eq!(dir.get(0, 1).unwrap()[0], 11);
    assert_eq!(snap2.get(0, 0).unwrap()[0], 2);
    assert_eq!(snap2.get(0, 1).unwrap()[0], 2);
}

#[test]
fn directory_snapshots_only_copy_the_written_chunk() {
    let mut dir = DenseTileDir::new(65, 1);
    for tx in 0..65 {
        dir.insert(tx, 0, u16_tile(8, 7));
    }
    let snapshot = dir.clone();
    assert!(dir.ptr_eq(&snapshot));
    assert_eq!(Arc::strong_count(dir.get(0, 0).unwrap()), 1);
    assert_eq!(dir.chunk_count(), 2);
    assert_eq!(dir.chunk_len(0), Some(64));
    assert_eq!(dir.chunk_len(1), Some(1));

    Arc::make_mut(dir.get_mut(63, 0).unwrap())[0] = 9;
    assert!(!dir.ptr_eq(&snapshot));
    assert!(!dir.chunk_ptr_eq(&snapshot, 0));
    assert!(dir.chunk_ptr_eq(&snapshot, 1));
    assert!(dir.shared_with(&snapshot));
    assert!(Arc::ptr_eq(dir.get(62, 0).unwrap(), snapshot.get(62, 0).unwrap()));

    let after_first = dir.clone();
    drop(after_first);
    Arc::make_mut(dir.get_mut(62, 0).unwrap())[0] = 11;
    assert!(dir.chunk_ptr_eq(&snapshot, 1));

    dir.remove(64, 0);
    assert_eq!(snapshot.get(63, 0).unwrap()[0], 7);
    assert_eq!(snapshot.get(64, 0).unwrap()[0], 7);
    assert_eq!(dir.keys().last(), Some((63, 0)));
}

#[test]
fn no_op_pruning_and_invalid_writes_keep_directory_shared() {
    let mut dir = DenseTileDir::new(65, 1);
    dir.insert(64, 0, u16_tile(8, 7));
    let snapshot = dir.clone();
    dir.retain(|_, data| data.iter().any(|value| *value != 0));
    assert!(dir.insert(65, 0, u16_tile(8, 1)).is_none());
    assert!(dir.insert(-1, 0, u16_tile(8, 1)).is_none());
    assert!(dir.remove(65, 0).is_none());
    assert!(dir.remove(0, 0).is_none());
    assert!(dir.get_mut(0, 1).is_none());
    assert!(dir.get_mut(0, 0).is_none());
    assert!(dir.ptr_eq(&snapshot));

    Arc::make_mut(dir.get_mut(64, 0).unwrap()).fill(0);
    dir.retain(|_, data| data.iter().any(|value| *value != 0));
    assert_eq!(dir.len(), 0);
    assert_eq!(snapshot.len(), 1);
}

#[test]
fn directory_debug_and_equality_keep_flat_value_semantics() {
    let mut directory = DenseTileDir::new(65, 1);
    let mut dense: Vec<Option<Arc<Vec<u16>>>> = vec![None; 65];
    dense[64] = Some(Arc::new(vec![7]));
    directory.insert(64, 0, Arc::clone(dense[64].as_ref().unwrap()));
    assert_eq!(format!("{directory:?}"), format!("{dense:?}"));

    let mut independent = DenseTileDir::new(65, 1);
    independent.insert(64, 0, Arc::new(vec![7]));
    assert!(!directory.ptr_eq(&independent));
    assert!(!directory.shared_with(&independent));
    assert_eq!(directory, independent);

    independent.remove(64, 0);
    assert_ne!(directory, independent);
}

#[test]
fn dense_for_each_mut_keeps_empty_chunks_shared() {
    let mut dir = DenseTileDir::new(128, 1);
    dir.insert(5, 0, u16_tile(4, 3));
    let snapshot = dir.clone();
    dir.for_each_mut(|_, tile| {
        Arc::make_mut(tile)[0] += 10;
    });
    assert!(!dir.chunk_ptr_eq(&snapshot, 0));
    assert!(dir.chunk_ptr_eq(&snapshot, 1), "empty chunk 1 must remain shared across for_each_mut");
    assert_eq!(dir.get(5, 0).unwrap()[0], 13);
    assert_eq!(snapshot.get(5, 0).unwrap()[0], 3);
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn sparse_edits_match_ordered_model_and_preserve_snapshots(
        edits in prop::collection::vec((coordinate(), coordinate(), prop::option::of(any::<u8>())), 0..128)
    ) {
        let mut directory = SparseTileDir::new();
        let mut model = BTreeMap::new();
        let mut snapshots = Vec::new();
        for (tx, ty, value) in edits {
            snapshots.push((directory.clone(), model.clone()));
            let coord = (tx, ty);
            match value {
                Some(value) => {
                    let tile = byte_tile(value);
                    prop_assert_eq!(
                        directory.insert(tx, ty, Arc::clone(&tile)),
                        model.insert(coord, tile)
                    );
                }
                None => prop_assert_eq!(directory.remove(tx, ty), model.remove(&coord)),
            }
            prop_assert_eq!(directory.get(tx, ty), model.get(&coord));
            prop_assert_eq!(directory.len(), model.len());
            prop_assert_eq!(directory.is_empty(), model.is_empty());
            prop_assert!(directory.iter().eq(model.iter().map(|(k, v)| (*k, v))));
            let rebuilt: SparseTileDir<_> = model
                .iter()
                .rev()
                .map(|(coord, value)| (*coord, Arc::clone(value)))
                .collect();
            prop_assert_eq!(&directory, &rebuilt);
        }
        for (snapshot, expected) in snapshots {
            prop_assert!(snapshot.iter().eq(expected.iter().map(|(k, v)| (*k, v))));
        }
    }

    #[test]
    fn sparse_repeated_edits_and_pruning_keep_canonical_shape(
        edits in prop::collection::vec((0i32..32, prop::option::of(any::<u8>())), 0..256)
    ) {
        let mut directory = SparseTileDir::new();
        let mut model = BTreeMap::new();
        for (ty, value) in edits {
            let coord = (-1, ty);
            match value {
                Some(value) => {
                    directory.insert(coord.0, coord.1, byte_tile(value));
                    model.insert(coord, byte_tile(value));
                }
                None => {
                    directory.remove(coord.0, coord.1);
                    model.remove(&coord);
                }
            }
            prop_assert!(directory.iter().eq(model.iter().map(|(k, v)| (*k, v))));
        }
        directory.retain(|(_, ty), _| ty % 2 == 0);
        model.retain(|(_, ty), _| ty % 2 == 0);
        let rebuilt: SparseTileDir<_> = model.into_iter().collect();
        prop_assert_eq!(&directory, &rebuilt);
        for (tx, ty) in directory.keys().collect::<Vec<_>>() {
            directory.remove(tx, ty);
        }
        prop_assert_eq!(directory, SparseTileDir::new());
    }

    #[test]
    fn sparse_undo_redo_involution_and_snapshot_isolation(
        steps in prop::collection::vec((-4i32..4, -4i32..4, 0u8..=255, any::<bool>()), 1..48)
    ) {
        let mut current = SparseTileDir::new();
        current.insert(0, 0, byte_tile(255));
        let mut undo_stack: Vec<SparseTileDir<Arc<Vec<u8>>>> = Vec::new();
        let mut redo_stack: Vec<SparseTileDir<Arc<Vec<u8>>>> = Vec::new();
        let mut states = vec![current.clone()];

        for (tx, ty, value, remove) in steps {
            undo_stack.push(current.clone());
            redo_stack.clear();
            if remove {
                current.remove(tx, ty);
            } else {
                let tile = current.get_or_insert_with(tx, ty, || byte_tile(0));
                Arc::make_mut(tile)[0] = value;
                current.retain(|_, data| data.iter().any(|b| *b != 0));
            }
            states.push(current.clone());
        }

        let final_state = current.clone();
        for expected in states.iter().rev().skip(1) {
            let prev = undo_stack.pop().unwrap();
            redo_stack.push(current);
            current = prev;
            prop_assert_eq!(&current, expected);
        }
        prop_assert!(undo_stack.is_empty());

        for expected in states.iter().skip(1) {
            let next = redo_stack.pop().unwrap();
            undo_stack.push(current);
            current = next;
            prop_assert_eq!(&current, expected);
        }
        prop_assert!(redo_stack.is_empty());
        prop_assert_eq!(&current, &final_state);

        // One final undo/redo involution check
        let prev = undo_stack.pop().unwrap();
        redo_stack.push(current);
        current = prev;
        let next = redo_stack.pop().unwrap();
        undo_stack.push(current);
        current = next;
        prop_assert_eq!(current, final_state);
    }

    #[test]
    fn dense_writes_match_vec_model_and_preserve_snapshots(
        columns in 1u32..20,
        rows in 1u32..10,
        comps in 1usize..=4,
        script in prop::collection::vec((0u8..6, any::<usize>(), 0usize..16, any::<u16>()), 1..40),
    ) {
        let tile_len = 16 * comps;
        let mut dir = DenseTileDir::new(columns, rows);
        let mut model: Vec<Option<Vec<u16>>> = vec![None; (columns * rows) as usize];
        let mut snapshots = Vec::new();

        for (operation, address, pixel, value) in script {
            snapshots.push((dir.clone(), model.clone()));
            let index = address % (model.len() + 2);
            let (tx, ty) = ((index % columns as usize) as i32, (index / columns as usize) as i32);
            match operation {
                0 => {
                    let data = vec![value; tile_len];
                    dir.insert(tx, ty, Arc::new(data.clone()));
                    if let Some(slot) = model.get_mut(index) {
                        *slot = Some(data);
                    }
                }
                1 | 2 => {
                    dir.remove(tx, ty);
                    if let Some(slot) = model.get_mut(index) {
                        *slot = None;
                    }
                }
                3 => {
                    let actual = dir.get_or_insert_with(tx, ty, || Arc::new(vec![value; tile_len]));
                    if let Some(slot) = model.get_mut(index) {
                        let data = slot.get_or_insert_with(|| vec![value; tile_len]);
                        data[pixel * comps] = 42;
                        Arc::make_mut(actual.unwrap())[pixel * comps] = 42;
                    } else {
                        prop_assert!(actual.is_none());
                    }
                }
                4 => {
                    if let Some(slot) = model.get_mut(index) {
                        let data = slot.get_or_insert_with(|| vec![0; tile_len]);
                        data[pixel * comps] = value;
                        let entry = dir.get_or_insert_with(tx, ty, || Arc::new(vec![0; tile_len])).unwrap();
                        Arc::make_mut(entry)[pixel * comps] = value;
                    } else {
                        prop_assert!(dir.get_mut(tx, ty).is_none());
                    }
                }
                _ => {
                    dir.retain(|_, data| !data.iter().all(|v| *v == 0));
                    for slot in &mut model {
                        if slot.as_ref().is_some_and(|data| data.iter().all(|v| *v == 0)) {
                            *slot = None;
                        }
                    }
                }
            }
            check_dense_model(&dir, &model);
        }

        for (snapshot, expected) in snapshots {
            check_dense_model(&snapshot, &expected);
        }

        let mut independent = DenseTileDir::new(columns, rows);
        for (index, tile) in model.into_iter().enumerate() {
            let (tx, ty) = ((index % columns as usize) as i32, (index / columns as usize) as i32);
            if let Some(data) = tile {
                independent.insert(tx, ty, Arc::new(data));
            }
        }
        prop_assert_eq!(dir, independent);
    }

    #[test]
    fn dense_undo_redo_involution_and_rollback(
        steps in prop::collection::vec((0i32..8, 0i32..8, any::<u16>(), any::<bool>()), 1..32),
    ) {
        let mut current = DenseTileDir::new(8, 8);
        current.insert(0, 0, u16_tile(16, 1));
        let mut undo_stack = Vec::new();
        let mut redo_stack = Vec::new();
        let mut states = vec![current.clone()];

        for (tx, ty, value, clear) in steps {
            undo_stack.push(current.clone());
            redo_stack.clear();
            if clear && (tx, ty) != (0, 0) {
                current.remove(tx, ty);
            } else {
                let slot = current.get_or_insert_with(tx, ty, || u16_tile(16, 0)).unwrap();
                Arc::make_mut(slot).fill(value);
            }
            states.push(current.clone());
        }

        let final_state = current.clone();
        // Simulated failed edit rollback via pre-edit snapshot
        let before_failed = current.clone();
        Arc::make_mut(current.get_or_insert_with(0, 0, || u16_tile(16, 0)).unwrap()).fill(123);
        current = before_failed;
        prop_assert_eq!(&current, &final_state);

        for expected in states[..states.len() - 1].iter().rev() {
            let prev = undo_stack.pop().unwrap();
            redo_stack.push(current);
            current = prev;
            prop_assert_eq!(&current, expected);
        }
        prop_assert!(undo_stack.is_empty());

        for expected in states.iter().skip(1) {
            let next = redo_stack.pop().unwrap();
            undo_stack.push(current);
            current = next;
            prop_assert_eq!(&current, expected);
        }
        prop_assert!(redo_stack.is_empty());
        prop_assert_eq!(&current, &final_state);

        // Undo + redo involution
        let prev = undo_stack.pop().unwrap();
        redo_stack.push(current);
        current = prev;
        let next = redo_stack.pop().unwrap();
        undo_stack.push(current);
        current = next;
        prop_assert_eq!(current, final_state);
    }

    #[test]
    fn sparse_and_dense_agree_on_random_script_within_bounds(
        tiles_x in 1u32..16,
        tiles_y in 1u32..16,
        script in prop::collection::vec((0u8..6, 0u32..32, 0u32..32, any::<u8>()), 1..80),
    ) {
        let mut sparse = SparseTileDir::new();
        let mut dense = DenseTileDir::new(tiles_x, tiles_y);
        let mut snapshots = Vec::new();

        for (op, raw_tx, raw_ty, val) in script {
            snapshots.push((sparse.clone(), dense.clone()));
            let tx = (raw_tx % tiles_x) as i32;
            let ty = (raw_ty % tiles_y) as i32;
            match op {
                0 => {
                    let tile = byte_tile(val);
                    let r_sparse = TileDirectory::insert(&mut sparse, tx, ty, Arc::clone(&tile));
                    let r_dense = TileDirectory::insert(&mut dense, tx, ty, tile);
                    prop_assert_eq!(r_sparse, r_dense);
                }
                1 => {
                    let r_sparse = TileDirectory::remove(&mut sparse, tx, ty);
                    let r_dense = TileDirectory::remove(&mut dense, tx, ty);
                    prop_assert_eq!(r_sparse, r_dense);
                }
                2 => {
                    if let Some(s_val) = TileDirectory::get_mut(&mut sparse, tx, ty) {
                        Arc::make_mut(s_val)[0] = val;
                        let d_val = TileDirectory::get_mut(&mut dense, tx, ty).unwrap();
                        Arc::make_mut(d_val)[0] = val;
                    } else {
                        prop_assert!(TileDirectory::get_mut(&mut dense, tx, ty).is_none());
                    }
                }
                3 => {
                    let s_val = sparse.get_or_insert_with(tx, ty, || byte_tile(val));
                    Arc::make_mut(s_val)[1] = val.wrapping_add(1);
                    let d_val = dense.get_or_insert_with(tx, ty, || byte_tile(val)).unwrap();
                    Arc::make_mut(d_val)[1] = val.wrapping_add(1);
                }
                4 => {
                    TileDirectory::retain(&mut sparse, |(x, y), data| (x + y) % 2 == 0 || data[0] >= 128);
                    TileDirectory::retain(&mut dense, |(x, y), data| (x + y) % 2 == 0 || data[0] >= 128);
                }
                _ => {
                    TileDirectory::for_each_mut(&mut sparse, |(x, y), data| {
                        Arc::make_mut(data)[0] = data[0].wrapping_add((x + y) as u8);
                    });
                    TileDirectory::for_each_mut(&mut dense, |(x, y), data| {
                        Arc::make_mut(data)[0] = data[0].wrapping_add((x + y) as u8);
                    });
                }
            }

            prop_assert_eq!(TileDirectory::len(&sparse), TileDirectory::len(&dense));
            prop_assert_eq!(TileDirectory::is_empty(&sparse), TileDirectory::is_empty(&dense));
            for y in -1..=tiles_y as i32 {
                for x in -1..=tiles_x as i32 {
                    prop_assert_eq!(
                        TileDirectory::get(&sparse, x, y),
                        TileDirectory::get(&dense, x, y)
                    );
                }
            }
            let sparse_items: Vec<_> = TileDirectory::iter(&sparse).collect();
            let dense_items: Vec<_> = TileDirectory::iter(&dense).collect();
            prop_assert_eq!(sparse_items, dense_items);
        }

        for (s_snap, d_snap) in snapshots {
            let s_items: Vec<_> = TileDirectory::iter(&s_snap).collect();
            let d_items: Vec<_> = TileDirectory::iter(&d_snap).collect();
            prop_assert_eq!(s_items, d_items);
        }
    }
}
