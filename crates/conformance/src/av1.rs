//! libaom AV1 vectors (`av1-1-*.ivf`, `.mkv`) against the per-frame MD5s of the `.md5` files.
//! Monochrome frames are hashed with 4:2:0 chroma planes of mid-grey, as libaom writes them.

use crate::{
    Files, Md5, Suite, Verdict, check_frames, feed_plane_u8, feed_plane_u16, guarded, ivf_frames, load, matroska_video_packets, md5_lines, par_map,
};
use craft_av1::{Decoder, Error, Picture};

pub fn run() -> Option<Suite> {
    let files = load("av1")?;
    let streams: Vec<&String> = files.names.iter().filter(|n| n.ends_with(".ivf") || n.ends_with(".mkv")).collect();
    let results = par_map(&streams, |n| ((*n).clone(), guarded(|| check(&files, n))));
    Some(Suite { codec: "av1".into(), results })
}

fn frame_md5(p: &Picture) -> String {
    let mut m = Md5::new();
    let high = p.bit_depth > 8;
    for plane in 0..3 {
        let (w, h) =
            if p.mono_chrome && plane > 0 { (p.width.div_ceil(2), p.height.div_ceil(2)) } else { (p.plane_width(plane), p.plane_height(plane)) };
        let (w, h) = (w as usize, h as usize);
        let samples: Vec<u16> = if p.mono_chrome && plane > 0 { vec![1 << (p.bit_depth - 1); w * h] } else { p.planes[plane].clone() };
        if high {
            feed_plane_u16(&mut m, &samples, w, w, h);
        } else {
            let bytes: Vec<u8> = samples.iter().map(|&s| s as u8).collect();
            feed_plane_u8(&mut m, &bytes, w, w, h);
        }
    }
    m.finish_hex()
}

fn check(files: &Files, name: &str) -> Verdict {
    if !files.names.iter().any(|n| *n == format!("{name}.md5")) {
        return Verdict::Fail("no .md5 published for this vector".into());
    }
    let want = md5_lines(&String::from_utf8_lossy(&files.read(&format!("{name}.md5"))));
    let data = files.read(name);
    let packets: Vec<Vec<u8>> = if name.ends_with(".mkv") {
        match matroska_video_packets(&data) {
            Ok(p) => p,
            Err(e) => return Verdict::Fail(e),
        }
    } else {
        ivf_frames(&data).into_iter().map(<[u8]>::to_vec).collect()
    };
    let mut dec = Decoder::new();
    let mut got = Vec::new();
    for (i, f) in packets.iter().enumerate() {
        match dec.decode_pts(f, i as i64) {
            Ok(pics) => got.extend(pics.iter().map(frame_md5)),
            Err(Error::Unsupported(m)) => return Verdict::Unsupported(m.to_string()),
            Err(e) => return Verdict::Fail(format!("temporal unit {i}: {e}")),
        }
    }
    match dec.flush_result() {
        Ok(pics) => got.extend(pics.iter().map(frame_md5)),
        Err(e) => return Verdict::Fail(format!("flush: {e}")),
    }
    check_frames(&got, &want)
}
