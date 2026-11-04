use std::sync::Arc;
// Compare one Annex-B stream across the old fork, the pre-fix crate, the new crate and each
// "new with one file reverted to pre-fix" variant. Prints a hash line per side.
fn au_split(data: &[u8]) -> Vec<&[u8]> {
    let mut starts = Vec::new();
    let mut i = 0;
    while i + 3 <= data.len() {
        if data[i] == 0 && data[i + 1] == 0 && data[i + 2] == 1 { let s = if i > 0 && data[i - 1] == 0 { i - 1 } else { i }; starts.push((s, i + 3)); i += 3; } else { i += 1; }
    }
    let mut aus = Vec::new(); let mut au_start = 0usize; let mut seen = false;
    for &(sc, p) in &starts {
        if p >= data.len() { continue; }
        let t = data[p] & 0x1f; let is_slice = t == 1 || t == 5;
        let fm = is_slice && p + 1 < data.len() && data[p + 1] & 0x80 != 0;
        if seen && (matches!(t, 6..=9) || fm) { aus.push(&data[au_start..sc]); au_start = sc; seen = false; }
        if is_slice { seen = true; }
    }
    if au_start < data.len() { aus.push(&data[au_start..]); }
    aus
}
macro_rules! run { ($k:ident, $data:expr) => {{
    use std::hash::{Hash, Hasher};
    let r = std::panic::catch_unwind(|| {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        let mut dec = $k::Decoder::with_threads(1);
        let mut n = 0; let mut end = String::from("ok");
        for (i, au) in au_split($data).into_iter().enumerate() {
            match dec.decode(au, i as i64) { Ok(p) => for p in p { n += 1; p.y.hash(&mut h); p.u.hash(&mut h); p.v.hash(&mut h); p.pts.hash(&mut h); }, Err(e) => { end = format!("err@{i}: {e}"); break; } }
        }
        if end == "ok" { for p in dec.flush() { n += 1; p.y.hash(&mut h); p.u.hash(&mut h); p.v.hash(&mut h); p.pts.hash(&mut h); } }
        format!("{end} frames={n} hash={:016x}", h.finish())
    });
    r.unwrap_or_else(|p| format!("PANIC {}", p.downcast_ref::<String>().cloned().or_else(|| p.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default()))
}}; }
fn main() {
    std::panic::set_hook(Box::new(|_| {}));
    let _ = Arc::new(0);
    for path in std::env::args().skip(1) {
        let (file, edit) = path.split_once('#').map(|(a, b)| (a.to_string(), Some(b.to_string()))).unwrap_or((path.clone(), None));
        let mut data = std::fs::read(&file).unwrap();
        if let Some(e) = edit { // flip@POS.BIT or setXX@POS
            if let Some(r) = e.strip_prefix("sweep-flip@") { let (p, b) = r.split_once('.').unwrap(); data[p.parse::<usize>().unwrap()] ^= 1 << b.parse::<u32>().unwrap(); }
            else if let Some(r) = e.strip_prefix("sweep-set") { let (v, p) = r.split_once('@').unwrap(); data[p.parse::<usize>().unwrap()] = u8::from_str_radix(v, 16).unwrap(); }
        }
        println!("== {path}");
        println!("old        {}", run!(old, &data));
        println!("pre        {}", run!(pre, &data));
        println!("new        {}", run!(v_none, &data));
        println!("new-decoder.rs->pre   {}", run!(v_decoder, &data));
        println!("new-dpb.rs->pre       {}", run!(v_dpb, &data));
        println!("new-picture.rs->pre   {}", run!(v_picture, &data));
        println!("new-slice.rs->pre     {}", run!(v_slice, &data));
        println!("new-slicedec.rs->pre  {}", run!(v_slicedec, &data));
        println!("new-transform.rs->pre {}", run!(v_transform, &data));
    }
}
