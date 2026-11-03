//! `[f32; 3]` arithmetic shared by the builder and the queries.

pub(crate) fn min3(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0].min(b[0]), a[1].min(b[1]), a[2].min(b[2])]
}

pub(crate) fn max3(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0].max(b[0]), a[1].max(b[1]), a[2].max(b[2])]
}

pub(crate) fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

pub(crate) fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

pub(crate) fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

pub(crate) fn grow(lo: &mut [f32; 3], hi: &mut [f32; 3], p: [f32; 3]) {
    *lo = min3(*lo, p);
    *hi = max3(*hi, p);
}

/// Half the surface area, the quantity the binned SAH minimises.
pub(crate) fn area(lo: [f32; 3], hi: [f32; 3]) -> f32 {
    let d = max3(sub(hi, lo), [0.0; 3]);
    d[0] * d[1] + d[1] * d[2] + d[2] * d[0]
}

pub(crate) fn centroid(t: [[f32; 3]; 3]) -> [f32; 3] {
    [(t[0][0] + t[1][0] + t[2][0]) / 3.0, (t[0][1] + t[1][1] + t[2][1]) / 3.0, (t[0][2] + t[1][2] + t[2][2]) / 3.0]
}

pub(crate) fn axis(v: [f32; 3], axis: usize) -> f32 {
    match axis {
        0 => v[0],
        1 => v[1],
        _ => v[2],
    }
}

pub(crate) fn widest(ext: [f32; 3]) -> usize {
    if ext[0] >= ext[1] && ext[0] >= ext[2] {
        0
    } else if ext[1] >= ext[2] {
        1
    } else {
        2
    }
}

pub(crate) fn finite(p: [f32; 3]) -> bool {
    p[0].is_finite() && p[1].is_finite() && p[2].is_finite()
}
