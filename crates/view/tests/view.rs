use craft_view::{Anchor, DocPoint, DocRect, FitMode, Rotation, ViewXform, ZoomPolicy, doc_point, doc_tuple};
use emath::{Pos2, Rect, pos2, vec2};
use proptest::prelude::*;

fn policy() -> ZoomPolicy {
    ZoomPolicy { min: 0.01, max: 100.0, steps: &[0.1, 0.5, 1.0, 2.0, 10.0], anchor: Anchor::Pointer }
}

fn view(zoom: f64, rotation: f64, ppp: f32) -> ViewXform {
    ViewXform { rect: Rect::from_min_max(pos2(10.0, 20.0), pos2(810.0, 620.0)), centre: [12.0, -18.0], zoom, rotation, ppp, aspect: 1.0 }
}

/// Vendored copy of designcraft's origin-based, quarter-turn `Xf` with `f64` internal arithmetic
/// and `f32` (`Pos2`) screen coordinates.
#[derive(Clone, Copy)]
struct DesigncraftXf {
    min: [f64; 2],
    origin: DocPoint,
    zoom: f64,
    rot: u8,
    pivot: [f64; 2],
}

fn designcraft_quarter(p: [f64; 2], c: [f64; 2], q: u8) -> [f64; 2] {
    let (dx, dy) = (p[0] - c[0], p[1] - c[1]);
    let (x, y) = match q % 4 {
        0 => (dx, dy),
        1 => (-dy, dx),
        2 => (-dx, -dy),
        _ => (dy, -dx),
    };
    [c[0] + x, c[1] + y]
}

impl DesigncraftXf {
    fn new(rect: Rect, origin: DocPoint, zoom: f64, rotation: u8) -> Self {
        let min = [f64::from(rect.min.x), f64::from(rect.min.y)];
        let max = [f64::from(rect.max.x), f64::from(rect.max.y)];
        Self { min, origin, zoom, rot: rotation % 4, pivot: [0.5 * (min[0] + max[0]), 0.5 * (min[1] + max[1])] }
    }
    fn to_screen(self, p: DocPoint) -> Pos2 {
        let s = [self.min[0] + (p[0] - self.origin[0]) * self.zoom, self.min[1] + (p[1] - self.origin[1]) * self.zoom];
        let [x, y] = if self.rot == 0 { s } else { designcraft_quarter(s, self.pivot, self.rot) };
        pos2(x as f32, y as f32)
    }
    fn unrotate(self, p: [f64; 2]) -> [f64; 2] {
        if self.rot == 0 { p } else { designcraft_quarter(p, self.pivot, 4 - self.rot) }
    }
    fn to_canvas(self, p: Pos2) -> DocPoint {
        let p = self.unrotate([f64::from(p.x), f64::from(p.y)]);
        [self.origin[0] + (p[0] - self.min[0]) / self.zoom, self.origin[1] + (p[1] - self.min[1]) / self.zoom]
    }
}

/// Vendored copy of vectorcraft's `fit_zoom`.
fn vectorcraft_fit_zoom(rect: Rect, target: DocRect, rotation_degrees: f64) -> f64 {
    let (sn, cs) = rotation_degrees.to_radians().sin_cos();
    let (w, h) = (target[1][0] - target[0][0], target[1][1] - target[0][1]);
    let zx = (f64::from(rect.width()) - 60.0) / (w * cs.abs() + h * sn.abs()).max(1.0);
    let zy = (f64::from(rect.height()) - 60.0) / (w * sn.abs() + h * cs.abs()).max(1.0);
    zx.min(zy).clamp(0.0313, 640.0)
}

