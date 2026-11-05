//! hevc conformance suite (see the crate docs). Run: `cargo test --release -p craft-conformance --test hevc -- --nocapture`.

use craft_conformance::hevc;

const KNOWN_FAILURES: &[(&str, &str)] = &[];
/// Floor: raise it when more streams pass, never lower it.
const MIN_PASS: usize = 170;

#[test]
fn hevc_conformance() {
    let Some(suite) = hevc::run() else { return };
    suite.assert_clean(KNOWN_FAILURES, MIN_PASS);
}
