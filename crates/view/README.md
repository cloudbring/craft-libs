# craft-view

Shared canvas transforms, anchored zoom, rotation, panning, fit and fill for egui applications.
Document points and vectors use `DocPoint = [f64; 2]`; document rectangles use
`DocRect = [min, max]`. Convert application point types by copying their coordinates;
`doc_point` and `doc_tuple` convert to and from `(f64, f64)` (the array alias cannot
implement additional `From` traits). Screen positions, vectors and rectangles use emath's
`Pos2`, `Vec2` and `Rect` in logical points, so screen conversion rounds to f32 and cannot
guarantee an arbitrary document-space round trip to 1e-6.

`ViewXform::new` defaults to zero rotation and square pixels (`aspect = 1.0`). Zoom is
physical pixels per document unit, converted using pixels per point (`ppp`); `aspect`
scales document x before rotation. Zoom, ppp and aspect must be finite and positive.
Rotation is in radians; degree helpers normalize setters to [0, 360). Policies require
finite, positive ordered bounds and sorted, finite steps within those bounds.

`pan_pixels(ppp, (dx, dy))` moves content by device pixels; pass the current screen ppp.
`fit` centres the document and fits its rotated bounds with logical-point padding; `fill`
centres it and covers the viewport, including at nonzero rotation. Policy bounds may prevent
the requested fit or fill. Document rectangles should have finite, strictly positive sizes.
`Anchor::Point` is a screen-space position. To move a clicked point to the viewport centre,
use `zoom_about_and_centre`. `as_tstransform` is available only without rotation or aspect
correction and uses f32. Page selection, page layout, flips, and animated cameras remain
application responsibilities.
