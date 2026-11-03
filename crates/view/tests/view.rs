use craft_view::{Anchor, ViewXform, ZoomPolicy, doc_point, doc_tuple};
use emath::{Rect, pos2, vec2};
use proptest::prelude::*;

fn policy() -> ZoomPolicy {
    ZoomPolicy { min: 0.01, max: 100.0, steps: &[0.1, 0.5, 1.0, 2.0, 10.0], anchor: Anchor::Pointer }
}

fn view(zoom: f64, rotation: f64, ppp: f32) -> ViewXform {
    ViewXform { rect: Rect::from_min_max(pos2(10.0, 20.0), pos2(810.0, 620.0)), centre: [12.0, -18.0], zoom, rotation, ppp, aspect: 1.0 }
}

proptest! {
    #[test]
    fn round_trip(x in -1000f32..1000.0, y in -1000f32..1000.0, zoom in 0.1f64..10.0, rotation in -std::f64::consts::PI..std::f64::consts::PI, ppp in prop::sample::select(vec![1.0, 1.5, 2.0])) {
        let xf = view(zoom, rotation, ppp);
        let point = pos2(x, y);
        prop_assert!((xf.to_screen(xf.to_doc(point)) - point).length() < 0.001);
    }
    #[test]
    fn zoom_preserves_anchor(x in 10f32..810.0, y in 20f32..620.0, zoom in 0.2f64..10.0, new_zoom in 0.2f64..10.0, rotation in -std::f64::consts::PI..std::f64::consts::PI, ppp in prop::sample::select(vec![1.0, 1.5, 2.0])) {
        let mut xf = view(zoom, rotation, ppp);
        let anchor = pos2(x, y);
        let doc = xf.to_doc(anchor);
        xf.zoom_about(anchor, new_zoom, &policy());
        prop_assert!((xf.to_screen(doc) - anchor).length() < 0.001);
    }
    #[test]
    fn steps_are_monotone(zoom in 0.01f64..100.0) {
        let policy = policy();
        prop_assert!(policy.next_step_up(zoom) >= zoom);
        prop_assert!(policy.next_step_down(zoom) <= zoom);
    }
    #[test]
    fn pan_moves_image(dx in -100f64..100.0, dy in -100f64..100.0, rotation in -std::f64::consts::PI..std::f64::consts::PI, aspect in 0.5f64..2.0, ppp in prop::sample::select(vec![1.0, 1.5, 2.0])) {
        let mut xf = view(1.7, rotation, ppp);
        xf.aspect = aspect;
        let point = xf.centre;
        let before = xf.to_screen(point);
        xf.pan_pixels(ppp, (dx, dy));
        let displacement = (xf.to_screen(point) - before) * ppp;
        prop_assert!((f64::from(displacement.x) - dx).abs() < 0.001);
        prop_assert!((f64::from(displacement.y) - dy).abs() < 0.001);
    }
    #[test]
    fn fit_fill_order(rotation in -std::f64::consts::PI..std::f64::consts::PI, aspect in 0.5f64..2.0) {
        let mut xf = view(1.0, rotation, 1.5);
        xf.aspect = aspect;
        let rect = [[-120.0, -40.0], [90.0, 230.0]];
        xf.fit(rect, 20.0, &policy());
        let fit_zoom = xf.zoom;
        prop_assert!(xf.rect.shrink(19.99).contains_rect(xf.doc_rect_to_screen(rect)));
        xf.fill(rect, &policy());
        prop_assert!(xf.zoom >= fit_zoom);
        let bounds = xf.visible_doc_rect();
        prop_assert!(bounds[0][0] >= rect[0][0] - 0.001 && bounds[0][1] >= rect[0][1] - 0.001);
        prop_assert!(bounds[1][0] <= rect[1][0] + 0.001 && bounds[1][1] <= rect[1][1] + 0.001);
    }
    #[test]
    fn aspect_round_trip(x in -100f64..100.0, y in -100f64..100.0, aspect in 0.5f64..2.0) {
        let mut xf = view(1.0, 0.0, 1.0);
        xf.rect = Rect::from_center_size(pos2(0.0, 0.0), vec2(800.0, 600.0));
        xf.centre = [0.0, 0.0];
        xf.aspect = aspect;
        let point = [x, y];
        let result = xf.to_doc(xf.to_screen(point));
        prop_assert!((result[0] - x).abs() < 1e-5 && (result[1] - y).abs() < 1e-5);
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
    let rect = [[-120.0, -40.0], [90.0, 230.0]];
    xf.fit(rect, 20.0, &policy());
    assert!(xf.rect.shrink(19.99).contains_rect(xf.doc_rect_to_screen(rect)));
    let anchor = pos2(150.0, 250.0);
    let doc = xf.to_doc(anchor);
    xf.rotate_about(anchor, 0.4);
    assert!((xf.to_screen(doc) - anchor).length() < 0.001);
    let clicked = xf.to_doc(anchor);
    xf.zoom_about_and_centre(anchor, 2.0, &policy());
    assert_eq!(xf.centre, clicked);
    assert!(xf.as_tstransform().is_none());
    xf.rotation = 0.0;
    assert!(xf.as_tstransform().is_some());
    xf.set_rotation_degrees(-90.0);
    assert!((xf.rotation_degrees() - 270.0).abs() < 1e-12);
    assert_eq!(doc_tuple(doc_point((2.0, 3.0))), (2.0, 3.0));
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

#[test]
fn document_round_trip_at_unit_zoom() {
    let mut xf = view(1.0, 0.0, 1.0);
    xf.rect = Rect::from_center_size(pos2(0.0, 0.0), vec2(800.0, 600.0));
    xf.centre = [0.0, 0.0];
    let point = [12.125, -18.25];
    let result = xf.to_doc(xf.to_screen(point));
    assert!((result[0] - point[0]).abs() < 1e-6 && (result[1] - point[1]).abs() < 1e-6);
}
