#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![forbid(unsafe_code)]

//! Bounding volume hierarchy over a triangle mesh: closest-hit and any-hit ray casts,
//! nearest-point queries and box overlap.
//!
//! The build is the binned surface-area heuristic (12 bins, leaf size 4) used by
//! MaterialCraft and TextureCraft. Positions are `[f32; 3]`. The apps keep their own
//! `Vec3` and `Aabb` types and convert at the boundary, so this crate has no
//! dependencies and does not pull in either geometry crate.
//!
//! ```
//! use craft_bvh::Bvh;
//!
//! let tri = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
//! let bvh = Bvh::new(&[tri]);
//! let hit = bvh.closest([0.2, 0.2, 1.0], [0.0, 0.0, -1.0], 10.0);
//! assert!(hit.is_some());
//! ```

mod build;
mod math;

#[cfg(test)]
mod reference;
#[cfg(test)]
mod tests;

/// One triangle, three vertex positions.
pub type Triangle = [[f32; 3]; 3];

/// A ray hit, or the nearest point on a triangle.
///
/// For a ray, `t` is the distance along `origin + t·direction`. For [`Bvh::nearest`],
/// `t` is the Euclidean distance. `u` and `v` are the weights of vertices 1 and 2
/// (vertex 0 has weight `1 - u - v`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hit {
    pub t: f32,
    pub tri: u32,
    pub u: f32,
    pub v: f32,
}

#[derive(Clone, Copy, Debug)]
struct Node {
    lo: [f32; 3],
    hi: [f32; 3],
    /// Leaf: first triangle in `order`. Inner: index of the left child (right = left + 1).
    first: u32,
    /// Triangles in a leaf; 0 for an inner node.
    count: u32,
}

/// A binned-SAH hierarchy over a list of triangles.
#[derive(Clone, Debug)]
pub struct Bvh {
    nodes: Vec<Node>,
    /// Triangle vertices in BVH order.
    tris: Vec<Triangle>,
    /// Original triangle index of each entry in `tris`.
    order: Vec<u32>,
}

struct LeafHit<'a> {
    o: [f32; 3],
    d: [f32; 3],
    tmax: &'a mut f32,
    any: bool,
    filter: &'a dyn Fn(u32) -> bool,
    best: &'a mut Option<Hit>,
}

/// Traversal stack of (node, entry distance): inline for typical depths, spilling to
/// the heap for a degenerate tree. No allocation on the common path.
struct Stack {
    inline: [(u32, f32); 64],
    len: usize,
    spill: Vec<(u32, f32)>,
}

impl Default for Stack {
    fn default() -> Self {
        Self { inline: [(0, 0.0); 64], len: 0, spill: Vec::new() }
    }
}

impl Stack {
    fn push(&mut self, v: (u32, f32)) {
        match self.inline.get_mut(self.len) {
            Some(slot) if self.spill.is_empty() => {
                *slot = v;
                self.len += 1;
            }
            _ => self.spill.push(v),
        }
    }

    fn pop(&mut self) -> Option<(u32, f32)> {
        self.spill.pop().or_else(|| {
            self.len = self.len.checked_sub(1)?;
            self.inline.get(self.len).copied()
        })
    }
}

impl Bvh {
    /// Build a hierarchy over `tris`. The index reported by a query is the index in this slice.
    pub fn new(tris: &[Triangle]) -> Bvh {
        build::build(tris)
    }

    /// No triangles were given.
    pub fn is_empty(&self) -> bool {
        self.tris.is_empty()
    }

