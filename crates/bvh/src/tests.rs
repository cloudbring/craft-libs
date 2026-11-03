//! Ports of the BVH tests in `materialcraft/crates/geom/src/bvh.rs`, which is byte-identical
//! to `texturecraft/crates/geom/src/bvh.rs`. The mesh primitives are generated here.
//!
//! The differential tests compare the prefix/suffix sweep with the rescanning builder in
//! [`super::reference`], transcribed from that file.

use std::f32::consts::{PI, TAU};
use std::time::Instant;

use proptest::prelude::*;

use super::reference;
use super::{Bvh, Stack, Triangle, closest_on_triangle};

fn add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn scale(a: [f32; 3], s: f32) -> [f32; 3] {
    [a[0] * s, a[1] * s, a[2] * s]
}

fn length(v: [f32; 3]) -> f32 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}

fn normalize(v: [f32; 3]) -> [f32; 3] {
    scale(v, 1.0 / length(v))
}

fn bits(p: [f32; 3]) -> [u32; 3] {
    [p[0].to_bits(), p[1].to_bits(), p[2].to_bits()]
}

fn grid(rows: usize, cols: usize, mut f: impl FnMut(usize, usize) -> [f32; 3]) -> Vec<Triangle> {
    let mut p = Vec::with_capacity((rows + 1) * (cols + 1));
    for i in 0..=rows {
        for j in 0..=cols {
            p.push(f(i, j));
        }
    }
    let w = cols + 1;
    let mut tris = Vec::with_capacity(rows * cols * 2);
    for i in 0..rows {
        for j in 0..cols {
            let a = i * w + j;
            let (b, c, d) = (a + 1, a + w, a + w + 1);
            tris.push([p[a], p[c], p[d]]);
            tris.push([p[a], p[d], p[b]]);
        }
    }
    tris
}

fn sphere(center: [f32; 3], radius: f32, segments: usize, rings: usize) -> Vec<Triangle> {
    let (segments, rings) = (segments.max(3), rings.max(2));
    grid(rings, segments, |i, j| {
        let (u, v) = (j as f32 / segments as f32, i as f32 / rings as f32);
        let (theta, phi) = (v * PI, u * TAU + PI * 1.5);
        let n = [theta.sin() * phi.cos(), theta.cos(), -theta.sin() * phi.sin()];
        add(center, scale(n, radius))
    })
}

fn torus(major: f32, minor: f32, segs: usize, sides: usize) -> Vec<Triangle> {
    let (segs, sides) = (segs.max(3), sides.max(3));
    grid(sides, segs, |i, j| {
        let (u, v) = (j as f32 / segs as f32, i as f32 / sides as f32);
        let phi = u * TAU + PI * 1.5;
        let theta = v * TAU;
        let dir = [phi.cos(), 0.0, -phi.sin()];
        let n = [dir[0] * theta.cos(), -theta.sin(), dir[2] * theta.cos()];
        add(scale(dir, major), scale(n, minor))
    })
}

fn plane(size: f32, subdivisions: usize) -> Vec<Triangle> {
    let s = subdivisions.max(1);
    grid(s, s, |i, j| {
        let (u, v) = (j as f32 / s as f32, i as f32 / s as f32);
        [(u - 0.5) * size, 0.0, (v - 0.5) * size]
    })
}

fn lathe(profile: &[(f32, f32)], segs: usize) -> Vec<Triangle> {
    if profile.len() < 2 {
        return Vec::new();
    }
    let segs = segs.max(3);
    grid(profile.len() - 1, segs, |i, j| {
        let (rad, y) = profile[i];
        let phi = j as f32 / segs as f32 * TAU + PI * 1.5;
        [phi.cos() * rad, y, -phi.sin() * rad]
    })
}