proptest! {
    #[test]
    fn round_trip(x in -1000f32..1000.0, y in -1000f32..1000.0, zoom in 0.1f64..10.0, rotation in -std::f64::consts::PI..std::f64::consts::PI, ppp in prop::sample::select(vec![1.0, 1.5, 2.0])) {
        let xf = view(zoom, rotation, ppp);
        let point = pos2(x, y);
        prop_assert!((xf.to_screen(xf.to_doc(point)) - point).length() < 0.001);
    }
    #[test]
    fn round_trip_f64_any_zoom(
        min_x in -1e5f32..1e5,
        min_y in -1e5f32..1e5,
        w in 1.0f32..3000.0,
        h in 1.0f32..2000.0,
        ox in -1e6f64..1e6,
        oy in -1e6f64..1e6,
        dx in -2000.0f64..2000.0,
        dy in -2000.0f64..2000.0,
        zoom in 0.05f64..40.0,
        rotation in -std::f64::consts::PI..std::f64::consts::PI,
        ppp in prop::sample::select(vec![1.0f32, 1.5, 2.0]),
        aspect in 0.5f64..2.0,
    ) {
        let rect = Rect::from_min_size(pos2(min_x, min_y), vec2(w, h));
        let xf = ViewXform::from_origin(rect, [ox, oy], zoom, rotation, ppp, aspect);
        let doc = [ox + dx, oy + dy];
        let screen = xf.to_screen_f64(doc);
        let round = xf.to_doc_f64(screen);
        prop_assert!((round[0] - doc[0]).abs() <= 1e-9 && (round[1] - doc[1]).abs() <= 1e-9);
    }
    #[test]
    fn designcraft_xf_identity(
        min_x in -1e5f32..1e5,
        min_y in -1e5f32..1e5,
        w in 1.0f32..3000.0,
        h in 1.0f32..2000.0,
        ox in -1e6f64..1e6,
        oy in -1e6f64..1e6,
        dx in -1000.0f64..2000.0,
        dy in -1000.0f64..2000.0,
        sx in -1e5f32..1e5,
        sy in -1e5f32..1e5,
        zoom in 0.05f64..40.0,
        factor in 0.25f64..4.0,
        rot in 0u8..4,
    ) {
        let rect = Rect::from_min_size(pos2(min_x, min_y), vec2(w, h));
        let origin = [ox, oy];
        let oracle = DesigncraftXf::new(rect, origin, zoom, rot);
        let mut xf = ViewXform::from_origin(rect, origin, zoom, Rotation::quarter(rot), 1.0, 1.0);
        let read_origin = xf.origin();
        prop_assert!((read_origin[0] - origin[0]).abs() <= 1e-9 && (read_origin[1] - origin[1]).abs() <= 1e-9);

        let point = [ox + dx, oy + dy];
        let delta_screen = xf.to_screen(point) - oracle.to_screen(point);
        prop_assert!(delta_screen.x.abs() <= 1e-3 && delta_screen.y.abs() <= 1e-3);

        let screen = pos2(sx, sy);
        let doc_actual = xf.to_doc(screen);
        let doc_expected = oracle.to_canvas(screen);
        prop_assert!((doc_actual[0] - doc_expected[0]).abs() <= 1e-9 && (doc_actual[1] - doc_expected[1]).abs() <= 1e-9);

        let zoom_policy = ZoomPolicy { min: 0.05, max: 40.0, steps: &[], anchor: Anchor::Pointer };
        xf.zoom_about(screen, zoom * factor, &zoom_policy);
        let after_doc = xf.to_doc(screen);
        prop_assert!((after_doc[0] - doc_expected[0]).abs() <= 1e-9 && (after_doc[1] - doc_expected[1]).abs() <= 1e-9);
    }
    #[test]
    fn vectorcraft_fit_zoom_identity(
        min_x in -500.0f32..500.0,
        min_y in -500.0f32..500.0,
        w in 61.0f32..2000.0,
        h in 61.0f32..2000.0,
        tx in -1000.0f64..1000.0,
        ty in -1000.0f64..1000.0,
        tw in 1.0f64..2000.0,
        th in 1.0f64..2000.0,
        rotation_deg in -180.0f64..180.0,
    ) {
        let rect = Rect::from_min_size(pos2(min_x, min_y), vec2(w, h));
        let target = [[tx, ty], [tx + tw, ty + th]];
        let vc_policy = ZoomPolicy { min: 0.0313, max: 640.0, steps: &[], anchor: Anchor::Pointer };
        let mut xf = ViewXform::new(rect, [0.0, 0.0], 1.0, 1.0);
        xf.rotation = rotation_deg.to_radians();
        xf.fit(target, 30.0, &vc_policy);
        let expected = vectorcraft_fit_zoom(rect, target, rotation_deg);
        prop_assert!((xf.zoom - expected).abs() <= 1e-12);
        prop_assert_eq!(xf.centre, [(target[0][0] + target[1][0]) / 2.0, (target[0][1] + target[1][1]) / 2.0]);
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
fn fit_degenerate_targets_and_fit_modes() {
    let mut xf = view(2.5, Rotation::quarter(0), 1.0);
    let policy = policy();
    xf.fit_with_mode([[10.0, 20.0], [10.0, 20.0]], 0.0, FitMode::Contain, &policy);
    assert_eq!(xf.centre, [10.0, 20.0]);
    assert_eq!(xf.zoom, 2.5);

    xf.fit_with_mode([[10.0, 20.0], [10.0, 120.0]], 0.0, FitMode::Contain, &policy);
    assert_eq!(xf.centre, [10.0, 70.0]);
    assert!((xf.zoom - 6.0).abs() <= 1e-12);

    xf.fit_with_mode([[10.0, 20.0], [210.0, 20.0]], 0.0, FitMode::Cover, &policy);
    assert_eq!(xf.centre, [110.0, 20.0]);
    assert!((xf.zoom - 4.0).abs() <= 1e-12);
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
