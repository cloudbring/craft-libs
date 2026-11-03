//! First-party oracle: runs WordCraft crates/metafile/src/lib.rs unchanged; see ../fixtures/README.md.
use kurbo::{BezPath, Shape};
use wordcraft_metafile_oracle::{Item, Seg};

fn main() {
    for file in std::env::args().skip(1) {
        let source = std::fs::read_to_string(&file).unwrap();
        let hex: String = source.split_whitespace().collect();
        let bytes: Vec<u8> = hex.as_bytes().as_chunks::<2>().0.iter().map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap()).collect();
        let picture = wordcraft_metafile_oracle::parse(&bytes).unwrap();
        let mut bounds: Option<kurbo::Rect> = None;
        for item in &picture.items {
            if let Item::Path { segs, .. } = item {
                let mut path = BezPath::new();
                for segment in segs {
                    match *segment {
                        Seg::Move(x,y) => path.move_to((f64::from(x),f64::from(y))),
                        Seg::Line(x,y) => path.line_to((f64::from(x),f64::from(y))),
                        Seg::Cubic(x1,y1,x2,y2,x3,y3) => path.curve_to((f64::from(x1),f64::from(y1)),(f64::from(x2),f64::from(y2)),(f64::from(x3),f64::from(y3))),
                        Seg::Close => path.close_path(),
                    }
                }
                let next = path.bounding_box();
                bounds = Some(bounds.map_or(next, |current| current.union(next)));
            }
        }
        println!("{file}: {bounds:?}; size=({},{}); items={}",picture.width,picture.height,picture.items.len());
    }
}
