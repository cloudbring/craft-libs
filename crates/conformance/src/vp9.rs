//! libvpx VP9 vectors (`vp90/91/92/93-2-*.webm`) against the per-frame MD5s of the `.md5` files:
//! the display-size planes in the stream's native format (16-bit little-endian above 8 bit).

use crate::{Files, Md5, Suite, Verdict, check_frames, feed_plane_u8, feed_plane_u16, guarded, load, matroska_video_packets, md5_lines, par_map};
use craft_vp9::{Decoder, Error, Picture, Plane};

pub fn run() -> Option<Suite> {
    let files = load("vp9")?;
    let streams: Vec<&String> = files.names.iter().filter(|n| n.ends_with(".webm")).collect();
    let results = par_map(&streams, |n| ((*n).clone(), guarded(|| check(&files, n))));
    Some(Suite { codec: "vp9".into(), results })
}

fn frame_md5(p: &Picture) -> String {
    let mut m = Md5::new();
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
    m.finish_hex()
}

fn check(files: &Files, name: &str) -> Verdict {
    let want = md5_lines(&String::from_utf8_lossy(&files.read(&format!("{name}.md5"))));
    let packets = match matroska_video_packets(&files.read(name)) {
        Ok(p) => p,
        Err(e) => return Verdict::Fail(e),
    };
    let mut dec = Decoder::new();
    let mut got = Vec::new();
    for (i, f) in packets.iter().enumerate() {
        match dec.decode(f, i as i64) {
            Ok(pics) => got.extend(pics.iter().map(frame_md5)),
            Err(Error::Unsupported(m)) => return Verdict::Unsupported(m),
            Err(e) => return Verdict::Fail(format!("packet {i}: {e}")),
        }
    }
    got.extend(dec.flush().iter().map(frame_md5));
    check_frames(&got, &want)
}
