//! Decode-only benchmark of the largest generated VP8 fixture.

use crate::common;
use craft_vp8::Decoder;
use std::time::Instant;

#[test]
#[ignore]
fn perf_1080p() {
    let fixture = common::fixture("hd_1080p");
    let Some((ivf, reference)) = common::ensure(fixture) else { return };
    let data = std::fs::read(ivf).unwrap();
    let frames = common::ivf_frames(&data);
    let pictures = common::decode_all(&frames);
    let actual: Vec<u8> = pictures.iter().flat_map(common::i420).collect();
    assert!(actual == std::fs::read(reference).unwrap());
    let mut best = f64::MAX;
    let mut count = 0;
    for _ in 0..3 {
        let start = Instant::now();
        count = 0;
        for _ in 0..20 {
            let mut decoder = Decoder::new();
            for frame in &frames {
                count += usize::from(decoder.decode(frame, 0).unwrap().is_some());
            }
        }
        best = best.min(start.elapsed().as_secs_f64());
    }
    println!("1080p VP8: {count} frames in {best:.6}s = {:.1} fps", count as f64 / best);
}
