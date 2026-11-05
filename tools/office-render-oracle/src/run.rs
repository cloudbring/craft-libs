//! Render one file with LibreOffice and with our app, then compare the two PDFs.

use crate::Entry;
use craft_compare::image::{Image, compare_images};
use craft_compare::layout::{TextLine, compare_layout};
use craft_compare::text::{reading_order_score, word_f1};
use serde::{Deserialize, Serialize};
use std::io::Read;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

pub const DPI: &str = "72";
/// Pages rasterised and layout-compared per file (page counts are always over the whole PDF).
pub const MAX_PAGES: usize = 10;

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Row {
    pub id: String,
    pub app: String,
    pub source: String,
    pub rel: String,
    pub ext: String,
    pub size: u64,
    pub tags: Vec<String>,
    pub stratum: String,
    /// ok | ref_fail | ours_fail | both_fail
    pub status: String,
    pub ref_err: Option<String>,
    pub ours_err: Option<String>,
    pub ref_ms: u64,
    pub ours_ms: u64,
    pub pages_ref: usize,
    pub pages_ours: usize,
    pub compared_pages: usize,
    pub ssim_mean: f64,
    pub ssim_min: f64,
    pub worst_page: usize,
    pub de_mean: f64,
    pub p99_mean: f64,
    pub p99_max: f64,
    pub size_match_frac: f64,
    pub word_f1: f64,
    pub order: f64,
    pub ref_words: usize,
    pub ours_words: usize,
    pub layout_pct: f64,
    pub dy_median: f64,
    pub page_start_frac: f64,
}

enum Out {
    Done(i32),
    Timeout,
    Spawn(String),
}

/// Run with a wall-clock limit; the child gets its own process group, which is killed afterwards
/// (soffice leaves `soffice.bin` behind when it is killed or crashes).
fn run_cmd(mut cmd: Command, limit: Duration) -> Out {
    cmd.stdin(Stdio::null()).process_group(0);
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => return Out::Spawn(e.to_string()),
    };
    let pgid = child.id();
    let start = Instant::now();
    let out = loop {
        match child.try_wait() {
            Ok(Some(st)) => break Out::Done(st.code().unwrap_or(-1)),
            Ok(None) if start.elapsed() > limit => break Out::Timeout,
            Ok(None) => std::thread::sleep(Duration::from_millis(40)),
            Err(e) => break Out::Spawn(e.to_string()),
        }
    };
    let _ = Command::new("kill").args(["-KILL", &format!("-{pgid}")]).stderr(Stdio::null()).status();
    let _ = child.wait();
    out
}

fn read_ppm(p: &Path) -> Option<Image> {
    let mut b = Vec::new();
    std::fs::File::open(p).ok()?.read_to_end(&mut b).ok()?;
    let mut pos = 0;
    let mut tok = || {
        while pos < b.len() && b[pos].is_ascii_whitespace() {
            pos += 1;
        }
        let s = pos;
        while pos < b.len() && !b[pos].is_ascii_whitespace() {
            pos += 1;
        }
        std::str::from_utf8(&b[s..pos]).ok().map(String::from)
    };
    if tok()? != "P6" {
        return None;
    }
    let (w, h): (usize, usize) = (tok()?.parse().ok()?, tok()?.parse().ok()?);
    tok()?;
    let data = &b[pos + 1..];
    if data.len() < w * h * 3 {
        return None;
    }
    let mut rgba = Vec::with_capacity(w * h * 4);
    for px in data[..w * h * 3].chunks_exact(3) {
        rgba.extend_from_slice(&[px[0], px[1], px[2], 255]);
    }
    Image::from_rgba8(w, h, &rgba).ok()
}

/// Rasterise the first [`MAX_PAGES`] pages at [`DPI`]; `from` is 1-based.
pub fn raster(pdf: &Path, dir: &Path, tag: &str, from: usize, to: usize) -> Result<Vec<Image>, String> {
    let prefix = dir.join(format!("{tag}-r"));
    let st = Command::new("pdftoppm")
        .args(["-r", DPI, "-f", &from.to_string(), "-l", &to.to_string()])
        .arg(pdf)
        .arg(&prefix)
        .stderr(Stdio::null())
        .status()
        .map_err(|e| e.to_string())?;
    if !st.success() {
        return Err(format!("pdftoppm exit {:?}", st.code()));
    }
    let stem = format!("{tag}-r-");
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|e| e.to_string())?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with(&stem) && n.ends_with(".ppm")))
        .collect();
    files.sort();
    let imgs: Vec<Image> = files.iter().filter_map(|f| read_ppm(f)).collect();
    for f in files {
        let _ = std::fs::remove_file(f);
    }
    Ok(imgs)
}

