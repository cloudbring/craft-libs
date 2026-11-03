//! Binned surface-area heuristic. Each node bins its triangles once, then a prefix and a
//! suffix sweep make every candidate split O(1): O(bins) per node instead of O(bins²).

use crate::math::{self, area, centroid, grow};
use crate::{Bvh, Node, Triangle};

/// Triangles per leaf before a split is considered.
const LEAF: usize = 4;
/// Bins along the split axis (Wald, Boulos, Shirley 2007).
const BINS: usize = 12;

#[derive(Clone, Copy)]
struct Bin {
    count: u32,
    lo: [f32; 3],
    hi: [f32; 3],
}

impl Bin {
    const EMPTY: Self = Self { count: 0, lo: [f32::MAX; 3], hi: [f32::MIN; 3] };
}

struct Extent {
    lo: [f32; 3],
    hi: [f32; 3],
    clo: [f32; 3],
    chi: [f32; 3],
}

struct Axis {
    axis: usize,
    origin: f32,
    width: f32,
}

struct Plan {
    axis: Axis,
    split: usize,
}

pub(crate) fn build(tris_in: &[Triangle]) -> Bvh {
    let tris = tris_in.to_vec();
    let mut order: Vec<u32> = (0..tris.len() as u32).collect();
    let cent: Vec<[f32; 3]> = tris.iter().map(|t| centroid(*t)).collect();
    let mut nodes = vec![Node { lo: [0.0; 3], hi: [0.0; 3], first: 0, count: tris.len() as u32 }];
    let mut stack = vec![0usize];
    while let Some(ni) = stack.pop() {
        refine(&mut nodes, &mut order, &tris, &cent, &mut stack, ni);
    }
    let ordered = reorder(&tris, &order);
    Bvh { nodes, tris: ordered, order }
}

fn refine(nodes: &mut Vec<Node>, order: &mut [u32], tris: &[Triangle], cent: &[[f32; 3]], stack: &mut Vec<usize>, ni: usize) {
    let Some(node) = nodes.get(ni).copied() else { return };
    let (f, c) = (node.first as usize, node.count as usize);
    let ext = extent(order, tris, cent, f, c);
    if let Some(n) = nodes.get_mut(ni) {
        n.lo = ext.lo;
        n.hi = ext.hi;
    }
    let Some(plan) = choose_split(&ext, order, tris, cent, f, c) else { return };
    let left = partition(order, cent, f, c, &plan);
    if left == 0 || left == c {
        return;
    }
    let child = nodes.len();
    nodes.push(Node { lo: [0.0; 3], hi: [0.0; 3], first: node.first, count: left as u32 });
    nodes.push(Node { lo: [0.0; 3], hi: [0.0; 3], first: node.first + left as u32, count: (c - left) as u32 });
    if let Some(n) = nodes.get_mut(ni) {
        n.first = u32::try_from(child).unwrap_or(0);
        n.count = 0;
    }
    stack.push(child);
    stack.push(child + 1);
}

fn extent(order: &[u32], tris: &[Triangle], cent: &[[f32; 3]], f: usize, c: usize) -> Extent {
    let mut lo = [f32::MAX; 3];
    let mut hi = [f32::MIN; 3];
    let mut clo = [f32::MAX; 3];
    let mut chi = [f32::MIN; 3];
    let ids = order.get(f..f.saturating_add(c)).unwrap_or(&[]);
    for &t in ids {
        if let Some(tri) = tris.get(t as usize) {
            for p in tri {
                grow(&mut lo, &mut hi, *p);
            }
        }
        if let Some(p) = cent.get(t as usize) {
            grow(&mut clo, &mut chi, *p);
        }
    }
    Extent { lo, hi, clo, chi }
}

fn choose_split(ext: &Extent, order: &[u32], tris: &[Triangle], cent: &[[f32; 3]], f: usize, c: usize) -> Option<Plan> {
    if c <= LEAF {
        return None;
    }
    let span = math::sub(ext.chi, ext.clo);
    let axis = math::widest(span);
    let width = math::axis(span, axis);
    if width <= 1e-12 {
        return None;
    }
    let axis = Axis { axis, origin: math::axis(ext.clo, axis), width };
    let bins = fill_bins(order, tris, cent, f, c, &axis);
    let (split, cost) = sweep_best(&bins);
    if split == 0 || cost >= c as f32 * area(ext.lo, ext.hi) {
        return None;
    }
    Some(Plan { axis, split })
}