    /// The flattened tree (for a GPU upload), root first: bounds, then for a leaf the range
    /// `first..first + count` of [`Bvh::triangles`], and for an inner node (`count` 0) the
    /// children `first` and `first + 1`.
    pub fn nodes(&self) -> impl ExactSizeIterator<Item = ([f32; 3], [f32; 3], u32, u32)> + '_ {
        self.nodes.iter().map(|n| (n.lo, n.hi, n.first, n.count))
    }

    /// Triangle vertices in BVH order, and the original triangle index of each.
    pub fn triangles(&self) -> (&[Triangle], &[u32]) {
        (&self.tris, &self.order)
    }

    fn slab(lo: [f32; 3], hi: [f32; 3], o: [f32; 3], inv: [f32; 3], tmax: f32) -> Option<f32> {
        let mut tn = f32::NEG_INFINITY;
        let mut tf = f32::INFINITY;
        for axis in 0..3 {
            let t0 = (lo[axis] - o[axis]) * inv[axis];
            let t1 = (hi[axis] - o[axis]) * inv[axis];
            // A ray lying in a slab plane gives 0·∞ = NaN: it is inside that slab, so the axis
            // does not bound the hit.
            let (near, far) = if t0.is_nan() || t1.is_nan() { (f32::NEG_INFINITY, f32::INFINITY) } else { (t0.min(t1), t0.max(t1)) };
            tn = tn.max(near);
            tf = tf.min(far);
        }
        tn = tn.max(0.0);
        tf = tf.min(tmax);
        (tn <= tf).then_some(tn)
    }

    /// Möller–Trumbore, double sided.
    fn hit_tri(t: &Triangle, o: [f32; 3], d: [f32; 3]) -> Option<(f32, f32, f32)> {
        let e1 = math::sub(t[1], t[0]);
        let e2 = math::sub(t[2], t[0]);
        let p = math::cross(d, e2);
        let det = math::dot(e1, p);
        if det.abs() < 1e-12 {
            return None;
        }
        let inv = 1.0 / det;
        let s = math::sub(o, t[0]);
        // A little slack on the edges: rays through a shared edge must not slip between its
        // two triangles to a surface behind.
        const EDGE: f32 = 1e-5;
        let u = math::dot(s, p) * inv;
        if !(-EDGE..=1.0 + EDGE).contains(&u) {
            return None;
        }
        let q = math::cross(s, e1);
        let v = math::dot(d, q) * inv;
        if v < -EDGE || u + v > 1.0 + EDGE {
            return None;
        }
        let tt = math::dot(e2, q) * inv;
        (tt > 0.0).then_some((tt, u, v))
    }

    fn traverse(&self, o: [f32; 3], d: [f32; 3], mut tmax: f32, any: bool, filter: &dyn Fn(u32) -> bool) -> Option<Hit> {
        let root = self.nodes.first().filter(|_| !self.tris.is_empty())?;
        let inv = [1.0 / d[0], 1.0 / d[1], 1.0 / d[2]];
        let mut best = None;
        let mut stack = Stack::default();
        stack.push((0, Self::slab(root.lo, root.hi, o, inv, tmax)?));
        // Entries carry their entry distance: nodes behind a closer hit are skipped unopened.
        while let Some((ni, tn)) = stack.pop() {
            let Some(n) = self.nodes.get(ni as usize).filter(|_| tn <= tmax) else { continue };
            if n.count > 0 {
                let stop = self.scan_leaf(n, &mut LeafHit { o, d, tmax: &mut tmax, any, filter, best: &mut best });
                if stop {
                    return best;
                }
            } else {
                self.push_children(n, o, inv, tmax, &mut stack);
            }
        }
        best
    }

    fn scan_leaf(&self, n: &Node, hit: &mut LeafHit<'_>) -> bool {
        let f = n.first as usize;
        let end = (f + n.count as usize).min(self.tris.len());
        for i in f..end {
            let (Some(tri), Some(&orig)) = (self.tris.get(i), self.order.get(i)) else { continue };
            if let Some((t, u, v)) = Self::hit_tri(tri, hit.o, hit.d)
                && t < *hit.tmax
                && (hit.filter)(orig)
            {
                *hit.tmax = t;
                *hit.best = Some(Hit { t, tri: orig, u, v });
                if hit.any {
                    return true;
                }
            }
        }
        false
    }

    fn push_children(&self, n: &Node, o: [f32; 3], inv: [f32; 3], tmax: f32, stack: &mut Stack) {
        // Visit the nearer child first.
        let (l, r) = (n.first, n.first + 1);
        let dl = self.nodes.get(l as usize).and_then(|c| Self::slab(c.lo, c.hi, o, inv, tmax));
        let dr = self.nodes.get(r as usize).and_then(|c| Self::slab(c.lo, c.hi, o, inv, tmax));
        match (dl, dr) {
            (Some(a), Some(b)) if a <= b => {
                stack.push((r, b));
                stack.push((l, a));
            }
            (Some(a), Some(b)) => {
                stack.push((l, a));
                stack.push((r, b));
            }
            (Some(a), None) => stack.push((l, a)),
            (None, Some(b)) => stack.push((r, b)),
            (None, None) => {}
        }
    }

    /// Closest hit along `o + t·d` for `t` in (0, tmax).
    pub fn closest(&self, o: [f32; 3], d: [f32; 3], tmax: f32) -> Option<Hit> {
        self.traverse(o, d, tmax, false, &|_| true)
    }

    /// Closest hit on triangles accepted by `filter` (original triangle index).
    pub fn closest_filtered(&self, o: [f32; 3], d: [f32; 3], tmax: f32, filter: &dyn Fn(u32) -> bool) -> Option<Hit> {
        self.traverse(o, d, tmax, false, filter)
    }

    /// Whether anything is hit before `tmax` (occlusion rays).
    pub fn occluded(&self, o: [f32; 3], d: [f32; 3], tmax: f32) -> bool {
        self.traverse(o, d, tmax, true, &|_| true).is_some()
    }

    /// The point nearest to `p` within `max_dist` on a triangle accepted by `filter`
    /// (original index). [`Hit::t`] is the distance.
    pub fn nearest(&self, p: [f32; 3], max_dist: f32, filter: &dyn Fn(u32) -> bool) -> Option<Hit> {
        let root = self.nodes.first().filter(|_| !self.tris.is_empty() && math::finite(p))?;
        let box_d2 = |n: &Node| outside_distance_squared(n.lo, n.hi, p);
        let (mut best_d2, mut best) = (max_dist * max_dist, None);
        let mut stack = Stack::default();
        stack.push((0, box_d2(root)));
        while let Some((ni, d2)) = stack.pop() {
            let Some(n) = self.nodes.get(ni as usize).filter(|_| d2 <= best_d2) else { continue };
            if n.count > 0 {
                self.scan_nearest(n, p, filter, &mut best_d2, &mut best);
            } else {
                self.push_nearer(n, box_d2, &mut stack);
            }
        }
        best
    }

    fn scan_nearest(&self, n: &Node, p: [f32; 3], filter: &dyn Fn(u32) -> bool, best_d2: &mut f32, best: &mut Option<Hit>) {
        let f = n.first as usize;
        let end = (f + n.count as usize).min(self.tris.len());
        for i in f..end {
            let (Some(tri), Some(&orig)) = (self.tris.get(i), self.order.get(i)) else { continue };
            if !filter(orig) {
                continue;
            }
            let (q, u, v) = closest_on_triangle(p, tri);
            let dd = distance_squared(q, p);
            if dd < *best_d2 {
                *best_d2 = dd;
                *best = Some(Hit { t: dd.sqrt(), tri: orig, u, v });
            }
        }
    }

    fn push_nearer(&self, n: &Node, box_d2: impl Fn(&Node) -> f32, stack: &mut Stack) {
        // Push the farther child first so the nearer one is searched first.
        let child = |c: u32| self.nodes.get(c as usize).map(|n| (c, box_d2(n)));
        let (Some(l), Some(r)) = (child(n.first), child(n.first + 1)) else { return };
        let (near, far) = if l.1 <= r.1 { (l, r) } else { (r, l) };
        stack.push(far);
        stack.push(near);
    }

    /// Call `f` with every triangle (original index) whose bounds overlap the box.
    pub fn query_box(&self, lo: [f32; 3], hi: [f32; 3], mut f: impl FnMut(u32)) {
        let mut stack = vec![0u32];
        while let Some(i) = stack.pop() {
            let Some(n) = self.nodes.get(i as usize) else { continue };
            if self.tris.is_empty() || separated(n.lo, n.hi, lo, hi) {
                continue;
            }
            if n.count > 0 {
                self.scan_box(n, lo, hi, &mut f);
            } else {
                stack.push(n.first);
                stack.push(n.first + 1);
            }
        }
    }

    fn scan_box(&self, n: &Node, lo: [f32; 3], hi: [f32; 3], f: &mut impl FnMut(u32)) {
        let start = n.first as usize;
        let end = (n.first + n.count) as usize;
        for k in start..end {
            let (Some(t), Some(&orig)) = (self.tris.get(k), self.order.get(k)) else { continue };
            let tlo = math::min3(math::min3(t[0], t[1]), t[2]);
            let thi = math::max3(math::max3(t[0], t[1]), t[2]);
            if !separated(tlo, thi, lo, hi) {
                f(orig);
            }
        }
    }
}

