//! av1 conformance suite (see the crate docs). Run: `cargo test --release -p craft-conformance --test av1 -- --nocapture`.

use craft_conformance::av1;

const KNOWN_FAILURES: &[(&str, &str)] = &[];
/// Floor: raise it when more streams pass, never lower it.
const MIN_PASS: usize = 244;

#[test]
fn av1_conformance() {
    let Some(suite) = av1::run() else { return };
    suite.assert_clean(KNOWN_FAILURES, MIN_PASS);
}
