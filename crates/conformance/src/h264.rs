//! H.264 conformance bitstreams (JVT, H.264.1) against `expected/h264.tsv`.
//!
//! Unsupported (the crate README, "Known gaps"): interlaced coding, anything but 8-bit 4:2:0,
//! lossless, FMO, SP/SI, data partitioning. The decoder returns `Error::Unsupported` for the
//! first two groups; [`feature_unsupported`] recognises the stream-level markers of the others.

use crate::{Files, Md5, Suite, Verdict, check_whole, feed_plane_u8, guarded, load, par_map, parse_expected, rejected_or_fail};
use craft_h264::{Decoder, Error, Picture};

const EXPECTED: &str = include_str!("../expected/h264.tsv");

pub fn run() -> Option<Suite> {
    let files = load("h264")?;
    let expected = parse_expected(EXPECTED);
    let streams: Vec<&String> = files.names.iter().collect();
    let results = par_map(&streams, |n| ((*n).clone(), guarded(|| check(&files, n, &expected))));
    Some(Suite { codec: "h264".into(), results })
}

/// Splits an Annex-B stream into `(start code offset, payload offset)` pairs.
fn nal_starts(data: &[u8]) -> Vec<(usize, usize)> {
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
    starts
}

/// Access units: a new one starts at an AUD/SEI/SPS/PPS after a slice or at a slice with
/// `first_mb_in_slice == 0` (as the crate's own tests split).
pub fn split_access_units(data: &[u8]) -> Vec<&[u8]> {
    let mut aus = Vec::new();
    let mut au_start = 0usize;
    let mut seen_slice = false;
    for (sc, payload) in nal_starts(data) {
        if payload >= data.len() {
            continue;
        }
        let t = data[payload] & 0x1f;
        let is_slice = t == 1 || t == 5;
        let first_mb_zero = is_slice && payload + 1 < data.len() && data[payload + 1] & 0x80 != 0;
        if seen_slice && (matches!(t, 6..=9) || first_mb_zero) {
            aus.push(&data[au_start..sc]);
            au_start = sc;
            seen_slice = false;
        }
        if is_slice {
            seen_slice = true;
        }
    }
    if au_start < data.len() {
        aus.push(&data[au_start..]);
    }
    aus
}

/// NAL unit types present in the stream.
pub fn nal_types(data: &[u8]) -> std::collections::BTreeSet<u8> {
    nal_starts(data).into_iter().filter(|&(_, p)| p < data.len()).map(|(_, p)| data[p] & 0x1f).collect()
}

/// Stream-level markers of README-listed gaps the decoder does not itself reject: data
/// partitioning (NAL types 2 to 4) and SVC/MVC enhancement layers (types 14, 15, 20).
pub fn feature_unsupported(data: &[u8]) -> Option<&'static str> {
    let t = nal_types(data);
    if t.iter().any(|x| (2..=4).contains(x)) {
        return Some("data partitioning (Extended profile)");
    }
    None
}

fn hash_pictures(pics: &[Picture]) -> String {
    let mut m = Md5::new();
    for p in pics {
        feed_plane_u8(&mut m, &p.y, p.y_stride, p.width as usize, p.height as usize);
        feed_plane_u8(&mut m, &p.u, p.uv_stride, p.chroma_width as usize, p.chroma_height as usize);
        feed_plane_u8(&mut m, &p.v, p.uv_stride, p.chroma_width as usize, p.chroma_height as usize);
    }
    m.finish_hex()
}

fn check(files: &Files, name: &str, expected: &std::collections::BTreeMap<String, (Option<usize>, String)>) -> Verdict {
    let data = files.read(name);
    if let Some(why) = feature_unsupported(&data) {
        return Verdict::Unsupported(why.into());
    }
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
