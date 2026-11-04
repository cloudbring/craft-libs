//! Per-call cost of vectorcraft's `Archive::read` before and after the swap: the new adapter reopens
//! (and re-validates) the whole archive on every read. Prints median microseconds over 5 runs.
use std::time::Instant;
fn med(mut v: Vec<f64>) -> f64 {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[v.len() / 2]
}
fn main() {
    for n in [10usize, 100, 1000, 10000, 65534] {
        let files: Vec<(String, Vec<u8>)> = (0..n).map(|i| (format!("dir/{i}.xml"), format!("<a>{i}</a>").repeat(50).into_bytes())).collect();
        let zip = container_oracle::old_vectorcraft_zip::deflate(&files, (0, 0x21), || false).unwrap();
        let names = [format!("dir/0.xml"), format!("dir/{}.xml", n / 2), format!("dir/{}.xml", n - 1)];
        let (mut o, mut w) = (vec![], vec![]);
        for _ in 0..5 {
            let t = Instant::now();
            let a = container_oracle::old_vectorcraft_zip::Archive::open(&zip, 70000).unwrap();
            for name in &names {
                a.read(name, 1 << 20).unwrap();
            }
            o.push(t.elapsed().as_secs_f64() * 1e6);
            let t = Instant::now();
            let a = container_oracle::new_vectorcraft_zip::Archive::open(&zip, 70000).unwrap();
            for name in &names {
                a.read(name, 1 << 20).unwrap();
            }
            w.push(t.elapsed().as_secs_f64() * 1e6);
        }
        println!("entries {n:>6}: open + 3 reads  old {:>10.0} us   new {:>10.0} us   ratio {:.1}x", med(o.clone()), med(w.clone()), med(w) / med(o));
    }
}
