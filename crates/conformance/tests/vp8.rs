//! vp8 conformance suite (see the crate docs). Run: `cargo test --release -p craft-conformance --test vp8 -- --nocapture`.

use craft_conformance::vp8;

const KNOWN_FAILURES: &[(&str, &str)] = &[];
/// Floor: raise it when more streams pass, never lower it.
const MIN_PASS: usize = 62;

#[test]
fn vp8_conformance() {
    let Some(suite) = vp8::run() else { return };
    suite.assert_clean(KNOWN_FAILURES, MIN_PASS);
}