fn pad(img: &Image, w: usize, h: usize) -> Image {
    if img.width == w && img.height == h {
        return img.clone();
    }
    let mut data = vec![1.0f32; w * h * 4];
    for y in 0..img.height.min(h) {
        for x in 0..img.width.min(w) {
            let (s, d) = ((y * img.width + x) * 4, (y * w + x) * 4);
            data[d..d + 4].copy_from_slice(&img.data[s..s + 4]);
        }
    }
    Image::new(w, h, 4, data).unwrap()
}

fn unescape(s: &str) -> String {
    s.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&#39;", "'").replace("&amp;", "&")
}

fn attr(tag: &str, name: &str) -> Option<f64> {
    let k = format!("{name}=\"");
    let s = tag.find(&k)? + k.len();
    tag[s..].split('"').next()?.parse().ok()
}

/// (page count, text lines of the first [`MAX_PAGES`] pages) from `pdftotext -bbox-layout`.
fn bbox_lines(pdf: &Path, out: &Path) -> Result<(usize, Vec<TextLine>), String> {
    let st = Command::new("pdftotext").arg("-bbox-layout").arg(pdf).arg(out).stderr(Stdio::null()).status().map_err(|e| e.to_string())?;
    if !st.success() {
        return Err(format!("pdftotext exit {:?}", st.code()));
    }
    let html = std::fs::read_to_string(out).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(out);
    let (mut pages, mut lines) = (0usize, Vec::new());
    let mut cur: Option<(f64, f64, Vec<String>)> = None;
    for l in html.lines() {
        let t = l.trim_start();
        if t.starts_with("<page ") {
            pages += 1;
        } else if t.starts_with("<line ") {
            cur = Some((attr(t, "xMin").unwrap_or(0.0), attr(t, "yMin").unwrap_or(0.0), Vec::new()));
        } else if t.starts_with("<word ") {
            if let (Some(c), Some(a), Some(b)) = (cur.as_mut(), t.find('>'), t.rfind("</word>")) {
                c.2.push(unescape(&t[a + 1..b]));
            }
        } else if t.starts_with("</line>") {
            if let Some((x, y, w)) = cur.take() {
                if pages <= MAX_PAGES && lines.len() < 4000 {
                    lines.push(TextLine::new(pages - 1, x, y, w.join(" ")));
                }
            }
        }
    }
    Ok((pages, lines))
}

fn plain_text(pdf: &Path, out: &Path) -> Result<String, String> {
    let st = Command::new("pdftotext").arg("-layout").arg(pdf).arg(out).stderr(Stdio::null()).status().map_err(|e| e.to_string())?;
    if !st.success() {
        return Err(format!("pdftotext exit {:?}", st.code()));
    }
    let t = String::from_utf8_lossy(&std::fs::read(out).map_err(|e| e.to_string())?).into_owned();
    let _ = std::fs::remove_file(out);
    Ok(t)
}

fn render_ref(input: &Path, dir: &Path, limit: u64) -> Result<(PathBuf, u64), String> {
    let t0 = Instant::now();
    let profile = std::env::var("LO_PROFILE").unwrap_or_else(|_| format!("{}/lo-profile", dir.parent().unwrap().display()));
    let mut c = Command::new("soffice");
    c.arg(format!("-env:UserInstallation=file://{profile}"))
        .args(["--headless", "--norestore", "--nolockcheck", "--convert-to", "pdf", "--outdir"])
        .arg(dir)
        .arg(input)
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let r = run_cmd(c, Duration::from_secs(limit));
    let ms = t0.elapsed().as_millis() as u64;
    let out = dir.join("input.pdf");
    match r {
        Out::Timeout => Err(format!("timeout {limit}s")),
        Out::Spawn(e) => Err(format!("spawn: {e}")),
        Out::Done(code) if !out.exists() => Err(format!("no output (exit {code})")),
        Out::Done(_) => {
            let to = dir.join("ref.pdf");
            std::fs::rename(&out, &to).map_err(|e| e.to_string())?;
            Ok((to, ms))
        }
    }
}

