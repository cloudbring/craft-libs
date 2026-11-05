//! Deterministic stratified sample from the shipped craft-corpus manifests.

use crate::{Entry, FOCUS_TAGS, app_for};
use std::path::Path;

/// splitmix64: tiny, fixed, dependency-free.
pub struct Rng(pub u64);
impl Rng {
    pub fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}

/// Files above this are skipped: a LibreOffice render of them would dominate the run time.
const MAX_BYTES: u64 = 8 * 1024 * 1024;
const SOURCES: [&str; 3] = ["office-poi", "office-libreoffice", "office-openxml-sdk"];

pub fn run(manifests: &str, corpus: &str, seed: u64, quotas: [usize; 3], out: &str) -> Result<(), String> {
    let mut by_ext: [Vec<Entry>; 3] = Default::default();
    for source in SOURCES {
        let text = std::fs::read_to_string(Path::new(manifests).join(format!("{source}.toml"))).map_err(|e| format!("{source}: {e}"))?;
        let doc: toml::Value = text.parse().map_err(|e| format!("{source}: {e}"))?;
        for g in doc.get("git").and_then(|g| g.as_array()).into_iter().flatten() {
            let strip = g.get("strip_prefix").and_then(|s| s.as_str()).unwrap_or("");
            for f in g.get("file").and_then(|f| f.as_array()).into_iter().flatten() {
                let rel = f.get("path").and_then(|p| p.as_str()).unwrap_or_default();
                let tags: Vec<String> =
                    f.get("tags").and_then(|t| t.as_array()).into_iter().flatten().filter_map(|t| t.as_str().map(String::from)).collect();
                let ext = rel.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
                let Some(app) = app_for(&ext) else { continue };
                if !tags.iter().any(|t| t == "ooxml") || tags.iter().any(|t| t == "encrypted" || t == "not-zip") {
                    continue;
                }
                let path = Path::new(corpus).join(source).join(rel.strip_prefix(strip).unwrap_or(rel));
                let Ok(md) = std::fs::metadata(&path) else { continue };
                if md.len() > MAX_BYTES || md.len() == 0 {
                    continue;
                }
                let idx = ["docx", "xlsx", "pptx"].iter().position(|e| *e == ext).unwrap();
                by_ext[idx].push(Entry {
                    id: String::new(),
                    source: source.into(),
                    rel: rel.into(),
                    path: path.to_string_lossy().into(),
                    ext,
                    app: app.into(),
                    size: md.len(),
                    tags,
                    stratum: String::new(),
                });
            }
        }
    }
    let mut chosen: Vec<Entry> = Vec::new();
    for (i, pool) in by_ext.iter_mut().enumerate() {
        pool.sort_by(|a, b| (&a.source, &a.rel).cmp(&(&b.source, &b.rel)));
        let mut rng = Rng(seed ^ (i as u64 + 1).wrapping_mul(0x1000_0000_0000_01B3));
        for k in (1..pool.len()).rev() {
            pool.swap(k, (rng.next() % (k as u64 + 1)) as usize);
        }
        let want = quotas[i];
        let has_focus = |e: &Entry| FOCUS_TAGS.iter().any(|t| e.tags.iter().any(|x| x == t));
        // Strata: each focus tag plus "none" (no focus tag). Rarest first, equal quota, shortfall filled at the end.
        let mut strata: Vec<(String, usize)> =
            FOCUS_TAGS.iter().map(|t| (t.to_string(), pool.iter().filter(|e| e.tags.iter().any(|x| x == t)).count())).collect();
        strata.push(("none".into(), pool.iter().filter(|e| !has_focus(e)).count()));
        strata.sort_by_key(|s| s.1);
        let quota = want.div_ceil(strata.len());
        let mut taken = vec![false; pool.len()];
        let mut picked: Vec<(usize, String)> = Vec::new();
        for (name, _) in &strata {
            let mut got = 0;
            for (k, e) in pool.iter().enumerate() {
                if got >= quota || picked.len() >= want {
                    break;
                }
                let hit = if name == "none" { !has_focus(e) } else { e.tags.iter().any(|x| x == name) };
                if hit && !taken[k] {
                    taken[k] = true;
                    got += 1;
                    picked.push((k, name.clone()));
                }
            }
        }
        for k in 0..pool.len() {
            if picked.len() >= want {
                break;
            }
            if !taken[k] {
                taken[k] = true;
                picked.push((k, "fill".into()));
            }
        }
        picked.sort();
        for (k, s) in picked {
            let mut e = pool[k].clone();
            e.stratum = s;
            e.id = format!("{}-{:03}", e.ext, chosen.iter().filter(|c| c.ext == e.ext).count());
            chosen.push(e);
        }
    }
    std::fs::write(out, serde_json::to_string_pretty(&chosen).unwrap()).map_err(|e| e.to_string())?;
    eprintln!("sampled {} files (seed {seed}) -> {out}", chosen.len());
    Ok(())
}
