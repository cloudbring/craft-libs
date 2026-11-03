//! First-party provenance: DeckCraft `crates/metafile/src/place.rs` (reader, originally from WordCraft).
//! Placing a picture on a target (a page or a device): the fitted transform, paths as kurbo paths in
//! target units and bitmaps as target rectangles. The rasteriser and the PDF writer only paint the result.

use std::sync::Arc;

use kurbo::{Affine, BezPath, Rect, Shape};

use crate::{HAlign, Item, Picture, Seg, Text, VAlign};

/// Largest bitmap side the renderers draw: within the PDF image limit and the rasteriser's 16-bit sides.
pub const MAX_BITMAP_SIDE: u32 = 30_000;
/// Placed coordinates past this (target units) are far off any page; such items are left out.
const MAX_COORD: f64 = 1e6;
/// Smallest placed bitmap side (target units), so the stretch onto it is never singular.
const MIN_BITMAP_SIDE: f64 = 1e-6;
/// Maximum aggregate path elements retained in placed output.
const MAX_PLACED_ELEMENTS: usize = 500_000;

/// The least of a source axis a crop keeps (0.5%). Files crop pictures to slivers (a header bar
/// that is 2.7% of a page-sized metafile, #1899). Layout makes a picture's frame at most 4000 pt
/// a side, so the whole source then spans at most 800,000 pt: what it covers stays inside
/// `MAX_COORD` and is never left out as far off the page.
const MIN_VISIBLE: f64 = 0.005;

/// The part of a picture's source that `crop` (left, top, right, bottom fractions of the source)
/// keeps, as fractions of the source: [left, top, width, height], bitmap or metafile alike. Each
/// side is taken as 0–1 (junk as 0). When the two sides of an axis leave less than 0.5% of it,
/// the left (top) side stands and 0.5% shows.
pub fn crop_window(crop: [f32; 4]) -> [f64; 4] {
    let [l, t, r, b] = crop.map(|v| if v.is_finite() { f64::from(v.clamp(0.0, 1.0)) } else { 0.0 });
    let axis = |lo: f64, hi: f64| {
        let lo = lo.min(1.0 - MIN_VISIBLE);
        (lo, (1.0 - lo - hi).max(MIN_VISIBLE))
    };
    let ((x, w), (y, h)) = (axis(l, r), axis(t, b));
    [x, y, w, h]
}

/// A text run laid out with a font's metrics: the em size (in the run's own units, see
/// [`PlacedItem::Text`]), the baseline's y and each glyph's (x, y), alignment included.
pub struct TextLayout {
    pub em: f64,
    pub baseline: f64,
    pub glyphs: Vec<(f64, f64)>,
}

impl Text {
    /// Lays out shaped glyphs, given as (cluster byte offset, x advance, x offset, y offset) in font
    /// units of a font with `upem` units per em, `ascent` above and `descent` below the baseline.
    /// The per-character advances `dx` place each cluster when there is one per character.
    pub fn layout(&self, upem: f64, ascent: f64, descent: f64, glyphs: &[(usize, f64, f64, f64)]) -> Option<TextLayout> {
        let upem = if upem.is_finite() && upem > 0.0 { upem } else { 1000.0 };
        let cell = ascent + descent;
        let size = f64::from(self.font.height.abs());
        let em = if self.font.height > 0.0 && cell.is_finite() && cell > 0.0 { size * upem / cell } else { size };
        if !em.is_finite() || em <= 0.0 {
            return None;
        }
        let k = em / upem;
        let starts: Vec<usize> = self.text.char_indices().map(|(i, _)| i).collect();
        let dx = (self.dx.len() == starts.len() && !starts.is_empty()).then(|| {
            let mut cum = Vec::with_capacity(self.dx.len() + 1);
            let mut x = 0.0;
            cum.push(x);
            for d in &self.dx {
                x += f64::from(*d);
                cum.push(x);
            }
            cum
        });
        let mut out = Vec::with_capacity(glyphs.len());
        let (mut pen, mut cluster_pen, mut last) = (0.0, 0.0, None);
        for &(cluster, adv, xo, yo) in glyphs {
            let ci = starts.partition_point(|&b| b <= cluster).saturating_sub(1);
            if last != Some(ci) {
                cluster_pen = pen;
                last = Some(ci);
            }
            let x = match &dx {
                Some(cum) => cum.get(ci).copied().unwrap_or(0.0) + pen - cluster_pen,
                None => pen,
            };
            out.push((x + xo * k, -yo * k));
            pen += adv * k;
        }
        let width = dx.as_ref().and_then(|c| c.last().copied()).unwrap_or(pen);
        let hoff = match self.h_align {
            HAlign::Left => 0.0,
            HAlign::Center => -width / 2.0,
            HAlign::Right => -width,
        };
        let baseline = match self.v_align {
            VAlign::Top => ascent * k,
            VAlign::Baseline => 0.0,
            VAlign::Bottom => -descent * k,
        };
        let glyphs = out.into_iter().map(|(x, y)| (x + hoff, y + baseline)).collect();
        Some(TextLayout { em, baseline, glyphs })
    }
}

