//! libvpx VP8 vectors: `vp80-*.ivf` against the per-frame I420 MD5s of the `.md5` files.

use crate::{Files, Md5, Suite, Verdict, check_frames, guarded, ivf_frames, load, md5_lines, par_map};

pub fn run() -> Option<Suite> {
    let files = load("vp8")?;
    let streams: Vec<&String> = files.names.iter().filter(|n| n.ends_with(".ivf")).collect();
    let results = par_map(&streams, |n| ((*n).clone(), guarded(|| check(&files, n))));
    Some(Suite { codec: "vp8".into(), results })
}

fn check(files: &Files, name: &str) -> Verdict {
    let want = md5_lines(&String::from_utf8_lossy(&files.read(&format!("{name}.md5"))));
    let data = files.read(name);
    let mut dec = craft_vp8::Decoder::new();
    let mut got = Vec::new();
    for (i, f) in ivf_frames(&data).into_iter().enumerate() {
        match dec.decode(f, i as i64) {
            Ok(Some(p)) => {
                let mut m = Md5::new();
                m.update(&p.y);
                m.update(&p.u);
                m.update(&p.v);
                got.push(m.finish_hex());
            }
            Ok(None) => {}
            Err(e) => return Verdict::Fail(format!("frame {i}: {e}")),
        }
    }
    check_frames(&got, &want)
}