fn render_ours(app: &str, bin_dir: &str, input: &Path, dir: &Path, limit: u64) -> Result<(PathBuf, u64), String> {
    let t0 = Instant::now();
    let out = dir.join("ours.pdf");
    let errf = dir.join("ours.err");
    let mut c = Command::new(Path::new(bin_dir).join(format!("{app}-cli")));
    c.arg("convert").arg(input).arg(&out).stdout(Stdio::null()).stderr(std::fs::File::create(&errf).map_err(|e| e.to_string())?);
    let r = run_cmd(c, Duration::from_secs(limit));
    let ms = t0.elapsed().as_millis() as u64;
    let tail = || {
        std::fs::read_to_string(&errf)
            .unwrap_or_default()
            .lines()
            .rev()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("")
            .chars()
            .take(160)
            .collect::<String>()
    };
    let _ = std::fs::remove_file(&errf);
    match r {
        Out::Timeout => Err(format!("timeout {limit}s")),
        Out::Spawn(e) => Err(format!("spawn: {e}")),
        Out::Done(code) if code != 0 || !out.exists() => Err(format!("exit {code}: {}", tail())),
        Out::Done(_) => Ok((out, ms)),
    }
}

fn median(v: &mut [f64]) -> f64 {
    if v.is_empty() {
        return 0.0;
    }
    v.sort_by(f64::total_cmp);
    v[v.len() / 2]
}

fn compare(row: &mut Row, dir: &Path, rpdf: &Path, opdf: &Path) -> Result<(), String> {
    let (rt, ot) = (plain_text(rpdf, &dir.join("r.txt"))?, plain_text(opdf, &dir.join("o.txt"))?);
    row.ref_words = craft_compare::text::words(&rt).len();
    row.ours_words = craft_compare::text::words(&ot).len();
    row.word_f1 = word_f1(&rt, &ot);
    row.order = reading_order_score(&rt, &ot);
    let ((pr, lr), (po, lo)) = (bbox_lines(rpdf, &dir.join("r.html"))?, bbox_lines(opdf, &dir.join("o.html"))?);
    row.pages_ref = pr;
    row.pages_ours = po;
    let lay = compare_layout(&lr, &lo);
    row.layout_pct = lay.same_break_pct;
    let mut dys: Vec<f64> = lay.pages.iter().filter_map(|p| p.median_dy).collect();
    row.dy_median = median(&mut dys);
    let starts = lay.pages.iter().filter(|p| p.page_start_same.is_some()).count();
    row.page_start_frac = if starts == 0 { 1.0 } else { lay.page_starts_same as f64 / starts as f64 };
    let n = pr.min(po).min(MAX_PAGES);
    row.compared_pages = n;
    if n == 0 {
        return Ok(());
    }
    let (ri, oi) = (raster(rpdf, dir, "ref", 1, n)?, raster(opdf, dir, "ours", 1, n)?);
    let n = ri.len().min(oi.len());
    let (mut ssim, mut de, mut p99, mut same) = (Vec::new(), Vec::new(), Vec::new(), 0usize);
    for i in 0..n {
        let (w, h) = (ri[i].width.max(oi[i].width), ri[i].height.max(oi[i].height));
        same += usize::from(ri[i].width.abs_diff(oi[i].width) <= 2 && ri[i].height.abs_diff(oi[i].height) <= 2);
        let rep = compare_images(&pad(&ri[i], w, h), &pad(&oi[i], w, h), 0.1).map_err(|e| format!("{e:?}"))?;
        ssim.push(rep.ssim);
        de.push(rep.delta_e_mean);
        p99.push(rep.p99_abs);
    }
    if n > 0 {
        row.ssim_mean = ssim.iter().sum::<f64>() / n as f64;
        row.ssim_min = ssim.iter().cloned().fold(1.0, f64::min);
        row.worst_page = ssim.iter().enumerate().min_by(|a, b| a.1.total_cmp(b.1)).map_or(0, |(i, _)| i);
        row.de_mean = de.iter().sum::<f64>() / n as f64;
        row.p99_mean = p99.iter().sum::<f64>() / n as f64;
        row.p99_max = p99.iter().cloned().fold(0.0, f64::max);
        row.size_match_frac = same as f64 / n as f64;
    }
    Ok(())
}

