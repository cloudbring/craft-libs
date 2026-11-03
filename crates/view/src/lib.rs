#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
//! Canvas transforms in logical screen points and f64 document units.

use emath::{Pos2, Rect, TSTransform, Vec2, pos2, vec2};

/// A point or vector in document units.
pub type DocPoint = [f64; 2];
/// Document-space minimum and maximum corners.
pub type DocRect = [DocPoint; 2];

/// Convert a tuple to a document point.
pub fn doc_point((x, y): (f64, f64)) -> DocPoint {
    [x, y]
}

/// Convert a document point to a tuple.
pub fn doc_tuple([x, y]: DocPoint) -> (f64, f64) {
    (x, y)
}

/// The document point kept fixed during a zoom. `Point` is in screen points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Anchor {
    Pointer,
    Centre,
    Point(Pos2),
}

/// Finite, positive ordered bounds and ascending finite steps within those bounds.
#[derive(Clone, Debug)]
pub struct ZoomPolicy {
    pub min: f64,
    pub max: f64,
    pub steps: &'static [f64],
    pub anchor: Anchor,
}

impl ZoomPolicy {
    pub fn clamp(&self, zoom: f64) -> f64 {
        zoom.max(self.min).min(self.max)
    }
    /// The next strictly larger step, or the upper bound.
    pub fn next_step_up(&self, zoom: f64) -> f64 {
        let zoom = self.clamp(zoom);
        self.steps.iter().copied().find(|step| *step > zoom).unwrap_or(self.max)
    }
    /// The next strictly smaller step, or the lower bound.
    pub fn next_step_down(&self, zoom: f64) -> f64 {
        let zoom = self.clamp(zoom);
        self.steps.iter().rev().copied().find(|step| *step < zoom).unwrap_or(self.min)
    }
}

/// Zoom is physical pixels per document unit; ppp converts to logical points.
/// Zoom, ppp, and aspect must be finite and positive; rotation is in radians.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewXform {
    pub rect: Rect,
    pub centre: DocPoint,
    pub zoom: f64,
    pub rotation: f64,
    pub ppp: f32,
    pub aspect: f64,
}

