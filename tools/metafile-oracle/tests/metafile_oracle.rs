//! Old-vs-new differential for the craft-metafile swaps (#92, verification #185): every EMF/WMF
//! embedded in the Office corpus is parsed and drawn by each app's pre-swap code (OLD) and by the
//! swap branch on this tree's `craft-metafile` (NEW).
//!
//! ```text
//! mkdir cache && cargo run --release --bin extract -- <corpus-root> cache
//! METAFILE_CACHE=cache cargo test --release -- --ignored --nocapture
//! ```
//!
//! Per metafile and per app the side reports, in order:
//! 1. parse result (an `Err` side is classified by craft-oracle: both-fail / only-old / only-new),
//! 2. the parsed record stream (deckcraft: the whole scene as `Debug`; wordcraft: a projection onto
//!    the fields both scenes carry, see `wc_project`),
//! 3. a raster drawn by the app's own renderer (deckcraft `decode_image`, 1024 px longer side;
//!    wordcraft `render_items` of one `Draw::Image`, 1 px/pt of the picture's frame, capped).
//!
//! The judge is strict: records equal and rasters byte-identical, else `Exceeds`, which
//! `expected.toml` may register as an intended change.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::time::Duration;

use craft_compare::Verdict;
use craft_oracle::{Differential, FnJudge, FnSide, Outcome, Policy, Registry};
use metafile_oracle::{Raster, judge_rasters, raster_report};

#[derive(Clone, Debug, PartialEq)]
struct Dump {
    /// Record stream, one line per item (plus a size header).
    records: Vec<String>,
    raster: Option<Raster>,
    /// Facts about the scene that are not compared (e.g. non-uniform text scale).
    notes: Vec<String>,
    /// New side only, when the sides differ: what the difference is made of (see [`attribute`]).
    attr: Option<Attr>,
}

/// Attribution of a difference by masking (WordCraft only), see [`attribute`].
#[derive(Clone, Debug, PartialEq)]
struct Attr {
    /// Metric name the judge reports, matched by `metric` entries in expected.toml.
    class: String,
    detail: String,
    family: Option<String>,
    text_cause: Option<String>,
}

/// The judge: strict equality of records and raster. A difference the masking ladder attributes is
/// reported under the attributed class as its metric (so `expected.toml` can register the class);
/// an unattributed one keeps the raw metric (`records`, `max_abs`, ...) and stays a regression.
fn judge(old: &Dump, new: &Dump) -> Verdict {
    let v = judge_plain(old, new);
    match (&v, &new.attr) {
        (Verdict::Exceeds { value, limit, .. }, Some(a)) => Verdict::Exceeds { metric: a.class.clone(), value: *value, limit: *limit },
        _ => v,
    }
}

fn judge_plain(old: &Dump, new: &Dump) -> Verdict {
    if old.records != new.records {
        let at = old.records.iter().zip(&new.records).position(|(a, b)| a != b).unwrap_or(old.records.len().min(new.records.len()));
        return Verdict::Exceeds { metric: "records".into(), value: at as f64, limit: 0.0 };
    }
    judge_rasters(&old.raster, &new.raster)
}

fn cache_files(dir: &Path) -> Vec<(String, PathBuf)> {
    let mut v: Vec<_> = std::fs::read_dir(dir)
        .expect("METAFILE_CACHE")
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "emf" || e == "wmf"))
        .collect();
    v.sort();
    v.into_iter().map(|p| (p.file_name().unwrap().to_string_lossy().into_owned(), p)).collect()
}

fn to_raster(img: image::RgbaImage) -> Raster {
    Raster { width: img.width(), height: img.height(), rgba: img.into_raw() }
}

// ------------------------------------------------------------------------------------ deckcraft

mod dc {
    use super::*;

    pub fn old(bytes: &[u8]) -> Result<Dump, String> {
        let pic = deckcraft_meta_old::parse(bytes).map_err(|e| e.to_string())?;
        let mut records = vec![format!("size {:?} {:?}", pic.width, pic.height)];
        records.extend(pic.items.iter().map(|i| format!("{i:?}")));
        let notes = Vec::new();
        let raster = deckcraft_render_old::decode_image(bytes).map(to_raster);
        Ok(Dump { records, raster, notes, attr: None })
    }

    pub fn new(bytes: &[u8]) -> Result<Dump, String> {
        let mut d = new_raw(bytes)?;
        if let Ok(o) = old(bytes)
            && !judge_plain(&o, &d).passed()
        {
            d.attr = super::attribute_deckcraft(bytes);
        }
        Ok(d)
    }

    pub fn new_raw(bytes: &[u8]) -> Result<Dump, String> {
        let pic = craft_metafile::parse(bytes).map_err(|e| e.to_string())?;
        let mut records = vec![format!("size {:?} {:?}", pic.width, pic.height)];
        records.extend(pic.items.iter().map(|i| format!("{i:?}")));
        let notes = Vec::new();
        let raster = deckcraft_render_new::decode_image(bytes).map(to_raster);
        Ok(Dump { records, raster, notes, attr: None })
    }
}

// ------------------------------------------------------------------------------------ wordcraft

fn r3(v: f64) -> String {
    format!("{:.3}", if v == 0.0 { 0.0 } else { v })
}

fn sha(b: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(b).iter().take(6).map(|x| format!("{x:02x}")).collect()
}

fn segs_old(s: &[wordcraft_meta_old::Seg]) -> String {
    s.iter().map(|s| format!("{s:?}")).collect::<Vec<_>>().join(" ")
}

