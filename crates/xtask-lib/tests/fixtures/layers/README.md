# Layer fixtures

Captured once on 2026-11-03 with `cargo metadata --format-version 1 --no-deps --locked --offline`. Local root prefixes are redacted. Graphs larger than 200 KB are projected onto packages (name, id, manifest_path, dependencies) and workspace_members without removing edges. No synthetic edges replace real graphs. Policies are ported from each first-party `xtask/src/layers.rs`, not inferred from passing graphs.

| App | HEAD at capture (working-tree metadata) | Packages | JSON bytes |
|---|---|---:|---:|
| photocraft | `327c4cc82e737a3e0af7c147a477126d77525495` | 31 | 143954 |
| lightcraft | `06c1b9ae0775464ce83ef0a9588be770337600f1` | 33 | 112351 |
| texturecraft | `4edb8b86bc67f58922d6f16a6489059a78b61a3e` | 15 | 53956 |
| vectorcraft | `11862f61198c8f62c0b22b6c9a331cc5950ac98d` | 28 | 124471 |
| figcraft | `02c38a6d959f0afd82e44d9d67d396266e6ab4a3` | 22 | 80933 |
| designcraft | `e79d682e02dc66f14370958b193bfa46072b2f41` | 21 | 78559 |
| deckcraft | `3e178d8f5a59c787a6a53a7ed0f052b3e4a1489f` | 28 | 68306 |
| cadcraft | `f6d84e28f31ff98224a2b63b04e5153f4899ae2b` | 16 | 64033 |
| wordcraft | `b47079fc791595fd84bb499d64dcd3f0624ef35a` | 19 | 71576 |
| pdfcraft | `3936f93a1479366a951206fc6d3094680fe2d3dc` | 37 | 119266 |
| gridcraft | `85e51e009c24fad47575f04d3c08f12342615d82` | 17 | 48312 |
| materialcraft | `613d603b8fc5f0805f70fe94f8797db9cff3001a` | 14 | 36765 |
