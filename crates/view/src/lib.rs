#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
//! Canvas transforms in logical screen points and document units.

use emath::{Pos2, Rect, TSTransform, Vec2, pos2, vec2};

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
    pub min: f32,
    pub max: f32,
    pub steps: &'static [f32],
    pub anchor: Anchor,
}

impl ZoomPolicy {
    pub fn clamp(&self, zoom: f32) -> f32 {
        zoom.max(self.min).min(self.max)
    }
    /// The next strictly larger step, or the upper bound.
    pub fn next_step_up(&self, zoom: f32) -> f32 {
        let zoom = self.clamp(zoom);
        self.steps.iter().copied().find(|step| *step > zoom).unwrap_or(self.max)
    }
    /// The next strictly smaller step, or the lower bound.
    pub fn next_step_down(&self, zoom: f32) -> f32 {
        let zoom = self.clamp(zoom);
        self.steps.iter().rev().copied().find(|step| *step < zoom).unwrap_or(self.min)
    }
}

/// Zoom is physical pixels per document unit; ppp converts to logical points.
/// Both must be finite and positive.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewXform {
    pub rect: Rect,
    pub centre: Pos2,
    pub zoom: f32,
    pub rotation: f32,
    pub ppp: f32,
}

impl ViewXform {
    /// Map a document vector to logical points, including rotation and scale.
    pub fn map_vec(&self, vector: Vec2) -> Vec2 {
        let (sin, cos) = self.rotation.sin_cos();
        vec2(cos * vector.x - sin * vector.y, sin * vector.x + cos * vector.y) * (self.zoom / self.ppp)
    }
    /// Map a logical screen vector to document units.
    pub fn unmap_vec(&self, vector: Vec2) -> Vec2 {
        let (sin, cos) = self.rotation.sin_cos();
        vec2(cos * vector.x + sin * vector.y, -sin * vector.x + cos * vector.y) * (self.ppp / self.zoom)
    }
    pub fn to_screen(&self, point: Pos2) -> Pos2 {
        self.rect.center() + self.map_vec(point - self.centre)
    }
    pub fn to_doc(&self, point: Pos2) -> Pos2 {
        self.centre + self.unmap_vec(point - self.rect.center())
    }
    fn bounds(rect: Rect, map: impl Fn(Pos2) -> Pos2) -> Rect {
        [rect.min, pos2(rect.max.x, rect.min.y), rect.max, pos2(rect.min.x, rect.max.y)].into_iter().fold(Rect::NOTHING, |mut bounds, corner| {
            bounds.extend_with(map(corner));
            bounds
        })
    }
    /// Axis-aligned screen bounding box of a document rectangle.
    pub fn doc_rect_to_screen(&self, rect: Rect) -> Rect {
        Self::bounds(rect, |point| self.to_screen(point))
    }
    /// Axis-aligned document bounding box of the viewport.
    pub fn visible_doc_rect(&self) -> Rect {
        Self::bounds(self.rect, |point| self.to_doc(point))
    }
    /// Return an equivalent translation/scale transform only when rotation is zero.
    pub fn as_tstransform(&self) -> Option<TSTransform> {
        (self.rotation == 0.0).then(|| {
            let scale = self.zoom / self.ppp;
            TSTransform::new(self.rect.center().to_vec2() - self.centre.to_vec2() * scale, scale)
        })
    }
    /// Zoom around the policy-selected screen point; `anchor_screen` is the pointer.
    pub fn zoom_about(&mut self, anchor_screen: Pos2, new_zoom: f32, policy: &ZoomPolicy) {
        let anchor = match policy.anchor {
            Anchor::Pointer => anchor_screen,
            Anchor::Centre => self.rect.center(),
            Anchor::Point(point) => point,
        };
        let before = self.to_doc(anchor);
        self.zoom = policy.clamp(new_zoom);
        self.centre += before - self.to_doc(anchor);
    }
    /// Move the clicked document point to the viewport centre, then zoom there.
    pub fn zoom_about_and_centre(&mut self, clicked_screen: Pos2, new_zoom: f32, policy: &ZoomPolicy) {
        self.centre = self.to_doc(clicked_screen);
        self.set_zoom_keep_centre(new_zoom, policy);
    }
    /// Rotate around a logical screen point, keeping its document point fixed.
    pub fn rotate_about(&mut self, anchor_screen: Pos2, delta: f32) {
        let before = self.to_doc(anchor_screen);
        self.rotation += delta;
        self.centre += before - self.to_doc(anchor_screen);
    }
    /// Move displayed content by physical pixels along screen axes.
    pub fn pan_pixels(&mut self, pixels: Vec2) {
        self.centre -= self.unmap_vec(pixels / self.ppp);
    }
    /// Centre and fit a document rectangle with logical-point padding on each side.
    /// Policy bounds can prevent the rectangle from fitting.
    pub fn fit(&mut self, doc_rect: Rect, padding: f32, policy: &ZoomPolicy) {
        let (sin, cos) = self.rotation.sin_cos();
        let size = doc_rect.size();
        let rotated = vec2(cos.abs() * size.x + sin.abs() * size.y, sin.abs() * size.x + cos.abs() * size.y);
        let available = (self.rect.size() - Vec2::splat(2.0 * padding.max(0.0))).max(Vec2::ZERO);
        let zoom = (available.x / rotated.x).min(available.y / rotated.y) * self.ppp;
        self.centre = doc_rect.center();
        self.set_zoom_keep_centre(zoom, policy);
    }
    /// Change zoom without moving the document point at the viewport centre.
    pub fn set_zoom_keep_centre(&mut self, zoom: f32, policy: &ZoomPolicy) {
        self.zoom = policy.clamp(zoom);
    }
}
