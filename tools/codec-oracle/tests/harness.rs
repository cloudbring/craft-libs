//! Self-checks of the oracle harness: it must see a planted difference, call equal things equal, and
//! turn an old-side panic into a classified trace rather than a crash. No fixtures needed.

use codec_oracle::judge::{diff, judge};
use codec_oracle::sides::lookup;
use codec_oracle::synth_h264;
use codec_oracle::trace::{End, Trace};
use craft_compare::Verdict;

fn frame(t: &mut Trace, fill: u8) {
    t.push_frame(2, 2, 8, 0, true, &[&[fill; 4], &[fill; 1], &[fill; 1]]);
}

#[test]
fn identical_traces_are_identical() {
    let (mut a, mut b) = (Trace::new(), Trace::new());
    frame(&mut a, 7);
    frame(&mut b, 7);
    assert_eq!(judge(&a, &b), Verdict::Identical);
}

#[test]
fn one_changed_sample_is_a_plane_difference() {
    let (mut a, mut b) = (Trace::new(), Trace::new());
    frame(&mut a, 7);
    frame(&mut b, 8);
    let d = diff(&a, &b);
    assert_eq!(d.aspects, vec!["plane_bytes".to_string()]);
    assert!(matches!(judge(&a, &b), Verdict::Exceeds { .. }));
}

#[test]
fn ending_and_count_differences_are_named() {
    let (mut a, mut b) = (Trace::new(), Trace::new());
    frame(&mut a, 1);
    b.end = End::Panic { msg: "attempt to add with overflow".into(), loc: "x/slice.rs:1".into() };
    let d = diff(&a, &b);
    assert_eq!(d.aspects, vec!["end_ok_panic".to_string(), "frames".to_string()]);
}

#[test]
fn pcm_difference_reports_the_largest_error() {
    let (mut a, mut b) = (Trace::new(), Trace::new());
    a.pcm = vec![0.0, 0.5, -0.25];
    b.pcm = vec![0.0, 0.5 + 1e-6, -0.25];
    let d = diff(&a, &b);
    assert_eq!(d.aspects, vec!["pcm_bytes".to_string()]);
    assert!((d.value - 1e-6).abs() < 1e-7);
}

/// The planted difference: a POC offset overflow panics the unhardened filmcraft fork (overflow
/// checks are on in this build) and decodes in the shared crate.
#[test]
fn old_fork_panic_is_classified_and_new_decodes() {
    let corpus = synth_h264::corpus();
    let (_, bytes) = corpus.iter().find(|(n, _)| n == "poc1-n0-t0-c1-d2147483647").expect("stream");
    let old = lookup::fc("h264", true).expect("fc")(bytes).expect("trace");
    let new = lookup::new("h264", true).expect("new")(bytes).expect("trace");
    assert!(matches!(old.end, End::Panic { .. }), "old: {:?}", old.end);
    assert_eq!(new.end, End::Complete);
    assert!(matches!(judge(&old, &new), Verdict::Exceeds { .. }));
}

/// Valid synthetic stream: every h264 implementation produces the same pictures.
#[test]
fn valid_synthetic_stream_is_identical_everywhere() {
    let corpus = synth_h264::corpus();
    let (_, bytes) = corpus.iter().find(|(n, _)| n == "qp-idr-t2-0").expect("stream");
    let new = lookup::new("h264", true).expect("new")(bytes).expect("trace");
    assert_eq!(new.end, End::Complete);
    assert_eq!(new.frames.len(), 2);
    for f in [lookup::fc("h264", true), lookup::dc("h264", true), lookup::sc("h264", true), lookup::pre("h264", true)] {
        let t = f.expect("driver")(bytes).expect("trace");
        assert_eq!(judge(&t, &new), Verdict::Identical);
    }
}