fn segs_new(s: &[craft_metafile::Seg]) -> String {
    s.iter().map(|s| format!("{s:?}")).collect::<Vec<_>>().join(" ")
}

mod wc {
    use super::*;

    macro_rules! raster_side {
        ($name:ident, $render:ident, $doc:ident, $layout:ident, $geom:ident) => {
            pub fn $name(bytes: &[u8]) -> Option<Raster> {
                let (w, h, _) = $render::metafile_size(bytes)?;
                let mut doc = $doc::Document::new();
                doc.media.insert("m".into(), std::sync::Arc::new(bytes.to_vec()));
                let item = $layout::display::Draw::Image {
                    rect: $geom::Rect { x: 0.0, y: 0.0, w, h },
                    media: "m".into(),
                    crop: [0.0; 4],
                    alpha: 1.0,
                    alt: String::new(),
                    decorative: false,
                };
                let scale = (1024.0 / w.max(h)).min(4.0);
                let r = $render::render_items(&doc, &[item], $geom::Rect { x: 0.0, y: 0.0, w, h }, scale, &$render::RenderOptions::default());
                Some(Raster { width: r.width, height: r.height, rgba: r.pixels })
            }
        };
    }
    raster_side!(raster_old, wordcraft_render_old, wordcraft_doc_old, wordcraft_layout_old, wordcraft_geom_old);
    raster_side!(raster_new, wordcraft_render_new, wordcraft_doc_new, wordcraft_layout_new, wordcraft_geom_new);

    pub fn old(bytes: &[u8]) -> Result<Dump, String> {
        use wordcraft_meta_old::{HAlign, Item};
        let pic = wordcraft_meta_old::parse(bytes).map_err(|e| e.to_string())?;
        let mut records = vec![format!("size {:?} {:?}", pic.width, pic.height)];
        for it in &pic.items {
            records.push(match it {
                Item::Path { segs, fill, stroke, even_odd } => format!("path fill={fill:?} stroke={stroke:?} eo={even_odd} [{}]", segs_old(segs)),
                Item::Bitmap { rect, width, height, rgba } => format!("bitmap {rect:?} {width}x{height} {}", sha(rgba)),
                Item::Text(t) => {
                    let h = match t.halign {
                        HAlign::Left => "L",
                        HAlign::Center => "C",
                        HAlign::Right => "R",
                    };
                    let v = format!("{:?}", t.valign);
                    format!(
                        "text {:?} dx={:?} font={:?} b={} i={} color={:?} at=({},{}) size={} cell={} angle={} {h}/{v}",
                        t.text,
                        t.dx,
                        t.font,
                        t.bold,
                        t.italic,
                        t.color,
                        r3(t.x as f64),
                        r3(t.y as f64),
                        r3(t.size as f64),
                        t.cell,
                        r3(t.angle as f64)
                    )
                }
            });
        }
        let notes = Vec::new();
        let raster = raster_old(bytes);
        Ok(Dump { records, raster, notes, attr: None })
    }

    pub fn new(bytes: &[u8]) -> Result<Dump, String> {
        let mut d = new_raw(bytes)?;
        if let Ok(o) = old(bytes)
            && !judge_plain(&o, &d).passed()
        {
            d.attr = super::attribute(bytes);
        }
        Ok(d)
    }

    pub fn new_raw(bytes: &[u8]) -> Result<Dump, String> {
        use craft_metafile::{HAlign, Item, VAlign};
        let pic = craft_metafile::parse(bytes).map_err(|e| e.to_string())?;
        let mut records = vec![format!("size {:?} {:?}", pic.width, pic.height)];
        for it in &pic.items {
            records.push(match it {
                Item::Path { segs, fill, stroke, even_odd, .. } => format!("path fill={fill:?} stroke={stroke:?} eo={even_odd} [{}]", segs_new(segs)),
                Item::Bitmap { rect, width, height, rgba, .. } => format!("bitmap {rect:?} {width}x{height} {}", sha(rgba)),
                Item::Pattern { segs, even_odd, .. } => format!("pattern eo={even_odd} [{}]", segs_new(segs)),
                Item::Text(t) => {
                    let [a, b, c, d, e, f] = t.transform;
                    let k = a.hypot(b);
                    let ky = c.hypot(d);
                    let h = match t.h_align {
                        HAlign::Left => "L",
                        HAlign::Center => "C",
                        HAlign::Right => "R",
                    };
                    let v = match t.v_align {
                        VAlign::Top => "Top".to_string(),
                        VAlign::Baseline => "Baseline".into(),
                        VAlign::Bottom => "Bottom".into(),
                    };
                    let dx: Vec<f32> = t.dx.iter().map(|d| (*d as f64 * k) as f32).collect();
                    format!(
                        "text {:?} dx={dx:?} font={:?} b={} i={} color={:?} at=({},{}) size={} cell={} angle={} {h}/{v}",
                        t.text,
                        t.font.family,
                        t.font.weight >= 700,
                        t.font.italic,
                        t.color,
                        r3(e),
                        r3(f),
                        r3(t.font.height.abs() as f64 * ky),
                        t.font.height > 0.0,
                        r3(b.atan2(a))
                    )
                }
            });
        }
        let mut notes = Vec::new();
        for it in &pic.items {
            let has_region = match it {
                Item::Path { region, .. } | Item::Bitmap { region, .. } | Item::Pattern { region, .. } => region.is_some(),
                Item::Text(t) => t.region.is_some() || t.clip.is_some(),
            };
            if has_region {
                notes.push("region".to_string());
            }
            if let Item::Text(t) = it {
                let [a, b, c, d, ..] = t.transform;
                let (kx, ky) = (a.hypot(b), c.hypot(d));
                let an = (kx / ky - 1.0).abs();
                // Beyond f32 rounding of the old isotropic size (about 1e-7).
                if an > 1e-6 {
                    notes.push("aniso-text".to_string());
                    notes.push(format!("aniso={an:.6}"));
                }
            }
        }
        let raster = raster_new(bytes);
        Ok(Dump { records, raster, notes, attr: None })
    }
}

