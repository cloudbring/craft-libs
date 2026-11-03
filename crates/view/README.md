# craft-view

Shared canvas transforms, anchored zoom, rotation, panning and fit for egui applications.
Only `emath` is required. Coordinates are logical points; zoom is physical pixels per
document unit, converted using pixels per point (`ppp`). A view requires finite,
positive zoom and ppp. Policies require finite, positive ordered bounds and sorted,
finite steps within those bounds. Fit may be limited by the policy's minimum zoom.

`Anchor::Point` is a screen-space position. For a clicked point that should move to
the viewport centre, use `zoom_about_and_centre`. Page selection, page layout,
aspect correction, flips, and animated cameras remain application responsibilities.
