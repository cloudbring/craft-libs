#![forbid(unsafe_code)]
//! Comparative benchmarks ported from PhotoCraft (`persistent_directory_bench.rs`) and
//! TextureCraft (`snapshot_bench.rs`), measuring `BTreeMap`, `Vec`, `SparseTileDir`, and
//! `DenseTileDir` across both sparse one-tile snapshot histories and dense full-canvas edits.
//!
//! Run with:
//! `cargo test -p craft-tiledir --release --test bench -- --ignored --nocapture --test-threads=1`

use std::collections::BTreeMap;
use std::hint::black_box;
use std::sync::Arc;
use std::time::Instant;

use craft_tiledir::{DenseTileDir, SparseTileDir, TileCoord};

const PHOTOCRAFT_TILE_BYTES: usize = 256 * 256;
const TEXTURECRAFT_TILE_U16S: usize = 64 * 64 * 4;

trait ByteSurface: Clone {
    fn edit(&mut self, tx: i32, ty: i32, value: u8, dense: bool);
}

fn edit_bytes(tile: &mut [u8], value: u8, dense: bool) {
    if dense {
        tile.fill(value);
    } else {
        tile[0] = value;
    }
}

impl ByteSurface for BTreeMap<TileCoord, Arc<Vec<u8>>> {
    fn edit(&mut self, tx: i32, ty: i32, value: u8, dense: bool) {
        if let Some(tile) = self.get_mut(&(tx, ty)) {
            edit_bytes(Arc::make_mut(tile).as_mut_slice(), value, dense);
        }
    }
}

impl ByteSurface for SparseTileDir<Arc<Vec<u8>>> {
    fn edit(&mut self, tx: i32, ty: i32, value: u8, dense: bool) {
        if let Some(tile) = self.get_mut(tx, ty) {
            edit_bytes(Arc::make_mut(tile).as_mut_slice(), value, dense);
        }
    }
}

impl ByteSurface for DenseTileDir<Arc<Vec<u8>>> {
    fn edit(&mut self, tx: i32, ty: i32, value: u8, dense: bool) {
        if let Some(tile) = self.get_mut(tx, ty) {
            edit_bytes(Arc::make_mut(tile).as_mut_slice(), value, dense);
        }
    }
}

fn run_photocraft_sample<S: ByteSurface>(mut surface: S, dense: bool, spacing: i32) -> f64 {
    let steps = if dense { 1 } else { 200 };
    let mut history = Vec::with_capacity(steps);
    let start = Instant::now();
    for step in 0..steps {
        history.push(black_box(surface.clone()));
        if dense {
            for tx in 0..32 {
                for ty in 0..32 {
                    surface.edit(tx, ty, 127, true);
                }
            }
        } else {
            let index = (step * 37) % 4096;
            surface.edit((index / 64) as i32 * spacing, (index % 64) as i32 * spacing, (step % 254) as u8, false);
        }
    }
    let elapsed = start.elapsed().as_secs_f64() * 1000.0;
    black_box((&surface, &history));
    elapsed
}

fn median_ms(mut samples: Vec<f64>) -> f64 {
    samples.sort_by(f64::total_cmp);
    samples[samples.len() / 2]
}

#[test]
fn clone_preserves_pointer_sharing_across_tile_counts() {
    let tile = Arc::new(vec![1u8; 64]);
    for side in [1i32, 4, 64] {
        let mut sparse = SparseTileDir::new();
        let mut dense = DenseTileDir::new(side as u32, side as u32);
        for tx in 0..side {
            for ty in 0..side {
                sparse.insert(tx, ty, Arc::clone(&tile));
                dense.insert(tx, ty, Arc::clone(&tile));
            }
        }
        let sparse_clone = sparse.clone();
        let dense_clone = dense.clone();
        assert!(sparse.ptr_eq(&sparse_clone));
        assert!(dense.ptr_eq(&dense_clone));
    }
}

