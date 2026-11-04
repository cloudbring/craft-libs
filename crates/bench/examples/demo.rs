//! `cargo run --release -p craft-bench --example demo -- out.json [old-rev new-rev]`
//!
//! Benchmarks a serial sum against a four-accumulator sum and writes the JSON report; prints the markdown.
use std::hint::black_box;

use craft_bench::report::Revs;
use craft_bench::{Config, Paired};

fn main() {
    let mut args = std::env::args().skip(1);
    let out = args.next().unwrap_or_else(|| "demo.json".into());
    let revs = Revs { old: args.next(), new: args.next() };
    let report = Paired::new("sum of squares: serial vs four accumulators")
        .old(|n: &u64| (0..black_box(*n)).fold(0u64, |a, i| a.wrapping_add(black_box(i).wrapping_mul(i))))
        .new(|n: &u64| {
            let mut acc = [0u64; 4];
            for i in 0..black_box(*n) {
                acc[(i & 3) as usize] = acc[(i & 3) as usize].wrapping_add(black_box(i).wrapping_mul(i));
            }
            acc.iter().fold(0u64, |a, b| a.wrapping_add(*b))
        })
        .inputs([("n=1e4", 10_000u64), ("n=1e6", 1_000_000)])
        .equal()
        .run(&Config { revs, ..Config::default() })
        .expect("benchmark failed");
    print!("{}", report.to_markdown());
    std::fs::write(&out, report.to_json()).expect("write report");
}
