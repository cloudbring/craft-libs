//! Is a decoder deterministic on damaged streams? Runs the default (multi-threaded) driver twice
//! on the same hostile mutants and counts runs whose outcome differs from the first. The oracle
//! decodes hostile sets single-threaded for this reason. Usage: determinism <fixture root>
use codec_oracle::{inputs, judge, matrix, sides::lookup};

fn main() {
    let root = std::path::PathBuf::from(std::env::args().nth(1).expect("fixture root"));
    for codec in ["h264", "hevc", "av1", "vp9"] {
        let dir = root.join(codec);
        let ext = matrix::ext(codec);
        let bases = inputs::mutant_bases(&dir, ext, 8, 400_000);
        let streams: Vec<_> = inputs::mutant_stream(bases).collect();
        for (side, f) in [("new", lookup::new(codec, false)), ("filmcraft", lookup::fc(codec, false))] {
            let f = f.expect("driver");
            let mut differ = 0;
            for (_, bytes) in &streams {
                let (a, b) = (f(bytes).expect("trace"), f(bytes).expect("trace"));
                if !judge::diff(&a, &b).aspects.is_empty() {
                    differ += 1;
                }
            }
            println!("{codec} {side}: {differ} of {} damaged streams decode differently on a second run (default threads)", streams.len());
        }
    }
}