pub fn process(e: &Entry, work: &str, bin_dir: &str, limit: u64) -> Row {
    let dir = Path::new(work).join(&e.id);
    let _ = std::fs::remove_dir_all(&dir);
    let mut row = Row {
        id: e.id.clone(),
        app: e.app.clone(),
        source: e.source.clone(),
        rel: e.rel.clone(),
        ext: e.ext.clone(),
        size: e.size,
        tags: e.tags.clone(),
        stratum: e.stratum.clone(),
        ..Default::default()
    };
    if std::fs::create_dir_all(&dir).is_err() {
        row.status = "both_fail".into();
        return row;
    }
    let input = dir.join(format!("input.{}", e.ext));
    if let Err(err) = std::fs::copy(&e.path, &input) {
        row.status = "both_fail".into();
        row.ref_err = Some(format!("copy: {err}"));
        return row;
    }
    let r = render_ref(&input, &dir, limit);
    let o = render_ours(&e.app, bin_dir, &input, &dir, limit.min(60));
    let _ = std::fs::remove_file(&input);
    let (rp, op) = match (r, o) {
        (Ok((rp, rms)), Ok((op, oms))) => {
            row.ref_ms = rms;
            row.ours_ms = oms;
            (rp, op)
        }
        (r, o) => {
            row.ref_err = r.as_ref().err().cloned();
            row.ours_err = o.as_ref().err().cloned();
            row.status = match (r.is_err(), o.is_err()) {
                (true, true) => "both_fail",
                (true, false) => "ref_fail",
                _ => "ours_fail",
            }
            .into();
            return row;
        }
    };
    match compare(&mut row, &dir, &rp, &op) {
        Ok(()) => row.status = "ok".into(),
        Err(err) => {
            row.status = "both_fail".into();
            row.ref_err = Some(format!("compare: {err}"));
        }
    }
    row
}

pub fn run_all(sample: &str, work: &str, bin_dir: &str, out: &str, limit: u64) -> Result<(), String> {
    let entries: Vec<Entry> = serde_json::from_str(&std::fs::read_to_string(sample).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(work).map_err(|e| e.to_string())?;
    let done: std::collections::HashSet<String> =
        std::fs::read_to_string(out).unwrap_or_default().lines().filter_map(|l| serde_json::from_str::<Row>(l).ok()).map(|r| r.id).collect();
    let mut f = std::fs::OpenOptions::new().create(true).append(true).open(out).map_err(|e| e.to_string())?;
    for (i, e) in entries.iter().enumerate() {
        if done.contains(&e.id) {
            continue;
        }
        let row = process(e, work, bin_dir, limit);
        eprintln!(
            "[{}/{}] {} {} {} f1={:.2} ssim={:.2}",
            i + 1,
            entries.len(),
            row.id,
            row.status,
            row.rel.rsplit('/').next().unwrap_or(""),
            row.word_f1,
            row.ssim_mean
        );
        use std::io::Write;
        writeln!(f, "{}", serde_json::to_string(&row).unwrap()).map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub fn one(file: &str, work: &str, bin_dir: &str) -> Result<(), String> {
    let ext = file.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    let app = crate::app_for(&ext).ok_or("only docx, xlsx, pptx")?;
    let e = Entry {
        id: "one".into(),
        source: "cli".into(),
        rel: file.into(),
        path: file.into(),
        ext,
        app: app.into(),
        size: std::fs::metadata(file).map_err(|e| e.to_string())?.len(),
        tags: vec![],
        stratum: "one".into(),
    };
    std::fs::create_dir_all(work).map_err(|e| e.to_string())?;
    println!("{}", serde_json::to_string_pretty(&process(&e, work, bin_dir, 90)).unwrap());
    Ok(())
}
