use craft_metafile::{Error, Item, Kind, Limits, Picture, Seg, is_metafile, parse, parse_with_limits, sniff};
use kurbo::{BezPath, Rect, Shape};

struct Fixture {
    name: &'static str,
    hex: &'static str,
    wordcraft_bounds: Option<[f64; 4]>,
}

const FIXTURES: &[Fixture] = &[
    Fixture { name: "rectangle.wmf", hex: include_str!("fixtures/rectangle.wmf.hex"), wordcraft_bounds: Some([0.0, 0.0, 100.0, 50.0]) },
    Fixture { name: "placeable.wmf", hex: include_str!("fixtures/placeable.wmf.hex"), wordcraft_bounds: Some([0.5, 1.0, 5.5, 3.5]) },
    Fixture { name: "mapped.wmf", hex: include_str!("fixtures/mapped.wmf.hex"), wordcraft_bounds: Some([10.0, 20.0, 110.0, 70.0]) },
    Fixture {
        name: "rectangle.emf",
        hex: include_str!("fixtures/rectangle.emf.hex"),
        wordcraft_bounds: Some([7.08661413192749, 14.17322826385498, 77.9527587890625, 49.606300354003906]),
    },
    Fixture {
        name: "bezier.emf",
        hex: include_str!("fixtures/bezier.emf.hex"),
        wordcraft_bounds: Some([7.08661413192749, 7.08661413192749, 134.6456756591797, 49.606297850608826]),
    },
    Fixture {
        name: "ellipse.emf",
        hex: include_str!("fixtures/ellipse.emf.hex"),
        wordcraft_bounds: Some([14.17322826385498, 7.08661413192749, 127.55905151367188, 63.77952575683594]),
    },
    Fixture {
        name: "polypolygon.emf",
        hex: include_str!("fixtures/polypolygon.emf.hex"),
        wordcraft_bounds: Some([7.08661413192749, 7.08661413192749, 127.55905151367188, 56.69291305541992]),
    },
    Fixture { name: "plus.emf", hex: include_str!("fixtures/plus.emf.hex"), wordcraft_bounds: None },
];

fn decode(hex: &str) -> Vec<u8> {
    let digits: String = hex.split_whitespace().collect();
    digits.as_bytes().as_chunks::<2>().0.iter().map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap()).collect()
}

fn bounds(picture: &Picture) -> Option<[f64; 4]> {
    let mut bounds: Option<Rect> = None;
    for item in &picture.items {
        if let Item::Path { segs, .. } | Item::Pattern { segs, .. } = item {
            let mut path = BezPath::new();
            for segment in segs {
                match *segment {
                    Seg::Move(x, y) => path.move_to((f64::from(x), f64::from(y))),
                    Seg::Line(x, y) => path.line_to((f64::from(x), f64::from(y))),
                    Seg::Cubic(x1, y1, x2, y2, x3, y3) => {
                        path.curve_to((f64::from(x1), f64::from(y1)), (f64::from(x2), f64::from(y2)), (f64::from(x3), f64::from(y3)))
                    }
                    Seg::Close => path.close_path(),
                }
            }
            let next = path.bounding_box();
            bounds = Some(bounds.map_or(next, |current| current.union(next)));
        }
    }
    bounds.map(|rect| [rect.x0, rect.y0, rect.x1, rect.y1])
}

#[test]
fn path_bounds_match_recorded_wordcraft_oracle() {
    for fixture in FIXTURES {
        let picture = parse(&decode(fixture.hex)).unwrap();
        let actual = bounds(&picture).unwrap();
        if let Some(expected) = fixture.wordcraft_bounds {
            for (actual, expected) in actual.into_iter().zip(expected) {
                assert!((actual - expected).abs() < 0.0001, "{}: {actual} != {expected}", fixture.name);
            }
        } else {
            assert_eq!(fixture.name, "plus.emf");
            assert_eq!(actual, [0.0, 0.0, 30.0, 40.0]);
        }
    }
}

#[test]
fn fixture_mutations_never_panic() {
    let mut seed = 0x92e0_2026_1103_f00du64;
    for fixture in FIXTURES {
        let original = decode(fixture.hex);
        for iteration in 0..2_000 {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            let mut bytes = original.clone();
            let index = seed as usize % bytes.len();
            if iteration % 2 == 0 {
                bytes.truncate(index);
            } else {
                bytes[index] ^= 1 << (seed % 8);
                let end = (index + 4).min(bytes.len());
                if iteration % 5 == 1 {
                    bytes[index..end].fill(0xff);
                }
            }
            let result = std::panic::catch_unwind(|| {
                let _ = parse(&bytes);
                let _ = parse_with_limits(&bytes, Limits { max_records: 8, max_items: 2, max_bitmap_pixels: 16, max_bytes: 8_192 });
            });
            assert!(result.is_ok(), "{} mutation {iteration} seed {seed:#x}", fixture.name);
        }
    }
}

#[test]
fn sniff_distinguishes_containers_without_validation() {
    for (index, expected) in [(0, Kind::Wmf), (1, Kind::PlaceableWmf), (3, Kind::Emf), (7, Kind::Emf)] {
        let bytes = decode(FIXTURES[index].hex);
        assert_eq!(sniff(&bytes), Some(expected));
        assert!(is_metafile(&bytes));
    }
    assert_eq!(sniff(b"not a metafile"), None);
    assert!(!is_metafile(&[]));
}

#[test]
fn resource_limits_fail_closed() {
    let bytes = decode(FIXTURES[3].hex);
    for limits in [
        Limits { max_records: 0, ..Limits::default() },
        Limits { max_items: 0, ..Limits::default() },
        Limits { max_bytes: bytes.len() - 1, ..Limits::default() },
        Limits { max_bytes: bytes.len(), ..Limits::default() },
    ] {
        assert_eq!(parse_with_limits(&bytes, limits), Err(Error::LimitExceeded));
    }
    let plus = decode(FIXTURES[7].hex);
    assert_eq!(parse_with_limits(&plus, Limits { max_records: 2, ..Limits::default() }), Err(Error::LimitExceeded));
}
