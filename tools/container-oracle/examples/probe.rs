//! Debug helper: `probe <scenario> <file>` prints what the old and the new side saw for one input
//! (byte arrays are collapsed), then the judge's verdict.
use container_oracle::cfbside::*;
use container_oracle::docside;
use container_oracle::zipside::*;

fn collapse(s: &str) -> String {
    let mut out = String::new();
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'[' {
            let mut j = i + 1;
            while j < b.len() && (b[j].is_ascii_digit() || b[j] == b',' || b[j] == b' ') {
                j += 1;
            }
            if j < b.len() && b[j] == b']' && j - i > 40 {
                out.push_str(&format!("[..{} bytes..]", s[i..j].matches(',').count() + 1));
                i = j + 1;
                continue;
            }
        }
        out.push(b[i] as char);
        i += 1;
    }
    out
}

macro_rules! show {
    ($old:expr, $new:expr, $judge:expr, $bytes:expr) => {{
        let (o, n) = ($old($bytes), $new($bytes));
        println!("OLD: {}\nNEW: {}", collapse(&format!("{o:?}")), collapse(&format!("{n:?}")));
        if let (Ok(o), Ok(n)) = (&o, &n) {
            println!("VERDICT: {:?}", $judge(o, n));
        }
    }};
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let bytes = if let Some(case) = a[2].strip_prefix("hostile:") {
        let mut all = container_oracle::hostile::zip_cases(&container_oracle::hostile::base_zip(), false);
        all.extend(container_oracle::hostile::zip_cases(&container_oracle::hostile::pcraft_base(), false).into_iter().map(|(n, b)| (format!("pc/{n}"), b)));
        all.extend(container_oracle::hostile::cfb_cases());
        all.into_iter().find(|(n, _)| n == case).map(|(_, b)| b).expect("no such hostile case")
    } else {
        std::fs::read(&a[2]).expect("read")
    };
    match a[1].as_str() {
        "vectorcraft-read" => {
            show!(vectorcraft_read::old, vectorcraft_read::new, judge_vc_l, &bytes);
            if let (Ok(Ok(o)), Ok(Ok(n))) = (vectorcraft_read::old(&bytes), vectorcraft_read::new(&bytes)) {
                for (a, b) in o.entries.iter().zip(&n.entries) {
                    if a != b {
                        let f = |r: &Result<Option<Vec<u8>>, String>| match r {
                            Ok(Some(v)) => format!("Some({} bytes) {:?}", v.len(), String::from_utf8_lossy(&v[..v.len().min(70)])),
                            other => format!("{other:?}"),
                        };
                        println!("DIFF {:?}: old {} | new {}", a.0, f(&a.3), f(&b.3));
                        if let (Ok(Some(x)), Ok(Some(y))) = (&a.3, &b.3) {
                            let at = x.iter().zip(y).position(|(p, q)| p != q);
                            if let Some(at) = at {
                                let w = |v: &Vec<u8>| String::from_utf8_lossy(&v[at.saturating_sub(20)..(at + 40).min(v.len())]).into_owned();
                                println!("  first diff at {at}: old {:?} | new {:?}", w(x), w(y));
                            }
                            if let Ok(dir) = std::env::var("PROBE_DUMP") {
                                let _ = std::fs::write(format!("{dir}/old-{}", a.0.replace('/', "_")), x);
                                let _ = std::fs::write(format!("{dir}/new-{}", a.0.replace('/', "_")), y);
                            }
                        }
                    }
                }
            }
        }
        "effectcraft-read" => show!(effectcraft_read::old, effectcraft_read::new, judge_entries_l, &bytes),
        "vectorcraft-write-deflate" => show!(vectorcraft_write::old_deflate, vectorcraft_write::new_deflate, judge_write_l, &bytes),
        "craftdoc" => show!(docside::old::run, docside::new::run, |o: &_, n: &_| docside::judge(o, n), &bytes),
        "gridcraft-streams" => show!(gridcraft::streams_old, gridcraft::streams_new, judge_streams_l, &bytes),
        "gridcraft-is-xls" => show!(gridcraft::is_xls_old, gridcraft::is_xls_new, |o: &bool, n: &bool| judge_bool(o, n), &bytes),
        "wordcraft-streams" => show!(wordcraft::streams_old, wordcraft::streams_new, judge_streams_l, &bytes),
        "wordcraft-sniff" => show!(wordcraft::sniff_old, wordcraft::sniff_new, |o: &_, n: &_| judge_bool(o, n), &bytes),
        "filmcraft-tree" => show!(filmcraft::old, filmcraft::new, judge_film_l, &bytes),
        "reference" => show!(tree_reference, tree_craft_cfb, judge_tree_l, &bytes),
        other => eprintln!("unknown scenario {other}"),
    }
}