// ------------------------------------------------------------------------------------ driver

/// Record function numbers present in the file (EMF: record types; WMF: function words).
fn record_types(bytes: &[u8]) -> Vec<u32> {
    let u32at = |o: usize| bytes.get(o..o + 4).map(|b| u32::from_le_bytes(b.try_into().unwrap()));
    let mut out = Vec::new();
    if u32at(0) == Some(1) && bytes.get(40..44) == Some(b" EMF") {
        let mut o = 0usize;
        while let (Some(t), Some(sz)) = (u32at(o), u32at(o + 4)) {
            if sz < 8 {
                break;
            }
            out.push(t);
            o += sz as usize;
        }
    } else {
        let mut o = if u32at(0) == Some(0x9AC6_CDD7) { 22 } else { 0 };
        o += 18;
        while let Some(words) = u32at(o) {
            if words < 3 {
                break;
            }
            let Some(f) = bytes.get(o + 4..o + 6) else { break };
            out.push(u32::from(u16::from_le_bytes([f[0], f[1]])));
            o += words as usize * 2;
        }
    }
    out
}

/// Records the pre-swap WordCraft reader did not handle (COVERAGE.md, WordCraft column is `—`,
/// `slot` with shared `Y`, or EMF+ comments), by (format, number, name).
const WC_GAPS_EMF: &[(u32, &str)] = &[
    (18, "SETBKMODE"),
    (25, "SETBKCOLOR"),
    (29, "EXCLUDECLIPRECT"),
    (30, "INTERSECTCLIPRECT"),
    (31, "SCALEVIEWPORTEXTEX"),
    (32, "SCALEWINDOWEXTEX"),
    (57, "SETARCDIRECTION"),
    (68, "ABORTPATH"),
    (75, "EXTSELECTCLIPRGN"),
    (83, "EXTTEXTOUTA"),
    (93, "CREATEMONOBRUSH"),
    (94, "CREATEDIBPATTERNBRUSHPT"),
    (108, "SMALLTEXTOUT"),
    (70, "COMMENT(EMF+)"),
];
const WC_GAPS_WMF: &[(u32, &str)] = &[
    (0x0102, "SETBKMODE"),
    (0x012E, "SETTEXTALIGN"),
    (0x0142, "DIBCREATEPATTERNBRUSH"),
    (0x01F9, "CREATEPATTERNBRUSH"),
    (0x0201, "SETBKCOLOR"),
    (0x0209, "SETTEXTCOLOR"),
    (0x02FB, "CREATEFONTINDIRECT"),
    (0x0415, "EXCLUDECLIPRECT"),
    (0x0416, "INTERSECTCLIPRECT"),
    (0x0521, "TEXTOUT"),
    (0x061D, "PATBLT"),
    (0x0A32, "EXTTEXTOUT"),
];

fn gap_tags(bytes: &[u8]) -> std::collections::BTreeSet<String> {
    let emf = bytes.get(40..44) == Some(b" EMF");
    let table = if emf { WC_GAPS_EMF } else { WC_GAPS_WMF };
    let present = record_types(bytes);
    table.iter().filter(|(n, _)| present.contains(n)).map(|(_, name)| format!("{}:{name}", if emf { "emf" } else { "wmf" })).collect()
}

/// Attribution ladder. Each level masks (replaces by a same-size no-op: an EMF comment without
/// payload, or an unknown WMF function) a further family of records in the input of both sides;
/// the first level at which old and new agree names what the difference was made of.
/// Level 1: records the pre-swap WordCraft reader did not handle (COVERAGE.md, `WC_GAPS_*`), except
/// the ones that only allocate object slots; 2: SETROP2 (mix modes); 3: text-out records
/// (EMF 83/84/108; WMF text is level 1); 4: pattern/mono brush creation (slot allocators).
const LEVELS: [&str; 4] = ["gap-records", "gap-records+rop2", "gap-records+rop2+text-drawing", "gap-records+rop2+text-drawing+pattern-brushes"];

fn mask_level(bytes: &[u8], level: usize) -> Vec<u8> {
    let mut emf_set: Vec<u32> = WC_GAPS_EMF.iter().map(|(n, _)| *n).filter(|n| ![93, 94].contains(n)).collect();
    let mut wmf_set: Vec<u32> = WC_GAPS_WMF.iter().map(|(n, _)| *n).filter(|n| ![0x0142, 0x01F9, 0x02FB].contains(n)).collect();
    if level >= 2 {
        emf_set.push(20);
        wmf_set.push(0x0104);
    }
    if level >= 3 {
        emf_set.extend([83, 84, 108]);
    }
    if level >= 4 {
        emf_set.extend([93, 94]);
        wmf_set.extend([0x0142, 0x01F9]);
    }
    mask_set(bytes, &emf_set, &wmf_set)
}