fn fill_bins(order: &[u32], tris: &[Triangle], cent: &[[f32; 3]], f: usize, c: usize, axis: &Axis) -> [Bin; BINS] {
    let mut bins = [Bin::EMPTY; BINS];
    let ids = order.get(f..f.saturating_add(c)).unwrap_or(&[]);
    for &t in ids {
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

/// Lowest split index whose cost is strictly smaller than the others. Left bounds are the
/// left-to-right fold of bins `0..split`. Right bounds are the same fold of `split..BINS`,
/// accumulated from the right so an equal component keeps the later operand, matching `f32::min`
/// / `f32::max` in the rescanning builder.
fn sweep_best(bins: &[Bin; BINS]) -> (usize, f32) {
    let (left_n, left_lo, left_hi) = prefix(bins);
    let (right_n, right_lo, right_hi) = suffix(bins);
    let mut best = 0usize;
    let mut best_cost = f32::MAX;
    for split in 1..BINS {
        let (Some(&nl), Some(&nr)) = (left_n.get(split), right_n.get(split)) else { continue };
        if nl == 0 || nr == 0 {
            continue;
        }
        let (Some(ll), Some(lh), Some(rl), Some(rh)) = (left_lo.get(split), left_hi.get(split), right_lo.get(split), right_hi.get(split)) else {
            continue;
        };
        let cost = nl as f32 * area(*ll, *lh) + nr as f32 * area(*rl, *rh);
        if cost < best_cost {
            best_cost = cost;
            best = split;
        }
    }
    (best, best_cost)
}

fn prefix(bins: &[Bin; BINS]) -> ([u32; BINS], [[f32; 3]; BINS], [[f32; 3]; BINS]) {
    let mut n = 0u32;
    let mut lo = [f32::MAX; 3];
    let mut hi = [f32::MIN; 3];
    let mut count = [0u32; BINS];
    let mut los = [[f32::MAX; 3]; BINS];
    let mut his = [[f32::MIN; 3]; BINS];
    for (i, bin) in bins.iter().enumerate() {
        n = n.saturating_add(bin.count);
        lo = math::min3(lo, bin.lo);
        hi = math::max3(hi, bin.hi);
        let split = i + 1;
        if let Some(slot) = count.get_mut(split) {
            *slot = n;
        }
        if let Some(slot) = los.get_mut(split) {
            *slot = lo;
        }
        if let Some(slot) = his.get_mut(split) {
            *slot = hi;
        }
    }
    (count, los, his)
}

fn suffix(bins: &[Bin; BINS]) -> ([u32; BINS], [[f32; 3]; BINS], [[f32; 3]; BINS]) {
    let mut n = 0u32;
    let mut lo = [f32::MAX; 3];
    let mut hi = [f32::MIN; 3];
    let mut count = [0u32; BINS];
    let mut los = [[f32::MAX; 3]; BINS];
    let mut his = [[f32::MIN; 3]; BINS];
    for (i, bin) in bins.iter().enumerate().rev() {
        n = n.saturating_add(bin.count);
        // Bin first, then the already-folded bins to its right: the same bits as folding
        // `bins[i..]` left to right (`f32::min` / `max` keep the later operand when equal).
        lo = math::min3(bin.lo, lo);
        hi = math::max3(bin.hi, hi);
        if let Some(slot) = count.get_mut(i) {
            *slot = n;
        }
        if let Some(slot) = los.get_mut(i) {
            *slot = lo;
        }
        if let Some(slot) = his.get_mut(i) {
            *slot = hi;
        }
    }
    (count, los, his)
}

fn partition(order: &mut [u32], cent: &[[f32; 3]], f: usize, c: usize, plan: &Plan) -> usize {
    let Some(slice) = order.get_mut(f..f.saturating_add(c)) else { return 0 };
    let mut i = 0;
    for j in 0..slice.len() {
        let Some(id) = slice.get(j).copied() else { continue };
        if bin_of(cent, id, &plan.axis) < plan.split {
            slice.swap(i, j);
            i += 1;
        }
    }
    i
}

fn bin_of(cent: &[[f32; 3]], tri: u32, axis: &Axis) -> usize {
    let Some(p) = cent.get(tri as usize) else { return 0 };
    let t = (math::axis(*p, axis.axis) - axis.origin) / axis.width * BINS as f32;
    (t as usize).min(BINS - 1)
}

fn reorder(tris: &[Triangle], order: &[u32]) -> Vec<Triangle> {
    order.iter().map(|&t| tris.get(t as usize).copied().unwrap_or([[0.0; 3]; 3])).collect()
}