/// One item of a placed picture, in target units.
pub enum PlacedItem {
    /// A filled and/or stroked path. The stroke width is in target units; 0 is a cosmetic hairline that
    /// the renderer draws at its minimum width.
    Path { path: BezPath, fill: Option<[u8; 4]>, stroke: Option<([u8; 4], f64)>, even_odd: bool, region: Option<Arc<BezPath>> },
    /// `Picture::items[index]`, a bitmap, stretched into `rect`.
    Bitmap { index: usize, rect: Rect, region: Option<Arc<BezPath>> },
    /// `Picture::items[index]`, a pattern fill: `path` (target units) filled with the item's tile
    /// repeated; `tile` maps tile pixels to target units.
    Pattern { index: usize, path: BezPath, even_odd: bool, tile: Affine, region: Option<Arc<BezPath>> },
    /// `Picture::items[index]`, a text string: `transform` maps its text space to target units;
    /// `clip` (target units) bounds what it draws. `region` is the device-context clip.
    Text { index: usize, transform: Affine, clip: Option<Rect>, region: Option<Arc<BezPath>> },
}

impl Picture {
    /// Whether the picture has a finite, positive size, so it can be fitted into a frame.
    pub fn has_size(&self) -> bool {
        self.width.is_finite() && self.height.is_finite() && self.width > 0.0 && self.height > 0.0
    }

    /// The transform that fits the picture into `r` with the crop fractions `crop` (left, top, right,
    /// bottom) removed first: the visible part fills `r`. None for a degenerate size.
    pub fn fit(&self, r: Rect, crop: [f32; 4]) -> Option<Affine> {
        let [cl, ct, vw, vh] = crop_window(crop);
        let (w, h) = (f64::from(self.width), f64::from(self.height));
        let sx = r.width() / (w * vw);
        let sy = r.height() / (h * vh);
        (sx.is_finite() && sy.is_finite() && sx > 0.0 && sy > 0.0)
            .then(|| Affine::translate((r.x0 - cl * w * sx, r.y0 - ct * h * sy)) * Affine::scale_non_uniform(sx, sy))
    }

    /// The items placed into `r` with `crop` applied. Items that are not finite or reach past ±1e6
    /// target units are left out (they are far off any page), as are bitmaps with no area. Stroke
    /// widths are scaled to target units (along the mean axis scale, so a non-uniform fit keeps
    /// strokes even).
    pub fn place(&self, r: Rect, crop: [f32; 4]) -> Vec<PlacedItem> {
        let Some(tf) = self.fit(r, crop) else { return Vec::new() };
        let c = tf.as_coeffs();
        let k = (c[0].abs() + c[3].abs()) / 2.0;
        let mut out = Vec::with_capacity(self.items.len());
        let mut total_elements = 0usize;
        type CachedRegion = (*const [f32; 4], usize, Option<Option<Arc<BezPath>>>);
        let mut cached_region: Option<CachedRegion> = None;
        let mut get_region = |reg: &Option<Vec<[f32; 4]>>| -> Option<Option<Arc<BezPath>>> {
            let (ptr, len) = match reg {
                None => (std::ptr::null(), 0),
                Some(v) => (v.as_ptr(), v.len()),
            };
            if let Some((c_ptr, c_len, ref res)) = cached_region
                && c_ptr == ptr
                && c_len == len
            {
                return res.clone();
            }
            let res = placed_region(&tf, reg);
            cached_region = Some((ptr, len, res.clone()));
            res
        };

        for (index, item) in self.items.iter().enumerate() {
            if total_elements >= MAX_PLACED_ELEMENTS {
                break;
            }
            match item {
                Item::Path { segs, fill, stroke, even_odd, region } => {
                    let path = tf * bez(segs);
                    let Some(region) = get_region(region) else { continue };
                    if in_range(path.bounding_box()) {
                        total_elements = total_elements.saturating_add(path.elements().len());
                        let stroke = stroke.map(|(c, w)| (c, f64::from(w) * k));
                        out.push(PlacedItem::Path { path, fill: *fill, stroke, even_odd: *even_odd, region });
                    }
                }
                Item::Pattern { segs, even_odd, tile: t, region } => {
                    let path = tf * bez(segs);
                    let Some(region) = get_region(region) else { continue };
                    let tile = tf
                        * Affine::translate((f64::from(t.origin[0]), f64::from(t.origin[1])))
                        * Affine::scale_non_uniform(f64::from(t.cell[0]), f64::from(t.cell[1]));
                    let det = tile.determinant();
                    if in_range(path.bounding_box()) && det.is_finite() && det.abs() > 1e-12 {
                        total_elements = total_elements.saturating_add(path.elements().len());
                        out.push(PlacedItem::Pattern { index, path, even_odd: *even_odd, tile, region });
                    }
                }
                Item::Text(t) => {
                    let transform = tf * Affine::new(t.transform);
                    let Some(region) = get_region(&t.region) else { continue };
                    let clip =
                        t.clip.map(|[x0, y0, x1, y1]| tf.transform_rect_bbox(Rect::new(f64::from(x0), f64::from(y0), f64::from(x1), f64::from(y1))));
                    let c = transform.as_coeffs();
                    let origin = Rect::new(c[4], c[5], c[4], c[5]);
                    if in_range(origin) && c.iter().all(|v| v.is_finite() && v.abs() <= MAX_COORD) && clip.is_none_or(in_range) {
                        out.push(PlacedItem::Text { index, transform, clip, region });
                    }
                }
                Item::Bitmap { rect: [x, y, w, h], region, .. } => {
                    let (x0, y0) = (f64::from(*x), f64::from(*y));
                    let rect = tf.transform_rect_bbox(Rect::new(x0, y0, x0 + f64::from(*w), y0 + f64::from(*h)));
                    let Some(region) = get_region(region) else { continue };
                    if in_range(rect) && rect.width() > MIN_BITMAP_SIDE && rect.height() > MIN_BITMAP_SIDE {
                        out.push(PlacedItem::Bitmap { index, rect, region });
                    }
                }
            }
        }
        out
    }
}