/// Gap-record families: (name, EMF record types, WMF functions). Level 1 masks all of them; a file
/// is attributed to a family when masking every family but that one still leaves a difference.
const FAMILIES: [(&str, &[u32], &[u32]); 8] = [
    ("wmf-text-out", &[], &[0x0521, 0x0A32]),
    ("wmf-patblt", &[], &[0x061D]),
    ("clip-rect-region", &[29, 30, 75], &[0x0415, 0x0416]),
    ("emf+-playback", &[70], &[]),
    ("extent-scaling", &[31, 32], &[]),
    ("arc-direction-abort-path", &[57, 68], &[]),
    ("emf-text-a-small", &[83, 108], &[]),
    ("bk-mode-color-text-state", &[18, 25], &[0x0102, 0x012E, 0x0201, 0x0209]),
];

fn mask_set(bytes: &[u8], emf_set: &[u32], wmf_set: &[u32]) -> Vec<u8> {
    let mut b = bytes.to_vec();
    let u32at = |b: &[u8], o: usize| b.get(o..o + 4).map(|x| u32::from_le_bytes(x.try_into().unwrap()));
    if b.get(40..44) == Some(b" EMF") {
        let mut o = 0usize;
        while let (Some(t), Some(sz)) = (u32at(&b, o), u32at(&b, o + 4)) {
            if sz < 8 {
                break;
            }
            let end = o + sz as usize;
            let is_plus = t == 70 && b.get(o + 12..o + 16) == Some(b"EMF+");
            if sz >= 12 && end <= b.len() && emf_set.contains(&t) && (t != 70 || is_plus) {
                b[o..o + 4].copy_from_slice(&70u32.to_le_bytes());
                b[o + 8..o + 12].copy_from_slice(&(sz - 12).to_le_bytes());
                b[o + 12..end].fill(0);
            }
            o = end;
        }
    } else {
        let mut o = if u32at(&b, 0) == Some(0x9AC6_CDD7) { 22 } else { 0 };
        o += 18;
        while let Some(words) = u32at(&b, o) {
            if words < 3 || o + 6 > b.len() {
                break;
            }
            let f = u32::from(u16::from_le_bytes([b[o + 4], b[o + 5]]));
            if wmf_set.contains(&f) {
                b[o + 4..o + 6].copy_from_slice(&0x0F00u16.to_le_bytes());
            }
            o += words as usize * 2;
        }
    }
    b
}

/// DeckCraft differences are few, so one mask each: with the named records replaced by no-ops in the
/// input of both sides, do old and new agree?
fn attribute_deckcraft(bytes: &[u8]) -> Option<Attr> {
    let tries: [(&str, &[u32], &[u32]); 2] = [("extent-scaling", &[31, 32], &[]), ("select-clip-region", &[], &[0x012C])];
    for (name, emf, wmf) in tries {
        let masked = mask_set(bytes, emf, wmf);
        if let (Ok(x), Ok(y)) = (dc::old(&masked), dc::new_raw(&masked))
            && judge_plain(&x, &y).passed()
        {
            return Some(Attr {
                class: format!("deckcraft/{name}"),
                detail: format!("identical with the {name} records masked"),
                family: None,
                text_cause: None,
            });
        }
    }
    None
}

/// Attribute the difference between the pre-swap and the shared WordCraft code on `bytes` by
/// masking. Levels (`LEVELS`) cumulatively replace families of records, in the input of both sides,
/// by same-size no-ops; the first level at which the sides agree names the cause. At level 1 the
/// gap family that alone still makes the sides differ is recorded, at level 3 the text cause.
fn attribute(bytes: &[u8]) -> Option<Attr> {
    let gaps = gap_tags(bytes);
    for (i, name) in LEVELS.iter().enumerate() {
        let masked = mask_level(bytes, i + 1);
        let (Ok(x), Ok(y)) = (wc::old(&masked), wc::new_raw(&masked)) else { continue };
        if !judge_plain(&x, &y).passed() {
            continue;
        }
        let mut attr = Attr {
            class: (*name).to_string(),
            detail: format!("identical at level {} ({name}); gap records present: {gaps:?}", i + 1),
            family: None,
            text_cause: None,
        };
        if i == 0 {
            let present = record_types(bytes);
            let emf = bytes.get(40..44) == Some(b" EMF");
            let all_emf: Vec<u32> = FAMILIES.iter().flat_map(|f| f.1.iter().copied()).collect();
            let all_wmf: Vec<u32> = FAMILIES.iter().flat_map(|f| f.2.iter().copied()).collect();
            let mut causal = Vec::new();
            for (fname, fe, fw) in FAMILIES {
                let own = if emf { fe } else { fw };
                if !own.iter().any(|r| present.contains(r)) {
                    continue;
                }
                let me: Vec<u32> = all_emf.iter().copied().filter(|r| !fe.contains(r)).collect();
                let mw: Vec<u32> = all_wmf.iter().copied().filter(|r| !fw.contains(r)).collect();
                let only = mask_set(bytes, &me, &mw);
                if let (Ok(x), Ok(y)) = (wc::old(&only), wc::new_raw(&only))
                    && !judge_plain(&x, &y).passed()
                {
                    causal.push(fname);
                }
            }
            attr.family = Some(if causal.is_empty() { "combination of families".to_string() } else { causal.join("+") });
        }
        if i == 2
            && let (Ok(x), Ok(y)) = (wc::old(bytes), wc::new_raw(bytes))
        {
            let cause = text_cause(&x, &y, bytes);
            attr.class = format!("{name}/{}", cause.0);
            attr.text_cause = Some(cause.1.to_string());
        }
        return Some(attr);
    }
    None
}

