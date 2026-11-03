# craft-bvh

A binned surface-area-heuristic bounding volume hierarchy over a mesh's triangles:
closest-hit and any-hit rays, nearest-point queries and box overlap. Leaf size 4,
12 bins along the widest centroid axis. No dependencies, no `unsafe`, and it builds
for `wasm32-unknown-unknown`.

MaterialCraft and TextureCraft each carried this tree. Their `Vec3` and `Aabb` types
stay in the apps. This crate takes positions as `[f32; 3]`. Convert at the boundary.

```toml
craft-bvh = { git = "https://github.com/storytold/craft-libs", rev = "<commit>" }
```

## Use

```rust
use craft_bvh::Bvh;

let tris = [[[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]];
let bvh = Bvh::new(&tris);
let hit = bvh.closest([0.2, 0.2, 1.0], [0.0, 0.0, -1.0], 10.0);
assert!(hit.is_some());
```

`Bvh::nodes` is the flattened tree (root first) for a GPU upload: bounds, then for a
leaf the range of `Bvh::triangles`, and for an inner node the children `first` and
`first + 1`.

Split selection sweeps prefix and suffix bin bounds once per node, so each candidate
is O(1). The rescanning builder folded both sides for every split.
