//! Paired remeasurement of craft-jpeg Huffman lookahead (artcraft-algorithms#190, claim C10).
//! Usage: perf-remeasure <out_dir> <libjpeg_turbo_dir> <office_jpegs_dir>
//! Env: OLD_REV, NEW_REV (recorded), REPS (default 20).
mod common;
use common::*;
use craft_bench::{report::Revs, Config, Paired};
use serde_json::json;

fn cfg() -> Config {
    let reps = std::env::var("REPS").ok().and_then(|s| s.parse().ok()).unwrap_or(20);
    Config { repetitions: reps, revs: Revs { old: std::env::var("OLD_REV").ok(), new: std::env::var("NEW_REV").ok() }, ..Config::default() }
}

/// Comparable form of a decode result: Ok((w, h, components, samples as u16 words, damaged)) or the error text.
type Out = Result<(u32, u32, u8, Vec<u16>, bool), String>;

fn dec_old(b: &Vec<u8>) -> Out {
    let d = old_jpeg::decode(b, &old_jpeg::Limits::default(), None).map_err(|e| e.to_string())?;
    let s = match d.samples { old_jpeg::Samples::U8(v) => v.into_iter().map(u16::from).collect(), old_jpeg::Samples::U16(v) => v };
    Ok((d.width, d.height, d.components, s, d.damaged))
}
fn dec_new(b: &Vec<u8>) -> Out {
    let d = new_jpeg::decode(b, &new_jpeg::Limits::default(), None).map_err(|e| e.to_string())?;
    let s = match d.samples { new_jpeg::Samples::U8(v) => v.into_iter().map(u16::from).collect(), new_jpeg::Samples::U16(v) => v };
    Ok((d.width, d.height, d.components, s, d.damaged))
}

fn files(dir: &str) -> Vec<(String, Vec<u8>)> {
    let mut p: Vec<_> = std::fs::read_dir(dir).expect("dir").filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "jpg")).collect();
    p.sort();
    p.into_iter().map(|p| (p.file_name().unwrap().to_string_lossy().into_owned(), std::fs::read(&p).unwrap())).collect()
}

/// Smooth structure plus noise, so the entropy-coded data is dense in AC symbols (the case the lookahead table helps).
fn synth(w: usize, h: usize, noise: u32, gray: bool, quality: u8) -> Vec<u8> {
    let mut s = 12345u64;
    let mut px = Vec::with_capacity(w * h * 3);
    for y in 0..h {
        for x in 0..w {
            let base = 128.0 + 90.0 * ((x as f32 / 37.0).sin() * (y as f32 / 53.0).cos());
            let n = (lcg(&mut s) % (2 * noise as u64 + 1)) as f32 - noise as f32;
            let v = (base + n).clamp(0.0, 255.0) as u8;
            if gray { px.push(v) } else { px.extend_from_slice(&[v, (v as i32 + (x as i32 % 64) - 32).clamp(0, 255) as u8, (255 - v)]) }
        }
    }
    let mut out = Vec::new();
    let enc = jpeg_encoder::Encoder::new(&mut out, quality);
    enc.encode(&px, w as u16, h as u16, if gray { jpeg_encoder::ColorType::Luma } else { jpeg_encoder::ColorType::Rgb }).expect("encode");
    out
}

fn group(out: &str, label: &str, desc: &str, inputs: Vec<(String, Vec<u8>)>) {
    let mut keep = Vec::new();
    let mut dropped = Vec::new();
    let mut both_err = Vec::new();
    for (n, b) in inputs {
        match (dec_old(&b), dec_new(&b)) {
            (Ok(a), Ok(c)) if a == c => keep.push((n, b)),
            (Err(a), Err(c)) if a == c => both_err.push(json!({"input": n, "error": a})),
            (a, c) => dropped.push(json!({"input": n, "reason": format!("old/new differ: old ok={} new ok={}", a.is_ok(), c.is_ok())})),
        }
    }
    let n = keep.len();
    let bytes: usize = keep.iter().map(|(_, b)| b.len()).sum();
    let rep = Paired::new(format!("craft-jpeg decode: {desc}")).old(dec_old).new(dec_new).inputs(keep).check(|a: &Out, b: &Out| a == b).run(&cfg()).expect("run");
    write(out, label, &rep, json!({"claim": "C10 craft-jpeg Huffman 1.77x (kernel-only, 13,107,000 symbols, prefix-span weighted; the ticket says it is not an image-decode benchmark)", "input_set": desc, "n_inputs": n, "total_input_bytes": bytes, "excluded_output_differs": dropped, "failed_identically_on_both_excluded": both_err, "equality": "decoded samples, dimensions, components and damaged flag identical (bit-for-bit), or identical error", "operation": "craft_jpeg::decode(bytes, Limits::default(), None)"}));
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let (out, turbo, office) = (a[1].as_str(), a[2].as_str(), a[3].as_str());
    group(out, "C10-jpeg-libjpeg-turbo", "libjpeg-turbo 3.0.0 test images (crates/jpeg/tests/data)", files(turbo));
    group(out, "C10-jpeg-office-real", "real JPEGs from the office corpus (word/media), 52 files", files(office));
    group(out, "C10-jpeg-synthetic", "synthetic encoder-generated images (jpeg-encoder 0.6), dense AC content", vec![
        ("rgb 1024x1024 q90 noise16".into(), synth(1024, 1024, 16, false, 90)),
        ("rgb 4000x3000 q90 noise16".into(), synth(4000, 3000, 16, false, 90)),
        ("rgb 4000x3000 q95 noise48".into(), synth(4000, 3000, 48, false, 95)),
        ("gray 4000x3000 q95 noise48".into(), synth(4000, 3000, 48, true, 95)),
    ]);
}
