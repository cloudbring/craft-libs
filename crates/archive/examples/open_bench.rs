//! Open-cost bench for #206: `cargo run --release -p craft-archive --example open_bench`.
//! Alternating reps, medians, at 10 / 1,000 / 65,534 entries. `PHASES=1` also times `zip::ZipArchive::new` alone.
use std::io::Cursor;
use std::time::Instant;

use craft_archive::{ZipLimits, ZipReader, ZipWriter};

fn synthetic(n: usize) -> Vec<u8> {
    let mut writer = ZipWriter::in_memory();
    for i in 0..n {
        let body = format!("<e id=\"{i}\">{}</e>", "x".repeat(40 + i % 50));
        writer.add_stored(&format!("part/e{i}.xml"), body.as_bytes()).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

fn median(v: &mut [f64]) -> f64 {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[v.len() / 2]
}

fn time(iters: u32, mut f: impl FnMut()) -> f64 {
    let t = Instant::now();
    for _ in 0..iters {
        f();
    }
    t.elapsed().as_secs_f64() / f64::from(iters) * 1e6
}

fn main() {
    let limits = ZipLimits { max_entries: 65_534, max_entry_bytes: 1 << 29, max_total_bytes: 1 << 31 };
    let phases = std::env::var("PHASES").is_ok();
    for n in [10usize, 1000, 65_534] {
        let zip = synthetic(n);
        let iters = (2_000_000 / n as u32).max(3);
        let (mut open, mut raw) = (vec![], vec![]);
        for _ in 0..15 {
            open.push(time(iters, || drop(std::hint::black_box(ZipReader::open(std::hint::black_box(&zip), limits).unwrap()))));
            if phases {
                raw.push(time(iters, || drop(std::hint::black_box(zip::ZipArchive::new(Cursor::new(std::hint::black_box(&zip[..]))).unwrap()))));
            }
        }
        if phases {
            let archive = zip::ZipArchive::new(Cursor::new(&zip[..])).unwrap();
            let t_iter = time(iters, || {
                let mut sum = 0;
                for i in 0..archive.len() {
                    sum += archive.by_index_data(i).unwrap().size();
                }
                std::hint::black_box(sum);
            });
            let t_name = time(iters, || {
                let mut sum = 0;
                for i in 0..archive.len() {
                    sum += archive.by_index_data(i).unwrap().name().unwrap().len();
                }
                std::hint::black_box(sum);
            });
            let t_mode = time(iters, || {
                let mut sum = 0;
                for i in 0..archive.len() {
                    let e = archive.by_index_data(i).unwrap();
                    sum += usize::from(e.unix_mode().is_some()) + e.compressed_size() as usize + usize::from(e.encrypted());
                }
                std::hint::black_box(sum);
            });
            println!("  walk-size {t_iter:.1} us  walk-name {t_name:.1} us  walk-mode {t_mode:.1} us");
        }
        let raw = if phases { format!("  zip-crate-only {:.1} us", median(&mut raw)) } else { String::new() };
        println!("n={n:>6}  open {:.1} us{raw}", median(&mut open));
    }
}