fn disk(y: f32, radius: f32, up: bool, segs: usize) -> Vec<Triangle> {
    let segs = segs.max(3);
    let c = [0.0, y, 0.0];
    let ring: Vec<[f32; 3]> = (0..segs)
        .map(|j| {
            let a = j as f32 / segs as f32 * TAU;
            [a.cos() * radius, y, a.sin() * radius]
        })
        .collect();
    (0..segs)
        .map(|j| {
            let a = ring[j];
            let b = ring[(j + 1) % segs];
            if up { [c, b, a] } else { [c, a, b] }
        })
        .collect()
}

/// Sphere on a lathed pedestal, the shape `primitives::material_ball` builds.
fn material_ball() -> Vec<Triangle> {
    let mut tris = sphere([0.0, 1.32, 0.0], 1.0, 128, 64);
    let (r, top, bot, bev) = (1.05f32, 0.32f32, 0.0f32, 0.07f32);
    let mut profile = vec![(0.42, top + 0.06), (0.62, top + 0.06)];
    for k in 1..=6 {
        let a = k as f32 / 6.0 * std::f32::consts::FRAC_PI_2;
        profile.push((0.62 + 0.06 * a.sin(), top + 0.06 * a.cos()));
    }
    profile.push((r - bev, top));
    for k in 1..=6 {
        let a = k as f32 / 6.0 * std::f32::consts::FRAC_PI_2;
        profile.push((r - bev + bev * a.sin(), top - bev + bev * a.cos()));
    }
    profile.push((r, bot + bev));
    for k in 1..=6 {
        let a = k as f32 / 6.0 * std::f32::consts::FRAC_PI_2;
        profile.push((r - bev + bev * a.cos(), bot + bev - bev * a.sin()));
    }
    tris.extend(lathe(&profile, 128));
    tris.extend(disk(top + 0.06, 0.42, true, 128));
    tris.extend(disk(bot, r - bev, false, 128));
    tris
}

fn soup(n: usize, seed: u32) -> Vec<Triangle> {
    let mut s = seed;
    let mut rnd = || {
        s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (s >> 8) as f32 / (1u32 << 24) as f32
    };
    (0..n)
        .map(|_| {
            let base = [rnd(), rnd(), rnd()];
            let mut j = |b: f32| b * 2.0 - 1.0 + (rnd() * 2.0 - 1.0) * 0.02;
            [[j(base[0]), j(base[1]), j(base[2])], [j(base[0]), j(base[1]), j(base[2])], [j(base[0]), j(base[1]), j(base[2])]]
        })
        .collect()
}

fn assert_same_layout(tris: &[Triangle]) {
    let old = reference::build(tris);
    let new = Bvh::new(tris);
    let nodes: Vec<_> = new.nodes().collect();
    assert_eq!(old.nodes.len(), nodes.len(), "node count");
    for (i, (a, b)) in old.nodes.iter().zip(&nodes).enumerate() {
        assert_eq!((a.2, a.3), (b.2, b.3), "links at {i}");
        assert_eq!(bits(a.0), bits(b.0), "lo at {i}");
        assert_eq!(bits(a.1), bits(b.1), "hi at {i}");
    }
    assert_eq!(old.order, new.triangles().1);
    assert_eq!(old.tris.len(), new.triangles().0.len());
    for (a, b) in old.tris.iter().zip(new.triangles().0) {
        for k in 0..3 {
            assert_eq!(bits(a[k]), bits(b[k]));
        }
    }
}

fn assert_closest(bvh: &Bvh, tris: &[Triangle], o: [f32; 3], d: [f32; 3], tmax: f32) {
    let fast = bvh.closest(o, d, tmax).map(|h| h.t);
    let mut slow: Option<f32> = None;
    for tri in tris {
        if let Some((tt, _, _)) = Bvh::hit_tri(tri, o, d)
            && tt < tmax
        {
            slow = Some(slow.map_or(tt, |s| s.min(tt)));
        }
    }
    match (fast, slow) {
        (Some(a), Some(b)) => assert!((a - b).abs() < 1e-4, "{a} vs {b}"),
        (None, None) => {}
        other => panic!("mismatch {other:?}"),
    }
}

