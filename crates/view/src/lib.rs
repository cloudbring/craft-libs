#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
//! Canvas transforms in logical screen points and `f64` document units.
//!
//! All transform arithmetic stays in `f64` until the final cast to [`Pos2`]:
//! 1. [`ViewXform::rect`] is converted once to `f64` corners `min` and `max`, with viewport
//!    centre `c = 0.5 * (min + max)` and unrotated top-left document point [`ViewXform::origin`].
//! 2. [`ViewXform::to_screen`] computes unrotated screen coordinates
//!    `u = min + (p - origin) * aspect * zoom / ppp`, rotates `u` clockwise about `c` by
//!    [`ViewXform::rotation`] (using exact `±1`/`0` swaps when `rotation` is a multiple of
//!    `π / 2`), and casts to [`Pos2`].
//! 3. [`ViewXform::to_doc`] widens the screen point to `f64`, unrotates counter-clockwise about
//!    `c` in the same operation order, and computes `origin + (u - min) * ppp / (aspect * zoom)`.

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

/// How a document rectangle is fitted into the viewport.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FitMode {
    /// Fit the entire rotated document rectangle inside the (padded) viewport.
    Contain,
    /// Cover the entire viewport with the document rectangle, even when rotated.
    Cover,
}

/// Exact rotation constructors for [`ViewXform::rotation`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rotation;

impl Rotation {
    /// Clockwise rotation in radians for `n` quarter turns (`(n % 4) * π / 2`).
    ///
    /// Transforms at quarter-turn multiples use exact `±1`/`0` swaps rather than `sin_cos`,
    /// matching quarter-turn canvas helpers bit-for-bit.
    pub fn quarter(n: u8) -> f64 {
        f64::from(n % 4) * std::f64::consts::FRAC_PI_2
    }
}

fn quarter_turn(rotation: f64) -> Option<u8> {
    if !rotation.is_finite() {
        return None;
    }
    let quarters = (rotation / std::f64::consts::FRAC_PI_2).round();
    if (rotation - quarters * std::f64::consts::FRAC_PI_2).abs() <= 1e-15 { Some((quarters as i64).rem_euclid(4) as u8) } else { None }
}

