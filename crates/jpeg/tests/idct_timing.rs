use std::{hint::black_box, time::Instant};

use craft_jpeg::{Limits, decode};

#[test]
#[ignore = "release-only decode timing; run with --release -- --ignored --nocapture"]
fn decode_largest_fixtures_timing() {
    let iterations = 200;
    for (name, bytes) in
        [("whitetail-huffman.jpg", &include_bytes!("data/whitetail-huffman.jpg")[..]), ("testorig12.jpg", &include_bytes!("data/testorig12.jpg")[..])]
    {
        let limits = Limits::default();
        let warmup = decode(bytes, &limits, None).unwrap_or_else(|error| panic!("{name}: {error}"));
        assert!(!warmup.damaged);
        let start = Instant::now();
        for _ in 0..iterations {
            black_box(decode(black_box(bytes), &limits, None).unwrap_or_else(|error| panic!("{name}: {error}")));
        }
        eprintln!("{name}: {iterations} decodes in {:.3} ms", start.elapsed().as_secs_f64() * 1000.0);
    }
}