impl Item {
    /// Takes a bitmap's pixels out of the item (its geometry stays) and returns them as (width, height,
    /// RGBA) when a renderer can draw them: both sides in 1..=[`MAX_BITMAP_SIDE`] and exactly that many
    /// pixels. None for paths and for bitmaps that cannot be drawn.
    pub fn take_pixels(&mut self) -> Option<(u32, u32, Vec<u8>)> {
        let Item::Bitmap { width, height, rgba, .. } = self else { return None };
        let (w, h, px) = (*width, *height, std::mem::take(rgba));
        let sides = (1..=MAX_BITMAP_SIDE).contains(&w) && (1..=MAX_BITMAP_SIDE).contains(&h);
        (sides && px.len() == (w as usize).saturating_mul(h as usize).saturating_mul(4)).then_some((w, h, px))
    }
}

/// The device-context clip in target units. `Some(None)` is the default clip. `None` means the
/// region does not meet the page, so the item draws nothing.
fn placed_region(tf: &Affine, region: &Option<Vec<[f32; 4]>>) -> Option<Option<Arc<BezPath>>> {
    let Some(rects) = region else { return Some(None) };
    let mut path = BezPath::new();
    for [x0, y0, x1, y1] in rects {
        let r = tf.transform_rect_bbox(Rect::new(f64::from(*x0), f64::from(*y0), f64::from(*x1), f64::from(*y1)));
        if in_range(r) && r.width() > 1e-9 && r.height() > 1e-9 {
            path.extend(r.to_path(0.1));
        }
    }
    (!path.is_empty()).then_some(Some(Arc::new(path)))
}

/// Segments as a kurbo path. A segment after a close opens a new subpath at the last start point.
fn bez(segs: &[Seg]) -> BezPath {
    let mut p = BezPath::new();
    let mut start = (0.0, 0.0);
    let mut open = false;
    for s in segs {
        match *s {
            Seg::Move(x, y) => {
                start = (f64::from(x), f64::from(y));
                p.move_to(start);
                open = true;
            }
            Seg::Line(x, y) => {
                if !open {
                    p.move_to(start);
                }
                p.line_to((f64::from(x), f64::from(y)));
                open = true;
            }
            Seg::Cubic(a, b, c, d, e, f) => {
                if !open {
                    p.move_to(start);
                }
                p.curve_to((f64::from(a), f64::from(b)), (f64::from(c), f64::from(d)), (f64::from(e), f64::from(f)));
                open = true;
            }
            Seg::Close => {
                if open {
                    p.close_path();
                }
                open = false;
            }
        }
    }
    p
}

/// Whether a placed box is finite and within ±[`MAX_COORD`].
fn in_range(b: Rect) -> bool {
    [b.x0, b.y0, b.x1, b.y1].iter().all(|v| v.is_finite() && v.abs() < MAX_COORD)
}