#[test]
#[ignore = "comparative timing benchmark; run with --release --ignored --nocapture --test-threads=1"]
fn photocraft_and_texturecraft_benchmarks() {
    println!("=== PhotoCraft Scenarios (256x256 GRAY8 tiles = 64 KiB/tile, median of 7) ===");
    for dense in [false, true] {
        let (side, spacing) = if dense { (32i32, 1i32) } else { (64i32, 2i32) };
        let seed_tile = Arc::new(vec![1u8; PHOTOCRAFT_TILE_BYTES]);
        let mut btree = BTreeMap::new();
        let mut sparse = SparseTileDir::new();
        let dense_bound = (side * spacing) as u32;
        let mut dense_dir = DenseTileDir::new(dense_bound, dense_bound);
        for tx in 0..side {
            for ty in 0..side {
                let (cx, cy) = (tx * spacing, ty * spacing);
                btree.insert((cx, cy), Arc::clone(&seed_tile));
                sparse.insert(cx, cy, Arc::clone(&seed_tile));
                dense_dir.insert(cx, cy, Arc::clone(&seed_tile));
            }
        }

        let mut btree_samples = Vec::new();
        let mut sparse_samples = Vec::new();
        let mut dense_samples = Vec::new();
        for round in 0..8 {
            let b = run_photocraft_sample(btree.clone(), dense, spacing);
            let s = run_photocraft_sample(sparse.clone(), dense, spacing);
            let d = run_photocraft_sample(dense_dir.clone(), dense, spacing);
            if round != 0 {
                btree_samples.push(b);
                sparse_samples.push(s);
                dense_samples.push(d);
            }
        }
        let scenario = if dense { "1024 tiles / full-canvas edit" } else { "4096 tiles / 200 one-tile snapshots" };
        println!(
            "{scenario}: BTreeMap {:.3} ms | SparseTileDir {:.3} ms | DenseTileDir {:.3} ms",
            median_ms(btree_samples),
            median_ms(sparse_samples),
            median_ms(dense_samples),
        );
    }

    println!(
        "
=== TextureCraft Scenarios (64x64 RGBA u16 tiles = 32 KiB/tile, 4096 tiles, median of 9) ==="
    );
    const COLUMNS: u32 = 64;
    const TILE_COUNT: usize = (COLUMNS * COLUMNS) as usize;
    const HISTORY: usize = 200;
    const REPEATS: usize = 9;

    #[derive(Clone, Copy)]
    enum Workload {
        Snapshots,
        OneTileHistory,
        FullImageEdit,
    }

    trait U16Grid: Clone {
        fn allocated() -> Self;
        fn rewrite(&mut self, index: usize, value: u16);
    }

    #[derive(Clone)]
    struct VecGrid(Vec<Option<Arc<Vec<u16>>>>);

    impl U16Grid for VecGrid {
        fn allocated() -> Self {
            Self((0..TILE_COUNT).map(|_| Some(Arc::new(vec![1u16; TEXTURECRAFT_TILE_U16S]))).collect())
        }
        fn rewrite(&mut self, index: usize, value: u16) {
            Arc::make_mut(self.0[index].as_mut().unwrap()).fill(value);
        }
    }

    impl U16Grid for DenseTileDir<Arc<Vec<u16>>> {
        fn allocated() -> Self {
            let mut dir = Self::new(COLUMNS, COLUMNS);
            for ty in 0..COLUMNS as i32 {
                for tx in 0..COLUMNS as i32 {
                    dir.insert(tx, ty, Arc::new(vec![1u16; TEXTURECRAFT_TILE_U16S]));
                }
            }
            dir
        }
        fn rewrite(&mut self, index: usize, value: u16) {
            let (tx, ty) = ((index as u32 % COLUMNS) as i32, (index as u32 / COLUMNS) as i32);
            Arc::make_mut(self.get_mut(tx, ty).unwrap()).fill(value);
        }
    }

    impl U16Grid for SparseTileDir<Arc<Vec<u16>>> {
        fn allocated() -> Self {
            let mut dir = Self::new();
            for ty in 0..COLUMNS as i32 {
                for tx in 0..COLUMNS as i32 {
                    dir.insert(tx, ty, Arc::new(vec![1u16; TEXTURECRAFT_TILE_U16S]));
                }
            }
            dir
        }
        fn rewrite(&mut self, index: usize, value: u16) {
            let (tx, ty) = ((index as u32 % COLUMNS) as i32, (index as u32 / COLUMNS) as i32);
            Arc::make_mut(self.get_mut(tx, ty).unwrap()).fill(value);
        }
    }

    fn run_u16_workload<G: U16Grid>(workload: Workload) -> f64 {
        let mut grid = G::allocated();
        let mut history = Vec::with_capacity(HISTORY);
        let start = Instant::now();
        match workload {
            Workload::Snapshots => {
                for _ in 0..HISTORY {
                    history.push(black_box(grid.clone()));
                }
            }
            Workload::OneTileHistory => {
                for step in 0..HISTORY {
                    history.push(black_box(grid.clone()));
                    grid.rewrite(step * 67 % TILE_COUNT, step as u16 + 2);
                }
            }
            Workload::FullImageEdit => {
                history.push(black_box(grid.clone()));
                for index in 0..TILE_COUNT {
                    grid.rewrite(index, 2);
                }
            }
        }
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        black_box((&grid, &history));
        elapsed
    }

    for (label, workload) in [
        ("200 snapshots", Workload::Snapshots),
        ("200 one-tile history edits", Workload::OneTileHistory),
        ("one full-image history edit", Workload::FullImageEdit),
    ] {
        black_box(run_u16_workload::<VecGrid>(workload));
        black_box(run_u16_workload::<DenseTileDir<Arc<Vec<u16>>>>(workload));
        black_box(run_u16_workload::<SparseTileDir<Arc<Vec<u16>>>>(workload));
        let mut vec_samples = Vec::with_capacity(REPEATS);
        let mut dense_samples = Vec::with_capacity(REPEATS);
        let mut sparse_samples = Vec::with_capacity(REPEATS);
        for _ in 0..REPEATS {
            vec_samples.push(run_u16_workload::<VecGrid>(workload));
            dense_samples.push(run_u16_workload::<DenseTileDir<Arc<Vec<u16>>>>(workload));
            sparse_samples.push(run_u16_workload::<SparseTileDir<Arc<Vec<u16>>>>(workload));
        }
        println!(
            "{label}: Vec {:.4} ms | DenseTileDir {:.4} ms | SparseTileDir {:.4} ms",
            median_ms(vec_samples),
            median_ms(dense_samples),
            median_ms(sparse_samples),
        );
    }
}
