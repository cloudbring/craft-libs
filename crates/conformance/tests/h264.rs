//! h264 conformance suite (see the crate docs). Run: `cargo test --release -p craft-conformance --test h264 -- --nocapture`.

use craft_conformance::h264;

const KNOWN_FAILURES: &[(&str, &str)] = &[];
/// Floor: raise it when more streams pass, never lower it.
const MIN_PASS: usize = 99;

#[test]
fn h264_conformance() {
    let Some(suite) = h264::run() else { return };
    suite.assert_clean(KNOWN_FAILURES, MIN_PASS);
}