fn assert_valid_tree(bvh: &Bvh, tris: &[Triangle]) {
    let nodes: Vec<_> = bvh.nodes().collect();
    let (ordered, index) = bvh.triangles();
    assert_eq!(ordered.len(), tris.len());
    assert_eq!(index.len(), tris.len());
    if tris.is_empty() {
        assert!(bvh.is_empty());
        assert!(bvh.closest([0.0; 3], [1.0, 0.0, 0.0], 1.0).is_none());
        return;
    }
    let mut seen = vec![false; tris.len()];
    let mut stack = vec![0usize];
    let mut visits = 0usize;
    while let Some(i) = stack.pop() {
        visits += 1;
        assert!(visits <= nodes.len(), "cycle");
        let (lo, hi, first, count) = nodes[i];
        if count > 0 {
            let f = first as usize;
            let c = count as usize;
            assert!(f + c <= ordered.len());
            for k in f..f + c {
                let orig = index[k] as usize;
                assert!(orig < tris.len() && !seen[orig], "triangle {orig}");
                seen[orig] = true;
                for v in 0..3 {
                    assert_eq!(bits(ordered[k][v]), bits(tris[orig][v]));
                    let p = ordered[k][v];
                    for axis in 0..3 {
                        assert!(lo[axis] <= p[axis] && p[axis] <= hi[axis], "vertex outside leaf");
                    }
                }
            }
        } else {
            let l = first as usize;
            let r = l + 1;
            assert!(l > i && r < nodes.len(), "children of {i}");
            let (llo, lhi, _, _) = nodes[l];
            let (rlo, rhi, _, _) = nodes[r];
            for axis in 0..3 {
                assert!(lo[axis] <= llo[axis] && lhi[axis] <= hi[axis]);
                assert!(lo[axis] <= rlo[axis] && rhi[axis] <= hi[axis]);
            }
            stack.push(r);
            stack.push(l);
        }
    }
    assert!(seen.iter().all(|s| *s), "a triangle was dropped");
}

#[test]
fn nearest_point_matches_brute_force() {
    let tris = torus(1.0, 0.35, 128, 48);
    let bvh = Bvh::new(&tris);
    let mut seed = 7u32;
    let mut rnd = || {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (seed >> 8) as f32 / (1u32 << 24) as f32 * 4.0 - 2.0
    };
    for _ in 0..200 {
        let p = [rnd(), rnd(), rnd()];
        let hit = bvh.nearest(p, f32::MAX, &|_| true).unwrap();
        let brute = tris.iter().map(|t| length(sub(closest_on_triangle(p, t).0, p))).fold(f32::MAX, f32::min);
        assert!((hit.t - brute).abs() < 1e-5, "{} vs {brute}", hit.t);
        let [a, b, c] = tris[hit.tri as usize];
        let w0 = 1.0 - hit.u - hit.v;
        let q = add(add(scale(a, w0), scale(b, hit.u)), scale(c, hit.v));
        assert!((length(sub(q, p)) - hit.t).abs() < 1e-4);
    }
    assert!(bvh.nearest([100.0; 3], 1.0, &|_| true).is_none(), "out of range");
    assert!(bvh.nearest([0.0; 3], f32::MAX, &|_| false).is_none(), "filtered out");
}

#[test]
fn traversal_stack_spills_in_lifo_order() {
    let mut st = Stack::default();
    for v in 0..200 {
        st.push((v, v as f32));
    }
    assert!((0..200).rev().all(|v| st.pop() == Some((v, v as f32))));
    assert_eq!(st.pop(), None);
}

