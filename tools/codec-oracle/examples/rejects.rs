//! List the streams of a directory that the shared decoder ends with an error (name and message).
//! Usage: rejects <codec: h264|hevc|av1|vp8|vp9> <dir> <ext>
use codec_oracle::{inputs, sides::lookup, trace::End};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let f = lookup::new(&a[1], false).expect("driver");
    for (name, bytes) in inputs::read_all(inputs::list(std::path::Path::new(&a[2]), &a[3])) {
        let t = f(&bytes).expect("trace");
        if let End::Error { msg, at, .. } = &t.end {
            println!("{name}: error at unit {at} after {} pictures: {msg}", t.frames.len());
        }
    }
}
