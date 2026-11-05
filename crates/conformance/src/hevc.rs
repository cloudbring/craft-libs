//! HEVC conformance bitstreams (JCT-VC, H.265.1) against `expected/hevc.tsv`.
//!
//! Supported: Main and Main 10 (4:2:0, 8 and 10 bit). The decoder returns `Error::Unsupported` for
//! 4:2:2 / 4:4:4 / monochrome, RExt tools and 12 bit and above.

use crate::{Files, Md5, Suite, Verdict, check_whole, feed_plane_u8, feed_plane_u16, guarded, load, par_map, parse_expected, rejected_or_fail};
use craft_hevc::{Decoder, Error, Picture, Plane};

const EXPECTED: &str = include_str!("../expected/hevc.tsv");

pub fn run() -> Option<Suite> {
    let files = load("hevc")?;
    let expected = parse_expected(EXPECTED);
    let streams: Vec<&String> = files.names.iter().collect();
    let results = par_map(&streams, |n| ((*n).clone(), guarded(|| check(&files, n, &expected))));
    Some(Suite { codec: "hevc".into(), results })
}

fn is_vcl(t: u8) -> bool {
    t < 32
}

/// Access units: a new one starts at an AUD / VPS / SPS / PPS / prefix SEI after a VCL NAL, or at a
/// VCL NAL with `first_slice_segment_in_pic_flag` set.
pub fn split_access_units(data: &[u8]) -> Vec<&[u8]> {
    let mut starts = Vec::new();
    let mut i = 0;
    while i + 3 <= data.len() {
        if data[i] == 0 && data[i + 1] == 0 && data[i + 2] == 1 {
            let s = if i > 0 && data[i - 1] == 0 { i - 1 } else { i };
            starts.push((s, i + 3));
            i += 3;
        } else {
            i += 1;
        }
    }
    let mut aus = Vec::new();
    let mut au_start = 0usize;
    let mut seen_vcl = false;
    for &(sc, payload) in &starts {
        if payload + 2 >= data.len() {
            continue;
        }
        let t = (data[payload] >> 1) & 0x3f;
        let first_slice = is_vcl(t) && data[payload + 2] & 0x80 != 0;
        if seen_vcl && (matches!(t, 32..=35 | 39) || first_slice) {
            aus.push(&data[au_start..sc]);
            au_start = sc;
            seen_vcl = false;
        }
        if is_vcl(t) {
            seen_vcl = true;
        }
    }
    if au_start < data.len() {
        aus.push(&data[au_start..]);
    }
    aus
}

fn hash_pictures(pics: &[Picture]) -> String {
    let mut m = Md5::new();
    for p in pics {
        for (plane, w, h, stride) in [
            (&p.y, p.width, p.height, p.y_stride),
            (&p.u, p.chroma_width, p.chroma_height, p.uv_stride),
            (&p.v, p.chroma_width, p.chroma_height, p.uv_stride),
        ] {
            match plane {
                Plane::U8(v) => feed_plane_u8(&mut m, v, stride, w as usize, h as usize),
                Plane::U16(v) => feed_plane_u16(&mut m, v, stride, w as usize, h as usize),
            }
        }
    }
    m.finish_hex()
}

fn check(files: &Files, name: &str, expected: &std::collections::BTreeMap<String, (Option<usize>, String)>) -> Verdict {
    let data = files.read(name);
    let mut dec = Decoder::new();
    let mut pics = Vec::new();
    for (i, au) in split_access_units(&data).into_iter().enumerate() {
        match dec.decode(au, i as i64) {
            Ok(p) => pics.extend(p),
            Err(Error::Unsupported(m)) => return Verdict::Unsupported(m),
            Err(e) => return rejected_or_fail(name, expected, format!("access unit {i}: {e}")),
        }
    }
    pics.extend(dec.flush());
    match dec.take_error() {
        Some(Error::Unsupported(m)) => return Verdict::Unsupported(m),
        Some(e) => return rejected_or_fail(name, expected, format!("deferred error: {e}")),
        None => {}
    }
    check_whole(name, expected, pics.len(), &hash_pictures(&pics))
}