/// Why drawing the text records alone makes old and new differ (level 3): the cause is read off the
/// two record streams, not guessed from the pixels.
fn text_cause(old: &Dump, new: &Dump, bytes: &[u8]) -> (&'static str, &'static str) {
    let (so, sn): (std::collections::BTreeSet<&String>, std::collections::BTreeSet<&String>) =
        (old.records.iter().collect(), new.records.iter().collect());
    let only_new_paths = sn.difference(&so).filter(|l| l.starts_with("path") || l.starts_with("bitmap")).count();
    let old_font_garbage = old
        .records
        .iter()
        .any(|l| l.starts_with("text") && l.split("font=\"").nth(1).is_some_and(|f| f.split('"').next().is_some_and(|n| n.contains("\\0"))));
    if only_new_paths > 0 {
        ("opaque-cell-fill", "text record fills its cell (ETO_OPAQUE): new draws the background rectangle, old did not")
    } else if old_font_garbage {
        ("face-name-nul", "face name not trimmed at NUL in the old reader (garbage after the name), so the font did not resolve")
    } else {
        let an = b_aniso(bytes);
        if an > 1e-2 {
            ("affine-scale-over-1pct", "text drawn under the full affine, x and y scale differ by more than 1%")
        } else if an > 1e-6 {
            ("affine-scale-up-to-1pct", "text drawn under the full affine, x and y scale differ by at most 1%")
        } else {
            ("float-rounding", "isotropic text with the same projection: floating-point rounding of the glyph transform")
        }
    }
}

fn has_emf_plus(bytes: &[u8]) -> bool {
    let u32at = |o: usize| bytes.get(o..o + 4).map(|b| u32::from_le_bytes(b.try_into().unwrap()));
    if bytes.get(40..44) != Some(b" EMF") {
        return false;
    }
    let mut o = 0;
    while let (Some(t), Some(sz)) = (u32at(o), u32at(o + 4)) {
        if sz < 8 {
            break;
        }
        if t == 70 && bytes.get(o + 12..o + 16) == Some(b"EMF+") {
            return true;
        }
        o += sz as usize;
    }
    false
}

/// Largest |kx/ky - 1| over the new scene's text runs (0 when there is none).
fn b_aniso(bytes: &[u8]) -> f64 {
    let Ok(pic) = craft_metafile::parse(bytes) else { return 0.0 };
    pic.items
        .iter()
        .filter_map(|i| if let craft_metafile::Item::Text(t) = i { Some(t) } else { None })
        .map(|t| {
            let [a, b, c, d, ..] = t.transform;
            (a.hypot(b) / c.hypot(d) - 1.0).abs()
        })
        .fold(0.0, f64::max)
}

/// What kind of difference: geometry (path/bitmap) lines, text lines, pattern items, regions.
fn signature(a: &Dump, b: &Dump) -> String {
    let strip = |l: &str| l.to_string();
    let split = |d: &Dump| {
        let (mut geom, mut text, mut pat) = (Vec::new(), Vec::new(), 0usize);
        for l in d.records.iter().skip(1) {
            if l.starts_with("text") {
                text.push(strip(l));
            } else if l.starts_with("pattern") {
                pat += 1;
            } else {
                geom.push(strip(l));
            }
        }
        (geom, text, pat)
    };
    let (ga, ta, pa) = split(a);
    let (gb, tb, pb) = split(b);
    let regions = b.notes.iter().filter(|n| n.starts_with("region")).count();
    format!(
        "header={} geom={} text={}(old {} new {}) patterns(old {} new {}) new-with-region={} raster={}",
        if a.records.first() == b.records.first() { "eq" } else { "DIFF" },
        if ga == gb { "eq" } else { "DIFF" },
        if ta == tb {
            "eq"
        } else if ta.is_empty() {
            "new-only"
        } else {
            "DIFF"
        },
        ta.len(),
        tb.len(),
        pa,
        pb,
        regions,
        if a.raster == b.raster { "eq" } else { "DIFF" }
    )
}

fn manifest(dir: &Path) -> BTreeMap<String, Vec<String>> {
    let mut m: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for l in std::fs::read_to_string(dir.join("manifest.tsv")).unwrap_or_default().lines() {
        let f: Vec<_> = l.split('\t').collect();
        if f.len() >= 4 {
            m.entry(format!("{}.{}", f[2], f[3])).or_default().push(format!("{} :: {}", f[0], f[1]));
        }
    }
    m
}

