use craft_view::{Anchor, ViewXform, ZoomPolicy};
use emath::{Rect, pos2, vec2};
use proptest::prelude::*;

fn policy() -> ZoomPolicy {
    ZoomPolicy { min: 0.01, max: 100.0, steps: &[0.1, 0.5, 1.0, 2.0, 10.0], anchor: Anchor::Pointer }
}

fn view(zoom: f32, rotation: f32, ppp: f32) -> ViewXform {
    ViewXform { rect: Rect::from_min_max(pos2(10.0, 20.0), pos2(810.0, 620.0)), centre: pos2(12.0, -18.0), zoom, rotation, ppp }
}

proptest! {
    #[test]
    fn round_trip(x in -1000f32..1000.0, y in -1000f32..1000.0, zoom in 0.1f32..10.0, rotation in -std::f32::consts::PI..std::f32::consts::PI, ppp in prop::sample::select(vec![1.0, 1.5, 2.0])) {
        let xf = view(zoom, rotation, ppp);
        let point = pos2(x, y);
        prop_assert!((xf.to_screen(xf.to_doc(point)) - point).length() < 0.001);
    }
    #[test]
    fn zoom_preserves_anchor(x in 10f32..810.0, y in 20f32..620.0, zoom in 0.2f32..10.0, new_zoom in 0.2f32..10.0, rotation in -std::f32::consts::PI..std::f32::consts::PI, ppp in prop::sample::select(vec![1.0, 1.5, 2.0])) {
        let mut xf = view(zoom, rotation, ppp);
        let anchor = pos2(x, y);
        let doc = xf.to_doc(anchor);
        xf.zoom_about(anchor, new_zoom, &policy());
        prop_assert!((xf.to_screen(doc) - anchor).length() < 0.001);
    }
    #[test]
    fn steps_are_monotone(zoom in 0.01f32..100.0) {
        let policy = policy();
        prop_assert!(policy.next_step_up(zoom) >= zoom);
        prop_assert!(policy.next_step_down(zoom) <= zoom);
    }
}

#[test]
fn endpoints_and_app_ranges() {
    for (min, max) in [(0.0001, 32.0), (0.08, 64.0)] {
        let policy = ZoomPolicy { min, max, steps: &[], anchor: Anchor::Pointer };
        assert_eq!(policy.clamp(0.0), min);
        assert_eq!(policy.clamp(1000.0), max);
        assert_eq!(policy.next_step_up(max), max);
        assert_eq!(policy.next_step_down(min), min);
    }
}

#[test]
fn fit_rotated_rect_and_other_operations() {
    let mut xf = view(1.0, 0.7, 1.5);
    let rect = Rect::from_min_max(pos2(-120.0, -40.0), pos2(90.0, 230.0));
    xf.fit(rect, 20.0, &policy());
    let bounds = xf.doc_rect_to_screen(rect);
    assert!(xf.rect.shrink(19.99).contains_rect(bounds));
    let anchor = pos2(150.0, 250.0);
    let doc = xf.to_doc(anchor);
    xf.rotate_about(anchor, 0.4);
    assert!((xf.to_screen(doc) - anchor).length() < 0.001);
    let old = xf.to_screen(doc);
    xf.pan_pixels(vec2(15.0, -30.0));
    assert!((xf.to_screen(doc) - old - vec2(10.0, -20.0)).length() < 0.001);
    let clicked = xf.to_doc(anchor);
    xf.zoom_about_and_centre(anchor, 2.0, &policy());
    assert!((xf.centre - clicked).length() < 0.001);
    assert!(xf.as_tstransform().is_none());
    xf.rotation = 0.0;
    if let Some(transform) = xf.as_tstransform() {
        assert!((transform * doc - xf.to_screen(doc)).length() < 0.001);
    }
    assert!(xf.as_tstransform().is_some());
    assert!(xf.visible_doc_rect().contains(xf.to_doc(xf.rect.min)));
}

#[test]
fn anchor_modes_and_clamping() {
    let mut xf = view(1.0, 0.3, 2.0);
    let mut policy = policy();
    policy.anchor = Anchor::Centre;
    let centre = xf.centre;
    xf.zoom_about(pos2(50.0, 70.0), 1000.0, &policy);
    assert_eq!(xf.centre, centre);
    assert_eq!(xf.zoom, policy.max);
    let point = pos2(300.0, 400.0);
    let doc = xf.to_doc(point);
    policy.anchor = Anchor::Point(point);
    xf.zoom_about(pos2(0.0, 0.0), 0.0, &policy);
    assert_eq!(xf.zoom, policy.min);
    assert!((xf.to_screen(doc) - point).length() < 0.001);
}