fn outside_distance_squared(lo: [f32; 3], hi: [f32; 3], p: [f32; 3]) -> f32 {
    let mut s = 0.0;
    for axis in 0..3 {
        let d = (lo[axis] - p[axis]).max(p[axis] - hi[axis]).max(0.0);
        s += d * d;
    }
    s
}

fn distance_squared(a: [f32; 3], b: [f32; 3]) -> f32 {
    let d = math::sub(a, b);
    math::dot(d, d)
}

fn separated(alo: [f32; 3], ahi: [f32; 3], blo: [f32; 3], bhi: [f32; 3]) -> bool {
    (0..3).any(|i| ahi[i] < blo[i] || alo[i] > bhi[i])
}

/// The point of triangle `t` nearest to `p` and its barycentrics (weights of vertices 1 and 2),
/// by Voronoi region (Ericson, Real-Time Collision Detection, 5.1.5). Degenerate triangles give
/// a non-finite distance and are never nearest.
fn closest_on_triangle(p: [f32; 3], t: &Triangle) -> ([f32; 3], f32, f32) {
    let [a, b, c] = *t;
    let (ab, ac, ap) = (math::sub(b, a), math::sub(c, a), math::sub(p, a));
    let (d1, d2) = (math::dot(ab, ap), math::dot(ac, ap));
    if d1 <= 0.0 && d2 <= 0.0 {
        return (a, 0.0, 0.0);
    }
    let bp = math::sub(p, b);
    let (d3, d4) = (math::dot(ab, bp), math::dot(ac, bp));
    if d3 >= 0.0 && d4 <= d3 {
        return (b, 1.0, 0.0);
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        let v = d1 / (d1 - d3);
        return (add(a, scale(ab, v)), v, 0.0);
    }
    let cp = math::sub(p, c);
    let (d5, d6) = (math::dot(ab, cp), math::dot(ac, cp));
    if d6 >= 0.0 && d5 <= d6 {
        return (c, 0.0, 1.0);
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        let w = d2 / (d2 - d6);
        return (add(a, scale(ac, w)), 0.0, w);
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && d4 - d3 >= 0.0 && d5 - d6 >= 0.0 {
        let w = (d4 - d3) / ((d4 - d3) + (d5 - d6));
        return (add(b, scale(math::sub(c, b), w)), 1.0 - w, w);
    }
    let denom = 1.0 / (va + vb + vc);
    let (v, w) = (vb * denom, vc * denom);
    (add(a, add(scale(ab, v), scale(ac, w))), v, w)
}

fn add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn scale(a: [f32; 3], s: f32) -> [f32; 3] {
    [a[0] * s, a[1] * s, a[2] * s]
}