impl ViewXform {
    /// Construct an unrotated view with square document pixels.
    pub fn new(rect: Rect, centre: DocPoint, zoom: f64, ppp: f32) -> Self {
        Self { rect, centre, zoom, rotation: 0.0, ppp, aspect: 1.0 }
    }
    pub fn rotation_degrees(&self) -> f64 {
        self.rotation.to_degrees()
    }
    /// Set the rotation in degrees, normalized to [0, 360).
    pub fn set_rotation_degrees(&mut self, degrees: f64) {
        self.rotation = degrees.rem_euclid(360.0).to_radians();
    }
    /// Map a document vector to logical screen points.
    pub fn map_vec(&self, vector: DocPoint) -> Vec2 {
        let (sin, cos) = self.rotation.sin_cos();
        let scale = self.zoom / f64::from(self.ppp);
        vec2(((cos * vector[0] * self.aspect - sin * vector[1]) * scale) as f32, ((sin * vector[0] * self.aspect + cos * vector[1]) * scale) as f32)
    }
    /// Map a logical screen vector to document units.
    pub fn unmap_vec(&self, vector: Vec2) -> DocPoint {
        let (sin, cos) = self.rotation.sin_cos();
        let (x, y) = (f64::from(vector.x), f64::from(vector.y));
        let scale = f64::from(self.ppp) / self.zoom;
        [(cos * x + sin * y) * scale / self.aspect, (-sin * x + cos * y) * scale]
    }
    pub fn to_screen(&self, point: DocPoint) -> Pos2 {
        self.rect.center() + self.map_vec([point[0] - self.centre[0], point[1] - self.centre[1]])
    }
    pub fn to_doc(&self, point: Pos2) -> DocPoint {
        let offset = self.unmap_vec(point - self.rect.center());
        [self.centre[0] + offset[0], self.centre[1] + offset[1]]
    }
    /// Axis-aligned screen bounding box of a document rectangle.
    pub fn doc_rect_to_screen(&self, [min, max]: DocRect) -> Rect {
        [min, [max[0], min[1]], max, [min[0], max[1]]].into_iter().fold(Rect::NOTHING, |mut bounds, corner| {
            bounds.extend_with(self.to_screen(corner));
            bounds
        })
    }
    /// Axis-aligned document bounding box of the viewport.
    pub fn visible_doc_rect(&self) -> DocRect {
        let rect = self.rect;
        [rect.min, pos2(rect.max.x, rect.min.y), rect.max, pos2(rect.min.x, rect.max.y)]
            .into_iter()
            .map(|point| self.to_doc(point))
            .fold([[f64::INFINITY; 2], [f64::NEG_INFINITY; 2]], |[min, max], point| {
                [[min[0].min(point[0]), min[1].min(point[1])], [max[0].max(point[0]), max[1].max(point[1])]]
            })
    }
    /// Return an equivalent translation/scale transform for square pixels and zero rotation.
    pub fn as_tstransform(&self) -> Option<TSTransform> {
        (self.rotation == 0.0 && self.aspect == 1.0).then(|| {
            let scale = (self.zoom / f64::from(self.ppp)) as f32;
            TSTransform::new(self.rect.center().to_vec2() - vec2(self.centre[0] as f32, self.centre[1] as f32) * scale, scale)
        })
    }
    /// Zoom around the policy-selected screen point; `anchor_screen` is the pointer.
    pub fn zoom_about(&mut self, anchor_screen: Pos2, new_zoom: f64, policy: &ZoomPolicy) {
        let anchor = match policy.anchor {
            Anchor::Pointer => anchor_screen,
            Anchor::Centre => self.rect.center(),
            Anchor::Point(point) => point,
        };
        let before = self.to_doc(anchor);
        self.zoom = policy.clamp(new_zoom);
        self.adjust_centre(before, anchor);
    }
    fn adjust_centre(&mut self, before: DocPoint, anchor: Pos2) {
        let after = self.to_doc(anchor);
        self.centre[0] += before[0] - after[0];
        self.centre[1] += before[1] - after[1];
    }
    /// Move the clicked document point to the viewport centre, then zoom there.
    pub fn zoom_about_and_centre(&mut self, clicked_screen: Pos2, new_zoom: f64, policy: &ZoomPolicy) {
        self.centre = self.to_doc(clicked_screen);
        self.set_zoom_keep_centre(new_zoom, policy);
    }
    /// Rotate around a logical screen point, keeping its document point fixed.
    pub fn rotate_about(&mut self, anchor_screen: Pos2, delta: f64) {
        let before = self.to_doc(anchor_screen);
        self.rotation += delta;
        self.adjust_centre(before, anchor_screen);
    }
    /// Move displayed content by device pixels along screen axes at the supplied ppp.
    pub fn pan_pixels(&mut self, ppp: f32, (dx, dy): (f64, f64)) {
        let (sin, cos) = self.rotation.sin_cos();
        let scale = f64::from(self.ppp) / (f64::from(ppp) * self.zoom);
        self.centre[0] -= (cos * dx + sin * dy) * scale / self.aspect;
        self.centre[1] -= (-sin * dx + cos * dy) * scale;
    }
    /// Centre and fit the whole document, with logical-point padding on each side.
    /// Policy bounds may prevent the requested fit.
    pub fn fit(&mut self, [min, max]: DocRect, padding: f32, policy: &ZoomPolicy) {
        let (sin, cos) = self.rotation.sin_cos();
        let (width, height) = ((max[0] - min[0]) * self.aspect, max[1] - min[1]);
        let available = (self.rect.size() - Vec2::splat(2.0 * padding.max(0.0))).max(Vec2::ZERO);
        let zoom = (f64::from(available.x) / (cos.abs() * width + sin.abs() * height))
            .min(f64::from(available.y) / (sin.abs() * width + cos.abs() * height))
            * f64::from(self.ppp);
        self.centre = [(min[0] + max[0]) / 2.0, (min[1] + max[1]) / 2.0];
        self.set_zoom_keep_centre(zoom, policy);
    }
    /// Centre the document and cover the viewport, even when rotated.
    /// Policy bounds may prevent the requested fill.
    pub fn fill(&mut self, [min, max]: DocRect, policy: &ZoomPolicy) {
        let (sin, cos) = self.rotation.sin_cos();
        let (width, height) = ((max[0] - min[0]) * self.aspect, max[1] - min[1]);
        let (screen_width, screen_height) = (f64::from(self.rect.width()), f64::from(self.rect.height()));
        let zoom = ((cos.abs() * screen_width + sin.abs() * screen_height) / width)
            .max((sin.abs() * screen_width + cos.abs() * screen_height) / height)
            * f64::from(self.ppp);
        self.centre = [(min[0] + max[0]) / 2.0, (min[1] + max[1]) / 2.0];
        self.set_zoom_keep_centre(zoom, policy);
    }
    /// Change zoom without moving the document point at the viewport centre.
    pub fn set_zoom_keep_centre(&mut self, zoom: f64, policy: &ZoomPolicy) {
        self.zoom = policy.clamp(zoom);
    }
}