#[test]
fn ray_hits_sphere_at_expected_distance() {
    let tris = sphere([0.0; 3], 1.0, 64, 32);
    let bvh = Bvh::new(&tris);
    let h = bvh.closest([0.0, 0.0, 5.0], [0.0, 0.0, -1.0], 100.0).unwrap();
    assert!((h.t - 4.0).abs() < 0.01, "{}", h.t);
    assert!(bvh.closest([0.0, 3.0, 5.0], [0.0, 0.0, -1.0], 100.0).is_none());
    assert!(bvh.closest_filtered([0.0, 0.0, 5.0], [0.0, 0.0, -1.0], 100.0, &|_| false).is_none());
    // Rays lying in a node's bounding plane (y = 0) still find the front.
    for x in [-0.5f32, -0.4, 0.3] {
        let h = bvh.closest([x, 0.0, 5.0], [0.0, 0.0, -1.0], 100.0).unwrap();
        assert!(h.t < 5.0, "x {x}: hit the back at t {}", h.t);
    }
    // Rays through shared edges (the equator) hit the front, never slip through to the back.
    let tris = sphere([0.0; 3], 1.0, 48, 24);
    let bvh = Bvh::new(&tris);
    for i in 0..200 {
        let x = i as f32 / 100.0 - 1.0;
        let o = [0.0, 0.0, 5.0];
        let d = normalize(sub([x * 0.9, 0.0, 0.0], o));
        let h = bvh.closest(o, d, 100.0).unwrap();
        assert!(h.t < 5.0, "x {x}: hit the back at t {}", h.t);
    }
    assert!(bvh.occluded([0.0, 0.0, 5.0], [0.0, 0.0, -1.0], 10.0));
    assert!(!bvh.occluded([0.0, 0.0, 5.0], [0.0, 0.0, -1.0], 3.0));
}

#[test]
fn matches_brute_force() {
    let tris = material_ball();
    let bvh = Bvh::new(&tris);
    for i in 0..200 {
        let a = i as f32 * 0.731;
        let o = [a.cos() * 6.0, (i as f32 * 0.37).sin() * 3.0 + 1.0, a.sin() * 6.0];
        let jitter = [(a * 3.1).sin(), (a * 1.7).cos(), (a * 2.3).sin()];
        let aim = [jitter[0] * 0.6, 1.0 + jitter[1] * 0.6, jitter[2] * 0.6];
        let d = normalize(sub(aim, o));
        assert_closest(&bvh, &tris, o, d, 1.0e9);
    }
}

#[test]
fn box_query_finds_overlapping_triangles() {
    let tris = plane(2.0, 4);
    let bvh = Bvh::new(&tris);
    let mut found = Vec::new();
    bvh.query_box([-0.1, -0.1, -0.1], [0.1, 0.1, 0.1], |t| found.push(t));
    assert!(!found.is_empty() && found.len() <= 8, "{found:?}");
    let empty = Bvh::new(&[]);
    assert!(empty.closest([0.0; 3], [1.0, 0.0, 0.0], 1.0).is_none());
    assert!(empty.is_empty());
}

