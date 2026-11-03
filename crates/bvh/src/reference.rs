//! Verbatim binned-SAH build from `materialcraft/crates/geom/src/bvh.rs`
//! (byte-identical to `texturecraft/crates/geom/src/bvh.rs`), `Bvh::new`.
//!
//! `glam::Vec3` / `Vec3A` are `[f32; 3]` here. Each candidate split still folds both bin
//! ranges (`O(bins²)` per node, issue #134). The tie break is unchanged: the lowest split
//! index whose cost is strictly smaller wins.

use crate::Triangle;
use crate::math::{self, area, centroid, grow};

const LEAF: usize = 4;
const BINS: usize = 12;

#[derive(Clone, Copy)]
struct Bin {
    count: u32,
    lo: [f32; 3],
    hi: [f32; 3],
}

struct SplitAxis {
    axis: usize,
    origin: f32,
    width: f32,
}

pub(crate) struct Built {
    pub nodes: Vec<([f32; 3], [f32; 3], u32, u32)>,
    pub tris: Vec<Triangle>,
    pub order: Vec<u32>,
}

pub(crate) fn build(tris_in: &[Triangle]) -> Built {
    let tris = tris_in.to_vec();
    let mut order: Vec<u32> = (0..tris.len() as u32).collect();
    let cent: Vec<[f32; 3]> = tris.iter().map(|t| centroid(*t)).collect();
    let mut nodes = vec![([0.0; 3], [0.0; 3], 0u32, tris.len() as u32)];
    let mut stack = vec![0usize];
    while let Some(ni) = stack.pop() {
        let Some((_, _, first, count)) = nodes.get(ni).copied() else { continue };
        let (f, c) = (first as usize, count as usize);
        let (lo, hi, clo, chi) = bounds(&order, &tris, &cent, f, c);
        if let Some(n) = nodes.get_mut(ni) {
            n.0 = lo;
            n.1 = hi;
        }
        if c <= LEAF {
            continue;
        }
        let ext = math::sub(chi, clo);
        let axis = math::widest(ext);
        let width = math::axis(ext, axis);
        if width <= 1e-12 {
            continue;
        }
        let split_axis = SplitAxis { axis, origin: math::axis(clo, axis), width };
        let bins = fill_bins(&order, &tris, &cent, f, c, &split_axis);
        let (best, best_cost) = rescan(&bins);
        if best == 0 || best_cost >= c as f32 * area(lo, hi) {
            continue;
        }
        let i = partition(&mut order, &cent, f, c, &split_axis, best);
        if i == 0 || i == c {
            continue;
        }
        let left = nodes.len();
        nodes.push(([0.0; 3], [0.0; 3], first, i as u32));
        nodes.push(([0.0; 3], [0.0; 3], first + i as u32, (c - i) as u32));
        if let Some(n) = nodes.get_mut(ni) {
            n.2 = u32::try_from(left).unwrap_or(0);
            n.3 = 0;
        }
        stack.push(left);
        stack.push(left + 1);
    }
    let ordered = order.iter().map(|&t| tris.get(t as usize).copied().unwrap_or([[0.0; 3]; 3])).collect();
    Built { nodes, tris: ordered, order }
}

fn bounds(order: &[u32], tris: &[Triangle], cent: &[[f32; 3]], f: usize, c: usize) -> ([f32; 3], [f32; 3], [f32; 3], [f32; 3]) {
    let mut lo = [f32::MAX; 3];
    let mut hi = [f32::MIN; 3];
    let mut clo = [f32::MAX; 3];
    let mut chi = [f32::MIN; 3];
    for &t in order.get(f..f.saturating_add(c)).unwrap_or(&[]) {
        if let Some(tri) = tris.get(t as usize) {
            for p in tri {
                grow(&mut lo, &mut hi, *p);
            }
        }
        if let Some(p) = cent.get(t as usize) {
            grow(&mut clo, &mut chi, *p);
        }
    }
    (lo, hi, clo, chi)
}

fn fill_bins(order: &[u32], tris: &[Triangle], cent: &[[f32; 3]], f: usize, c: usize, axis: &SplitAxis) -> [Bin; BINS] {
    let mut bins = [Bin { count: 0, lo: [f32::MAX; 3], hi: [f32::MIN; 3] }; BINS];
    for &t in order.get(f..f.saturating_add(c)).unwrap_or(&[]) {
        let Some(bin) = bins.get_mut(bin_of(cent, t, axis)) else { continue };
        bin.count = bin.count.saturating_add(1);
        if let Some(tri) = tris.get(t as usize) {
            for p in tri {
                grow(&mut bin.lo, &mut bin.hi, *p);
            }
        }
    }
    bins
}

/// The original split loop: both sides are folded again for every candidate.
fn rescan(bins: &[Bin; BINS]) -> (usize, f32) {
    let mut best = 0usize;
    let mut best_cost = f32::MAX;
    for split in 1..BINS {
        let (nl, ll, lh) = fold(&bins[..split]);
        let (nr, rl, rh) = fold(&bins[split..]);
        if nl == 0 || nr == 0 {
            continue;
        }
        let cost = nl as f32 * area(ll, lh) + nr as f32 * area(rl, rh);
        if cost < best_cost {
            best_cost = cost;
            best = split;
        }
    }
    (best, best_cost)
}

fn fold(bins: &[Bin]) -> (u32, [f32; 3], [f32; 3]) {
    bins.iter().fold((0u32, [f32::MAX; 3], [f32::MIN; 3]), |(n, l, h), b| (n.saturating_add(b.count), math::min3(l, b.lo), math::max3(h, b.hi)))
}

fn partition(order: &mut [u32], cent: &[[f32; 3]], f: usize, c: usize, axis: &SplitAxis, best: usize) -> usize {
    let Some(slice) = order.get_mut(f..f.saturating_add(c)) else { return 0 };
    let mut i = 0;
    for j in 0..slice.len() {
        let Some(id) = slice.get(j).copied() else { continue };
        if bin_of(cent, id, axis) < best {
            slice.swap(i, j);
            i += 1;
        }
    }
    i
}

fn bin_of(cent: &[[f32; 3]], tri: u32, axis: &SplitAxis) -> usize {
    let Some(p) = cent.get(tri as usize) else { return 0 };
    let t = (math::axis(*p, axis.axis) - axis.origin) / axis.width * BINS as f32;
    (t as usize).min(BINS - 1)
}