fn run_app(app: &str, old: fn(&[u8]) -> Result<Dump, String>, new: fn(&[u8]) -> Result<Dump, String>) {
    let Some(dir) = std::env::var_os("METAFILE_CACHE") else {
        eprintln!("METAFILE_CACHE is not set; nothing to compare");
        return;
    };
    let dir = PathBuf::from(dir);
    let files = cache_files(&dir);
    assert!(!files.is_empty(), "no .emf/.wmf under {}", dir.display());
    let man = manifest(&dir);
    let out = PathBuf::from(std::env::var_os("METAFILE_ORACLE_OUT").unwrap_or_else(|| "target/metafile-oracle".into())).join(app);
    std::fs::create_dir_all(&out).unwrap();
    let registry = Registry::from_path(&Path::new(env!("CARGO_MANIFEST_DIR")).join("expected.toml")).expect("expected.toml");
    let policy = Policy::default();
    let diff = Differential::new(FnSide(old), FnSide(new), FnJudge(judge))
        .timeout(Duration::from_secs(120))
        .expected(registry)
        .policy(policy)
        .artifact_dir(out.join("failures"));
    // The app prefix lets one expected.toml hold both apps' entries.
    let lazy = files.iter().map(|(n, p)| (format!("{app}/{n}"), std::fs::read(p).unwrap_or_default()));
    let report = diff.run_files(lazy);
    report.write(&out, &policy).expect("write report");

    let mut details = String::new();
    let (mut both_same, mut both_differ, mut raster_none, mut identical_drawn, mut identical_with_text) = (0, 0, 0, 0, 0);
    let mut by_kind: BTreeMap<String, usize> = BTreeMap::new();
    let mut classes: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut tag_counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut family_counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut text_cause_rows: Vec<(String, Option<Raster>, Option<Raster>)> = Vec::new();
    for r in &report.results {
        let short = r.name.split_once('/').map_or(r.name.as_str(), |x| x.1);
        let bytes = std::fs::read(dir.join(short)).unwrap_or_default();
        let kind = if short.ends_with("emf") { "emf" } else { "wmf" };
        *by_kind.entry(format!("{kind} {}", r.outcome.name())).or_default() += 1;
        match &r.outcome {
            Outcome::Identical => {
                if let Ok(d) = old(&bytes) {
                    raster_none += usize::from(d.raster.is_none());
                    // "Identical" is only evidence when something is drawn.
                    let drawn = d.records.len() > 1 && d.raster.as_ref().is_some_and(|r| r.rgba.chunks(4).any(|p| p[3] != 0));
                    identical_drawn += usize::from(drawn);
                    identical_with_text += usize::from(d.records.iter().any(|l| l.starts_with("text") || l.starts_with("Text(")));
                }
            }
            Outcome::BothFail { old_err, new_err } => {
                if old_err == new_err {
                    both_same += 1;
                } else {
                    both_differ += 1;
                    let _ = writeln!(details, "{short} [both fail, different messages]\n  old: {old_err}\n  new: {new_err}");
                }
            }
            o => {
                let _ = writeln!(
                    details,
                    "{short} [{}] {o:?}\n  sources: {}",
                    o.name(),
                    man.get(short).map(|v| v.iter().take(3).cloned().collect::<Vec<_>>().join(" | ")).unwrap_or_default()
                );
                match (old(&bytes), new(&bytes)) {
                    (Ok(a), Ok(b)) => {
                        let _ = writeln!(details, "  signature: {}", signature(&a, &b));
                        let mut tags = std::collections::BTreeSet::new();
                        if let Some(at) = &b.attr {
                            tags.insert(at.class.clone());
                            let _ = writeln!(
                                details,
                                "  masked: {}; family left unmasked that alone still differs: {:?}; text cause: {:?}",
                                at.detail, at.family, at.text_cause
                            );
                            if let Some(f) = &at.family {
                                *family_counts.entry(f.clone()).or_default() += 1;
                            }
                            text_cause_rows.push((at.class.clone(), a.raster.clone(), b.raster.clone()));
                        } else if app == "wordcraft" {
                            let _ = writeln!(details, "  masked: not identical at any masking level");
                        }
                        let tag = if tags.is_empty() { "UNEXPLAINED".to_string() } else { tags.iter().cloned().collect::<Vec<_>>().join(",") };
                        let an = b_aniso(&bytes);
                        let _ = writeln!(details, "  tags: {tag}; max text anisotropy |kx/ky-1| = {an:.6}");
                        classes.entry(tag.clone()).or_default().push(short.to_string());
                        for t in &tags {
                            *tag_counts.entry(t.clone()).or_default() += 1;
                        }
                        let n = a.records.len().max(b.records.len());
                        let mut shown = 0;
                        for i in 0..n {
                            if a.records.get(i) != b.records.get(i) && shown < 4 {
                                shown += 1;
                                let _ = writeln!(
                                    details,
                                    "  record {i}:\n    old: {}\n    new: {}",
                                    a.records.get(i).map_or("<none>", |s| s.as_str()).chars().take(400).collect::<String>(),
                                    b.records.get(i).map_or("<none>", |s| s.as_str()).chars().take(400).collect::<String>()
                                );
                            }
                        }
                        let _ = writeln!(
                            details,
                            "  records: old {} new {}; differing lines {}",
                            a.records.len(),
                            b.records.len(),
                            (0..n).filter(|i| a.records.get(*i) != b.records.get(*i)).count()
                        );
                        if let (Some(x), Some(y)) = (&a.raster, &b.raster) {
                            match raster_report(x, y) {
                                Some(rep) => {
                                    let _ = writeln!(
                                        details,
                                        "  raster {}x{}: max_abs {:.5} ({:.1}/255) p99 {:.5} mean {:.6} ssim {:.6} dE_mean {:.4} dE_max {:.3} bad_px {:.6}",
                                        x.width,
                                        x.height,
                                        rep.max_abs,
                                        rep.max_abs * 255.0,
                                        rep.p99_abs,
                                        rep.mean_abs,
                                        rep.ssim,
                                        rep.delta_e_mean,
                                        rep.delta_e_max,
                                        rep.bad_pixel_fraction
                                    );
                                }
                                None => {
                                    let _ = writeln!(details, "  raster shapes differ: {}x{} vs {}x{}", x.width, x.height, y.width, y.height);
                                }
                            }
                            if x != y {
                                for (tag, rr) in [("old", x), ("new", y)] {
                                    if let Some(img) = image::RgbaImage::from_raw(rr.width, rr.height, rr.rgba.clone()) {
                                        let _ = img.save(out.join(format!("{short}.{tag}.png")));
                                    }
                                }
                            }
                        } else {
                            let _ = writeln!(details, "  raster: old {} new {}", a.raster.is_some(), b.raster.is_some());
                        }
                    }
                    (a, b) => {
                        let _ = writeln!(details, "  old: {:?}\n  new: {:?}", a.map(|_| "ok"), b.map(|_| "ok"));
                    }
                }
            }
        }
    }
    let emf_plus = files.iter().filter(|(_, p)| std::fs::read(p).is_ok_and(|b| has_emf_plus(&b))).count();
    let _ =
        writeln!(details, "\nboth-fail identical messages: {both_same}; different messages: {both_differ}; identical with no raster: {raster_none}");
    let _ = writeln!(
        details,
        "identical files that draw something (items and a non-transparent pixel): {identical_drawn}; identical files with text runs: {identical_with_text}; files carrying EMF+ records: {emf_plus}"
    );
    for (k, n) in &by_kind {
        let _ = writeln!(details, "{k}: {n}");
    }
    let _ = writeln!(details, "\nlevel-1 files by the gap family that alone makes the sides differ:");
    for (f, n) in &family_counts {
        let _ = writeln!(details, "  {f}: {n}");
    }
    let mut by_cause: BTreeMap<String, Vec<(f64, f64, f64)>> = BTreeMap::new();
    for (c, x, y) in &text_cause_rows {
        if let (Some(x), Some(y)) = (x, y)
            && let Some(r) = raster_report(x, y)
        {
            by_cause.entry(c.clone()).or_default().push((r.max_abs * 255.0, r.ssim, r.delta_e_max));
        }
    }
    let _ =
        writeln!(details, "\nraster metrics of the non-identical files by attributed class: count, worst max_abs/255, lowest SSIM, worst dE_max:");
    for (c, v) in &by_cause {
        let mx = v.iter().map(|t| t.0).fold(0.0, f64::max);
        let ss = v.iter().map(|t| t.1).fold(1.0, f64::min);
        let de = v.iter().map(|t| t.2).fold(0.0, f64::max);
        let _ = writeln!(details, "  {c}: {} files, max_abs {mx:.0}/255, ssim {ss:.6}, dE_max {de:.2}", v.len());
    }
    let _ = writeln!(details, "\nnon-identical files by cause tag (a file can carry several):");
    for (t, n) in &tag_counts {
        let _ = writeln!(details, "  {t}: {n}");
    }
    for (c, v) in &classes {
        let _ = writeln!(details, "{c}: {}", v.len());
        if c == "UNEXPLAINED" {
            for f in v {
                let _ = writeln!(details, "  {f}");
            }
        }
    }
    std::fs::write(out.join("details.txt"), &details).unwrap();
    eprintln!("{}", report.to_markdown(&policy));
    eprintln!("[{app}] files {}; both-fail same msg {both_same}, different msg {both_differ}; out {}", files.len(), out.display());
    assert_eq!(report.exit_code(&policy), 0, "regressions: see {}", out.join("details.txt").display());
}

