//! Side-by-side PNGs (reference | ours | difference) of the worst files per app.

use crate::run::{Row, raster};
use std::path::Path;

fn score(r: &Row) -> f64 {
    0.5 * r.word_f1 + 0.5 * r.ssim_mean
}

pub fn run(results: &str, work: &str, out: &str, n: usize) -> Result<(), String> {
    let rows: Vec<Row> = std::fs::read_to_string(results).map_err(|e| e.to_string())?.lines().filter_map(|l| serde_json::from_str(l).ok()).collect();
    std::fs::create_dir_all(out).map_err(|e| e.to_string())?;
    let mut index = Vec::new();
    for app in ["wordcraft", "gridcraft", "deckcraft"] {
        let mut ok: Vec<&Row> = rows.iter().filter(|r| r.app == app && r.status == "ok").collect();
        ok.sort_by(|a, b| score(a).total_cmp(&score(b)));
        for (rank, r) in ok.iter().take(n).enumerate() {
            let dir = Path::new(work).join(&r.id);
            let pg = r.worst_page + 1;
            let a = raster(&dir.join("ref.pdf"), &dir, "tri-ref", pg, pg)?;
            let b = raster(&dir.join("ours.pdf"), &dir, "tri-ours", pg, pg)?;
            let (Some(a), Some(b)) = (a.first(), b.first()) else { continue };
            let (w, h) = (a.width.max(b.width), a.height.max(b.height));
            let gap = 8;
            let (tw, mut buf) = (w * 3 + gap * 2, Vec::new());
            buf.resize(tw * h * 3, 200u8);
            let px = |im: &craft_compare::image::Image, x: usize, y: usize| -> [u8; 3] {
                if x < im.width && y < im.height {
                    let i = (y * im.width + x) * 4;
                    [0, 1, 2].map(|c| (im.data[i + c] * 255.0).round() as u8)
                } else {
                    [255, 255, 255]
                }
            };
            for y in 0..h {
                for x in 0..w {
                    let (pa, pb) = (px(a, x, y), px(b, x, y));
                    let d = (0..3).map(|c| pa[c].abs_diff(pb[c])).max().unwrap();
                    let dv = 255 - d.saturating_mul(2).min(255);
                    for (k, p) in [pa, pb, [dv, dv, dv]].iter().enumerate() {
                        let o = (y * tw + k * (w + gap) + x) * 3;
                        buf[o..o + 3].copy_from_slice(p);
                    }
                }
            }
            let stem: String =
                r.rel.rsplit('/').next().unwrap_or("x").chars().map(|c| if c.is_ascii_alphanumeric() || c == '.' { c } else { '_' }).collect();
            let name = format!("{app}-{:02}-{}-p{pg}.png", rank + 1, stem);
            let f = std::fs::File::create(Path::new(out).join(&name)).map_err(|e| e.to_string())?;
            let mut enc = png::Encoder::new(std::io::BufWriter::new(f), tw as u32, h as u32);
            enc.set_color(png::ColorType::Rgb);
            enc.set_depth(png::BitDepth::Eight);
            enc.write_header().map_err(|e| e.to_string())?.write_image_data(&buf).map_err(|e| e.to_string())?;
            index.push(format!("{name}\t{}\tf1={:.3}\tssim={:.3}\tpages={}/{}\t{}", r.id, r.word_f1, r.ssim_mean, r.pages_ref, r.pages_ours, r.rel));
        }
    }
    std::fs::write(Path::new(out).join("index.tsv"), index.join("\n") + "\n").map_err(|e| e.to_string())?;
    eprintln!("wrote {} triage PNGs to {out}", index.len());
    Ok(())
}
