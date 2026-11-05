//! vp9 conformance suite (see the crate docs). Run: `cargo test --release -p craft-conformance --test vp9 -- --nocapture`.

use craft_conformance::vp9;

/// Streams the decoder rejects although libvpx decodes them (docs/verification/codec-conformance.md).
const KNOWN_FAILURES: &[(&str, &str)] = &[
    // MAX dimension 16384 in decoder.rs (hostile-input guard, filmcraft #2930); the VP9 limit is 65536.
    ("vp90-2-13-largescaling.webm", "frame width 19200 exceeds the decoder's 16384 limit"),
    // Strict exit_bool: a tile partition read past its end (libvpx zero-fills); passes with the check off.
    ("vp90-2-15-fuzz-flicker.webm", "boolean partition exhausted (strict, libvpx tolerates)"),
    // Not decodable by ffmpeg (native or libvpx wrapper) either; its md5 does not match a lenient decode.
    ("vp91-2-04-yv444.webm", "nonzero boolean padding; reference md5 not reproducible"),
];
/// Floor: raise it when more streams pass, never lower it.
const MIN_PASS: usize = 340;

#[test]
fn vp9_conformance() {
    let Some(suite) = vp9::run() else { return };
    suite.assert_clean(KNOWN_FAILURES, MIN_PASS);
}