#[test]
#[ignore = "needs METAFILE_CACHE (run `extract` first)"]
fn deckcraft_old_and_new_agree_on_the_office_corpus() {
    run_app("deckcraft", dc::old, dc::new);
}

#[test]
#[ignore = "needs METAFILE_CACHE (run `extract` first)"]
fn wordcraft_old_and_new_agree_on_the_office_corpus() {
    run_app("wordcraft", wc::old, wc::new);
}

/// Debug aid: `METAFILE_FILE=path cargo test --release dump_one -- --ignored --nocapture` prints
/// the raw scene of every side for one file.
#[test]
#[ignore = "debug aid"]
fn dump_one() {
    let Some(p) = std::env::var_os("METAFILE_FILE") else { return };
    let bytes = std::fs::read(p).unwrap();
    println!("wordcraft old: {:#?}", wordcraft_meta_old::parse(&bytes));
    println!("new: {:#?}", craft_metafile::parse(&bytes));
}

/// Debug aid: replace the listed EMF record types (`MASK=20,30`) with no-op comments and judge
/// the wordcraft sides on the result: `METAFILE_FILE=f MASK=20 cargo test --release probe_mask -- --ignored --nocapture`.
#[test]
#[ignore = "debug aid"]
fn probe_mask() {
    let Some(p) = std::env::var_os("METAFILE_FILE") else { return };
    let mut b = std::fs::read(p).unwrap();
    let wmf_mask: Vec<u32> = std::env::var("MASKW")
        .unwrap_or_default()
        .split(',')
        .filter_map(|x| u32::from_str_radix(x.trim().trim_start_matches("0x"), 16).ok())
        .collect();
    if b.get(40..44) != Some(b" EMF") {
        let u32at = |b: &[u8], o: usize| b.get(o..o + 4).map(|x| u32::from_le_bytes(x.try_into().unwrap()));
        let mut o = if u32at(&b, 0) == Some(0x9AC6_CDD7) { 22 } else { 0 } + 18;
        while let Some(w) = u32at(&b, o) {
            if w < 3 || o + 6 > b.len() {
                break;
            }
            if wmf_mask.contains(&u32::from(u16::from_le_bytes([b[o + 4], b[o + 5]]))) {
                b[o + 4..o + 6].copy_from_slice(&0x0F00u16.to_le_bytes());
            }
            o += w as usize * 2;
        }
    }
    let mask: Vec<u32> = std::env::var("MASK").unwrap_or_default().split(',').filter_map(|x| x.trim().parse().ok()).collect();
    let mut o = 0usize;
    while o + 8 <= b.len() {
        let t = u32::from_le_bytes(b[o..o + 4].try_into().unwrap());
        let sz = u32::from_le_bytes(b[o + 4..o + 8].try_into().unwrap()) as usize;
        if sz < 8 || o + sz > b.len() {
            break;
        }
        if sz >= 12 && mask.contains(&t) {
            b[o..o + 4].copy_from_slice(&70u32.to_le_bytes());
            b[o + 8..o + 12].copy_from_slice(&((sz - 12) as u32).to_le_bytes());
            b[o + 12..o + sz].fill(0);
        }
        o += sz;
    }
    match (wc::old(&b), wc::new(&b)) {
        (Ok(x), Ok(y)) => {
            println!("verdict {:?}; {}; records old {} new {}", judge(&x, &y), signature(&x, &y), x.records.len(), y.records.len());
            if std::env::var_os("SETDIFF").is_some() {
                let (so, sn): (std::collections::BTreeSet<_>, std::collections::BTreeSet<_>) =
                    (x.records.iter().collect(), y.records.iter().collect());
                for l in so.difference(&sn) {
                    println!("ONLY OLD {}", &l[..l.len().min(260)]);
                }
                for l in sn.difference(&so) {
                    println!("ONLY NEW {}", &l[..l.len().min(260)]);
                }
            }
            if let Some(i) = (0..x.records.len().max(y.records.len())).find(|i| x.records.get(*i) != y.records.get(*i)) {
                println!(
                    "first diff {i}:\n old {}\n new {}",
                    x.records.get(i).map_or("-", |s| &s[..s.len().min(300)]),
                    y.records.get(i).map_or("-", |s| &s[..s.len().min(300)])
                );
            }
        }
        (x, y) => println!("old {:?} new {:?}", x.map(|_| "ok"), y.map(|_| "ok")),
    }
}