fn quarter_rotate([px, py]: [f64; 2], [cx, cy]: [f64; 2], q: u8) -> [f64; 2] {
    if q.is_multiple_of(4) {
        return [px, py];
    }
    let (dx, dy) = (px - cx, py - cy);
    let (x, y) = match q % 4 {
        0 => (dx, dy),
        1 => (-dy, dx),
        2 => (-dx, -dy),
        _ => (dy, -dx),
    };
    [cx + x, cy + y]
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

/// Zoom is physical pixels per document unit; `ppp` converts to logical points.
/// Zoom, `ppp`, and `aspect` must be finite and positive; `rotation` is in radians.
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
    /// Construct a view from its unrotated top-left document point (`origin` at `rect.min`).
    ///
    /// `rect` is converted once to `f64` corners and `centre` is computed as
    /// `origin + 0.5 * (max - min) * ppp / (zoom * aspect)`.
    pub fn from_origin(rect: Rect, origin: DocPoint, zoom: f64, rotation: f64, ppp: f32, aspect: f64) -> Self {
        let (min, max) = Self::rect_corners_f64(rect);
        let half_w = 0.5 * (max[0] - min[0]);
        let half_h = 0.5 * (max[1] - min[1]);
        let scale = zoom / f64::from(ppp);
        let centre = [origin[0] + half_w / (scale * aspect), origin[1] + half_h / scale];
        Self { rect, centre, zoom, rotation, ppp, aspect }
    }
    /// The document point at `rect.min` in the unrotated view:
    /// `centre - 0.5 * (max - min) * ppp / (zoom * aspect)`.
    pub fn origin(&self) -> DocPoint {
        let (min, max) = Self::rect_corners_f64(self.rect);
        let half_w = 0.5 * (max[0] - min[0]);
        let half_h = 0.5 * (max[1] - min[1]);
        let scale = self.zoom / f64::from(self.ppp);
        [self.centre[0] - half_w / (scale * self.aspect), self.centre[1] - half_h / scale]
    }
    fn rect_corners_f64(rect: Rect) -> ([f64; 2], [f64; 2]) {
        ([f64::from(rect.min.x), f64::from(rect.min.y)], [f64::from(rect.max.x), f64::from(rect.max.y)])
    }
    fn sin_cos(&self) -> (f64, f64) {
        if let Some(q) = quarter_turn(self.rotation) {
            match q {
                0 => (0.0, 1.0),
                1 => (1.0, 0.0),
                2 => (0.0, -1.0),
                _ => (-1.0, 0.0),
            }
        } else {
            self.rotation.sin_cos()
        }
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
        let (sin, cos) = self.sin_cos();
        let scale = self.zoom / f64::from(self.ppp);
        vec2(((cos * vector[0] * self.aspect - sin * vector[1]) * scale) as f32, ((sin * vector[0] * self.aspect + cos * vector[1]) * scale) as f32)
    }
    /// Map a logical screen vector to document units.
    pub fn unmap_vec(&self, vector: Vec2) -> DocPoint {
        let (sin, cos) = self.sin_cos();
        let (x, y) = (f64::from(vector.x), f64::from(vector.y));
        let scale = f64::from(self.ppp) / self.zoom;
        [(cos * x + sin * y) * scale / self.aspect, (-sin * x + cos * y) * scale]
    }
    /// Map a document point to logical screen coordinates in `f64`.
    ///
    /// Operation order:
    /// 1. Convert `rect` once to `f64` corners `min` and `max`, with `centre = 0.5 * (min + max)`
    ///    and `origin = self.origin()`.
    /// 2. Compute `u = min + (p - origin) * aspect * zoom / ppp`.
    /// 3. Rotate `u` clockwise about `centre` by `rotation` (using exact `±1`/`0` swaps when
    ///    `rotation` is a multiple of `π / 2`).
    pub fn to_screen_f64(&self, point: DocPoint) -> [f64; 2] {
        let (min, max) = Self::rect_corners_f64(self.rect);
        let centre = [0.5 * (min[0] + max[0]), 0.5 * (min[1] + max[1])];
        let origin = self.origin();
        let scale = self.zoom / f64::from(self.ppp);
        let unrotated = [min[0] + (point[0] - origin[0]) * self.aspect * scale, min[1] + (point[1] - origin[1]) * scale];
        if let Some(q) = quarter_turn(self.rotation) {
            quarter_rotate(unrotated, centre, q)
        } else {
            let (sin, cos) = self.rotation.sin_cos();
            let (dx, dy) = (unrotated[0] - centre[0], unrotated[1] - centre[1]);
            [centre[0] + (cos * dx - sin * dy), centre[1] + (sin * dx + cos * dy)]
        }
    }
    /// Map logical screen coordinates in `f64` to document units in the exact inverse operation
    /// order of [`Self::to_screen_f64`]:
    /// 1. Convert `rect` once to `f64` corners `min` and `max`, with `centre = 0.5 * (min + max)`.
    /// 2. Unrotate `point` counter-clockwise about `centre` by `rotation` (using exact `±1`/`0`
    ///    swaps when `rotation` is a multiple of `π / 2`) to obtain `u`.
    /// 3. Compute `self.origin() + (u - min) * ppp / (aspect * zoom)`.
    pub fn to_doc_f64(&self, point: [f64; 2]) -> DocPoint {
        let (min, max) = Self::rect_corners_f64(self.rect);
        let centre = [0.5 * (min[0] + max[0]), 0.5 * (min[1] + max[1])];
        let unrotated = if let Some(q) = quarter_turn(self.rotation) {
            quarter_rotate(point, centre, (4 - q) % 4)
        } else {
            let (sin, cos) = self.rotation.sin_cos();
            let (dx, dy) = (point[0] - centre[0], point[1] - centre[1]);
            [centre[0] + (cos * dx + sin * dy), centre[1] + (-sin * dx + cos * dy)]
        };
        let origin = self.origin();
        let scale = self.zoom / f64::from(self.ppp);
        [origin[0] + (unrotated[0] - min[0]) / (self.aspect * scale), origin[1] + (unrotated[1] - min[1]) / scale]
    }
    /// Map a document point to logical screen coordinates, keeping all arithmetic in `f64` until
    /// the final cast to [`Pos2`]: `rect.min + (p - origin) * zoom` rotated about the `f64`
    /// viewport centre.
    pub fn to_screen(&self, point: DocPoint) -> Pos2 {
        let [x, y] = self.to_screen_f64(point);
        pos2(x as f32, y as f32)
    }
    /// Map a logical screen point to document units as the exact inverse of [`Self::to_screen`].
    pub fn to_doc(&self, point: Pos2) -> DocPoint {
        self.to_doc_f64([f64::from(point.x), f64::from(point.y)])
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
            Anchor::Centre => {
                self.set_zoom_keep_centre(new_zoom, policy);
                return;
            }
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
        let (sin, cos) = self.sin_cos();
        let scale = f64::from(self.ppp) / (f64::from(ppp) * self.zoom);
        self.centre[0] -= (cos * dx + sin * dy) * scale / self.aspect;
        self.centre[1] -= (-sin * dx + cos * dy) * scale;
    }
    /// Centre the document rectangle and fit or cover the viewport according to `mode`.
    ///
    /// For [`FitMode::Contain`], the rotated bounding box of the target at zoom 1 is
    /// `(|w cos| + |h sin|, |w sin| + |h cos|)` and `zoom = min(rect.w / bw, rect.h / bh)` after
    /// subtracting `2 * padding` from each viewport dimension. Degenerate targets with one zero
    /// dimension use the other dimension; if both dimensions are zero (or non-positive), `zoom`
    /// is left unchanged and only `centre` is updated.
    pub fn fit_with_mode(&mut self, [min, max]: DocRect, padding: f32, mode: FitMode, policy: &ZoomPolicy) {
        self.centre = [(min[0] + max[0]) / 2.0, (min[1] + max[1]) / 2.0];
        let raw_w = (max[0] - min[0]) * self.aspect;
        let raw_h = max[1] - min[1];
        if raw_w <= 0.0 && raw_h <= 0.0 {
            return;
        }
        let (sin, cos) = self.sin_cos();
        let (sin_abs, cos_abs) = (sin.abs(), cos.abs());
        let pad = 2.0 * f64::from(padding.max(0.0));
        let avail_w = (f64::from(self.rect.width()) - pad).max(0.0);
        let avail_h = (f64::from(self.rect.height()) - pad).max(0.0);
        let zoom = match mode {
            FitMode::Contain => {
                let w = raw_w.max(0.0);
                let h = raw_h.max(0.0);
                let bw = w * cos_abs + h * sin_abs;
                let bh = w * sin_abs + h * cos_abs;
                match (bw > 0.0, bh > 0.0) {
                    (true, true) => (avail_w / bw).min(avail_h / bh),
                    (true, false) => avail_w / bw,
                    (false, true) => avail_h / bh,
                    (false, false) => return,
                }
            }
            FitMode::Cover => {
                let req_w = cos_abs * avail_w + sin_abs * avail_h;
                let req_h = sin_abs * avail_w + cos_abs * avail_h;
                match (raw_w > 0.0, raw_h > 0.0) {
                    (true, true) => (req_w / raw_w).max(req_h / raw_h),
                    (true, false) => req_w / raw_w,
                    (false, true) => req_h / raw_h,
                    (false, false) => return,
                }
            }
        } * f64::from(self.ppp);
        self.set_zoom_keep_centre(zoom, policy);
    }
    /// Centre and fit the whole document, with logical-point padding on each side.
    /// Policy bounds may prevent the requested fit.
    pub fn fit(&mut self, doc_rect: DocRect, padding: f32, policy: &ZoomPolicy) {
        self.fit_with_mode(doc_rect, padding, FitMode::Contain, policy);
    }
    /// Centre the document and cover the viewport, even when rotated.
    /// Policy bounds may prevent the requested fill.
    pub fn fill(&mut self, doc_rect: DocRect, policy: &ZoomPolicy) {
        self.fit_with_mode(doc_rect, 0.0, FitMode::Cover, policy);
    }
    /// Change zoom without moving the document point at the viewport centre.
    pub fn set_zoom_keep_centre(&mut self, zoom: f64, policy: &ZoomPolicy) {
        self.zoom = policy.clamp(zoom);
    }
}