#[test]
fn non_finite_positions_do_not_panic() {
    let tris = [
        [[f32::NAN, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
        [[f32::INFINITY, 0.0, 0.0], [0.0, f32::NEG_INFINITY, 0.0], [1.0, 1.0, 1.0]],
        [[0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
    ];
    let bvh = Bvh::new(&tris);
    let _ = bvh.closest([0.0, 0.0, 1.0], [0.0, 0.0, -1.0], 10.0);
    let _ = bvh.nearest([0.0; 3], 100.0, &|_| true);
    let _ = bvh.nearest([f32::NAN; 3], 1.0, &|_| true);
    let _ = bvh.occluded([0.0, 0.0, 1.0], [0.0, 0.0, -1.0], 10.0);
    bvh.query_box([-1.0; 3], [1.0; 3], |_| {});
}

#[test]
fn sweep_matches_rescan_on_fixed_meshes() {
    assert_same_layout(&[]);
    assert_same_layout(&[[[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]]);
    assert_same_layout(&plane(2.0, 4));
    assert_same_layout(&sphere([0.0; 3], 1.0, 16, 8));
    assert_same_layout(&torus(1.0, 0.35, 24, 8));
    assert_same_layout(&soup(300, 9));
    let zeros: Vec<Triangle> = (0..40)
        .map(|i| {
            let x = i as f32 * 0.25;
            [[x, -0.0, -0.0], [x + 0.2, 0.0, 0.1], [x + 0.05, 0.2, -0.0]]
        })
        .collect();
    assert_same_layout(&zeros);
    let mut degenerate = Vec::new();
    for i in 0..30 {
        let s = i as f32 * 0.37;
        degenerate.push([[s, 0.0, 0.0], [s, 0.0, 0.0], [s, 0.0, 0.0]]);
        degenerate.push([[s, 0.0, 0.0], [s + 1.0, 0.0, 0.0], [s + 2.0, 0.0, 0.0]]);
        degenerate.push([[s, 0.0, s * 0.2], [s + 1.0, 0.0, s * 0.2], [s, 0.0, s * 0.2]]);
    }
    assert_same_layout(&degenerate);
    let bvh = Bvh::new(&degenerate);
    assert_valid_tree(&bvh, &degenerate);
    for i in 0..8 {
        let a = i as f32;
        assert_closest(&bvh, &degenerate, [a.cos() * 3.0, 1.5, a.sin() * 3.0], normalize([-a.cos(), -0.4, -a.sin()]), 20.0);
    }
}

fn tri_strategy() -> impl Strategy<Value = Triangle> {
    proptest::array::uniform3(proptest::array::uniform3(-4.0f32..4.0))
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 48, ..ProptestConfig::default() })]

    #[test]
    fn sweep_layout_matches_rescan(tris in proptest::collection::vec(tri_strategy(), 0..48usize)) {
        assert_same_layout(&tris);
        assert_valid_tree(&Bvh::new(&tris), &tris);
    }

    #[test]
    fn closest_matches_linear_scan(tris in proptest::collection::vec(tri_strategy(), 0..40usize)) {
        let bvh = Bvh::new(&tris);
        assert_valid_tree(&bvh, &tris);
        for i in 0..8 {
            let a = i as f32 * 0.9 + 0.2;
            let o = [a.cos() * 3.0, (i as f32 * 0.4).sin() * 2.0, a.sin() * 3.0];
            let d = normalize([-(o[0]) + (i as f32 * 0.2).sin() * 0.3, -o[1], -o[2]]);
            assert_closest(&bvh, &tris, o, d, 1.0e6);
        }
        assert_closest(&bvh, &tris, [0.0, 0.0, 4.0], [0.0, 0.0, -1.0], 20.0);
    }
}

#[test]
#[ignore = "build time on 1M triangles, rescanning bins versus the prefix/suffix sweep (#134)"]
fn bench_build_one_million_triangles() {
    let tris = soup(1_000_000, 1);
    let t0 = Instant::now();
    let old = reference::build(&tris);
    let rescan = t0.elapsed();
    let t1 = Instant::now();
    let new = Bvh::new(&tris);
    let sweep = t1.elapsed();
    std::hint::black_box((old.nodes.len(), new.nodes().len()));
    let nodes: Vec<_> = new.nodes().collect();
    assert_eq!(old.nodes.len(), nodes.len());
    assert_eq!(old.order, new.triangles().1);
    eprintln!("build 1000000 triangles: rescan {rescan:.3?}, sweep {sweep:.3?}, speedup {:.2}x", rescan.as_secs_f64() / sweep.as_secs_f64());
}

#[test]
#[ignore = "closest-hit traversal, the same walk as the mesh BVH"]
fn bench_traversal_closest_hit() {
    let tris = soup(100_000, 2);
    let bvh = Bvh::new(&tris);
    let rays = 10_000u32;
    let t0 = Instant::now();
    let mut hits = 0u32;
    for i in 0..rays {
        let a = i as f32 * 0.731;
        let o = [a.cos() * 3.0, (i as f32 * 0.37).sin() * 2.0, a.sin() * 3.0];
        let d = normalize([-o[0], -o[1], -o[2]]);
        if bvh.closest(o, d, 1.0e9).is_some() {
            hits += 1;
        }
    }
    let dt = t0.elapsed();
    std::hint::black_box(hits);
    let ns = dt.as_secs_f64() * 1.0e9 / f64::from(rays);
    eprintln!("traversal: {rays} closest-hit rays on {} triangles in {dt:.3?} ({ns:.1} ns/ray, {hits} hits)", tris.len());
}
