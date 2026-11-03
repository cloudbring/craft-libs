use std::path::PathBuf;

pub fn ffmpeg_or_skip(context: &str) -> Option<PathBuf> {
    craft_media_testkit::ffmpeg().or_else(|| craft_media_testkit::skip(&format!("{context}: ffmpeg not found")))
}

pub fn fixtures_dir(name: &str) -> PathBuf {
    craft_media_testkit::fixture_subdir(name)
}

pub fn vectors_dir() -> Option<PathBuf> {
    let directory = fixtures_dir("opus/vectors");
    for vector in 1..=12 {
        for suffix in [".bit", ".dec", "m.dec"] {
            let path = directory.join(format!("testvector{vector:02}{suffix}"));
            if !std::fs::metadata(&path).is_ok_and(|metadata| metadata.is_file() && metadata.len() > 0) {
                return craft_media_testkit::skip(&format!("Opus conformance vector missing: {}; see crates/opus/README.md", path.display()));
            }
        }
    }
    Some(directory)
}