/// Census aid: weights of every font used by a text run in the corpus (new reader).
#[test]
#[ignore = "debug aid"]
fn font_weight_census() {
    let Some(dir) = std::env::var_os("METAFILE_CACHE") else { return };
    let mut c: BTreeMap<u16, usize> = BTreeMap::new();
    for (_, p) in cache_files(Path::new(&dir)) {
        if let Ok(pic) = craft_metafile::parse(&std::fs::read(p).unwrap()) {
            for it in &pic.items {
                if let craft_metafile::Item::Text(t) = it {
                    *c.entry(t.font.weight).or_default() += 1;
                }
            }
        }
    }
    println!("weights: {c:?}");
}

/// Latent swap bug probe (no corpus file has a weight between 600 and 699): the pre-swap WordCraft
/// reader called a font bold from weight 600 (`font.weight >= 600`), the swapped renderer calls it
/// bold from 700 (`t.font.weight >= 700`, wordcraft `render/src/media.rs` and `pdf`). Patch every
/// EXTCREATEFONTINDIRECTW weight 700 -> 600 in a corpus EMF that has bold text and draw both.
/// `semibold_text_is_bold_before_and_regular_after_the_swap` documents the divergence: it passes
/// while the divergence exists and fails when the adapter is fixed (then register nothing and
/// delete this test).
#[test]
#[ignore = "needs METAFILE_CACHE"]
fn semibold_text_is_bold_before_and_regular_after_the_swap() {
    let Some(dir) = std::env::var_os("METAFILE_CACHE") else { return };
    for (name, p) in cache_files(Path::new(&dir)) {
        if !name.ends_with(".emf") {
            continue;
        }
        let mut b = std::fs::read(p).unwrap();
        // Only files whose text is really bold (weight 700 in use), so the patch is observable.
        if !wc::old(&b).is_ok_and(|d| d.records.iter().any(|l| l.contains(" b=true"))) {
            continue;
        }
        let (mut o, mut patched) = (0usize, 0);
        while o + 8 <= b.len() {
            let t = u32::from_le_bytes(b[o..o + 4].try_into().unwrap());
            let sz = u32::from_le_bytes(b[o + 4..o + 8].try_into().unwrap()) as usize;
            if sz < 8 || o + sz > b.len() {
                break;
            }
            // EMR_EXTCREATEFONTINDIRECTW: weight (i32) at record offset 8 + 20.
            if t == 82 && sz >= 36 && i32::from_le_bytes(b[o + 28..o + 32].try_into().unwrap()) == 700 {
                b[o + 28..o + 32].copy_from_slice(&600i32.to_le_bytes());
                patched += 1;
            }
            o += sz;
        }
        if patched == 0 {
            continue;
        }
        let (Ok(x), Ok(y)) = (wc::old(&b), wc::new_raw(&b)) else { continue };
        let has_text = x.records.iter().any(|l| l.starts_with("text"));
        if !has_text || x.raster.is_none() {
            continue;
        }
        let (bold_old, bold_new) =
            (x.records.iter().filter(|l| l.contains(" b=true")).count(), y.records.iter().filter(|l| l.contains(" b=true")).count());
        println!("{name}: weight 600 on {patched} fonts: bold runs old {bold_old}, new {bold_new}; rasters identical: {}", x.raster == y.raster);
        assert!(bold_old > 0 && bold_new == 0, "old reader should call weight 600 bold and the swapped adapter should not");
        return;
    }
    panic!("no corpus EMF with weight-700 text");
}
