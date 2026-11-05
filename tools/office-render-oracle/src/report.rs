//! Markdown tables from a results file, plus CI-gate baselines.

use crate::FOCUS_TAGS;
use crate::run::Row;
use std::fmt::Write;

pub const TEXT_F1: f64 = 0.90;
pub const SSIM: f64 = 0.80;
pub const LAYOUT: f64 = 80.0;
const APPS: [&str; 3] = ["wordcraft", "gridcraft", "deckcraft"];

fn pct(v: &mut [f64], q: f64) -> f64 {
    if v.is_empty() {
        return f64::NAN;
    }
    v.sort_by(f64::total_cmp);
    v[((v.len() - 1) as f64 * q).round() as usize]
}

fn rate(rows: &[&Row], f: impl Fn(&Row) -> bool) -> f64 {
    if rows.is_empty() { f64::NAN } else { 100.0 * rows.iter().filter(|r| f(r)).count() as f64 / rows.len() as f64 }
}

fn col(rows: &[&Row], f: impl Fn(&Row) -> f64) -> Vec<f64> {
    rows.iter().map(|r| f(r)).collect()
}

pub fn run(results: &str, baseline: Option<&str>) -> Result<(), String> {
    let rows: Vec<Row> = std::fs::read_to_string(results).map_err(|e| e.to_string())?.lines().filter_map(|l| serde_json::from_str(l).ok()).collect();
    let mut o = String::new();
    let mut base = serde_json::Map::new();
    writeln!(o, "### Outcomes\n\n| App | Sampled | ok | ours failed | reference failed | both failed |\n|---|---|---|---|---|---|").unwrap();
    for app in APPS {
        let a: Vec<&Row> = rows.iter().filter(|r| r.app == app).collect();
        let c = |s: &str| a.iter().filter(|r| r.status == s).count();
        writeln!(o, "| {app} | {} | {} | {} | {} | {} |", a.len(), c("ok"), c("ours_fail"), c("ref_fail"), c("both_fail")).unwrap();
    }
    writeln!(o, "\nAgreement thresholds: text = word F1 >= {TEXT_F1}; visual = mean SSIM >= {SSIM}; layout = matched-line % >= {LAYOUT}; pages = equal page count. Only files both sides rendered (status ok) enter the tables below.\n").unwrap();
    writeln!(o, "### Distributions (ok files)\n\n| App | Metric | n | p10 | p25 | median | p75 | p90 |\n|---|---|---|---|---|---|---|---|").unwrap();
    type M = (&'static str, fn(&Row) -> f64);
    let metrics: [M; 8] = [
        ("word F1", |r| r.word_f1),
        ("reading order", |r| r.order),
        ("SSIM mean", |r| r.ssim_mean),
        ("SSIM min page", |r| r.ssim_min),
        ("DeltaE mean", |r| r.de_mean),
        ("p99 abs err", |r| r.p99_mean),
        ("layout match %", |r| r.layout_pct),
        ("median dy (pt)", |r| r.dy_median),
    ];
    for app in APPS {
        let ok: Vec<&Row> = rows.iter().filter(|r| r.app == app && r.status == "ok").collect();
        for (name, f) in metrics {
            let mut v = col(&ok, f);
            writeln!(
                o,
                "| {app} | {name} | {} | {:.3} | {:.3} | {:.3} | {:.3} | {:.3} |",
                v.len(),
                pct(&mut v.clone(), 0.1),
                pct(&mut v.clone(), 0.25),
                pct(&mut v.clone(), 0.5),
                pct(&mut v.clone(), 0.75),
                pct(&mut v, 0.9)
            )
            .unwrap();
        }
        let mut m = serde_json::Map::new();
        let (tf, vs, lm, pm) = (
            rate(&ok, |r| r.word_f1 >= TEXT_F1),
            rate(&ok, |r| r.ssim_mean >= SSIM),
            rate(&ok, |r| r.layout_pct >= LAYOUT),
            rate(&ok, |r| r.pages_ref == r.pages_ours),
        );
        let total: Vec<&Row> = rows.iter().filter(|r| r.app == app).collect();
        m.insert("n_sampled".into(), total.len().into());
        m.insert("n_ok".into(), ok.len().into());
        // Gate values leave 5 points (rates) or 0.05 (medians) of slack below the measured baseline.
        let slack = |x: f64| ((x - 5.0).floor()).max(0.0);
        m.insert("min_ok_rate_pct".into(), slack(100.0 * ok.len() as f64 / total.len().max(1) as f64).into());
        m.insert("min_text_agree_pct".into(), slack(tf).into());
        m.insert("min_visual_agree_pct".into(), slack(vs).into());
        m.insert("min_layout_agree_pct".into(), slack(lm).into());
        m.insert("min_page_match_pct".into(), slack(pm).into());
        m.insert("min_median_word_f1".into(), (((pct(&mut col(&ok, |r| r.word_f1), 0.5) - 0.05) * 100.0).floor().max(0.0) / 100.0).into());
        m.insert("min_median_ssim".into(), (((pct(&mut col(&ok, |r| r.ssim_mean), 0.5) - 0.05) * 100.0).floor().max(0.0) / 100.0).into());
        base.insert(app.into(), m.into());
    }
    writeln!(o, "\n### Agreement rates per app (ok files)\n\n| App | n | text agree | visual agree | layout agree | page count equal | all four |\n|---|---|---|---|---|---|---|").unwrap();
    for app in APPS {
        let ok: Vec<&Row> = rows.iter().filter(|r| r.app == app && r.status == "ok").collect();
        writeln!(
            o,
            "| {app} | {} | {:.1}% | {:.1}% | {:.1}% | {:.1}% | {:.1}% |",
            ok.len(),
            rate(&ok, |r| r.word_f1 >= TEXT_F1),
            rate(&ok, |r| r.ssim_mean >= SSIM),
            rate(&ok, |r| r.layout_pct >= LAYOUT),
            rate(&ok, |r| r.pages_ref == r.pages_ours),
            rate(&ok, |r| r.word_f1 >= TEXT_F1 && r.ssim_mean >= SSIM && r.layout_pct >= LAYOUT && r.pages_ref == r.pages_ours)
        )
        .unwrap();
    }
    for app in APPS {
        writeln!(o, "\n### {app} per feature tag\n\nA file counts under every tag it carries. `ours fail` counts files where our export errored but LibreOffice rendered.\n\n| Tag | sampled | ok | ours fail | median F1 | median SSIM | text agree | visual agree | layout agree | pages equal |\n|---|---|---|---|---|---|---|---|---|---|").unwrap();
        let all: Vec<&Row> = rows.iter().filter(|r| r.app == app).collect();
        let mut groups: Vec<(String, Vec<&Row>)> =
            FOCUS_TAGS.iter().map(|t| (t.to_string(), all.iter().copied().filter(|r| r.tags.iter().any(|x| x == t)).collect())).collect();
        groups.push(("(no focus tag)".into(), all.iter().copied().filter(|r| !FOCUS_TAGS.iter().any(|t| r.tags.iter().any(|x| x == t))).collect()));
        groups.push(("all".into(), all.clone()));
        for (t, g) in groups {
            let ok: Vec<&Row> = g.iter().copied().filter(|r| r.status == "ok").collect();
            if g.is_empty() {
                continue;
            }
            writeln!(
                o,
                "| {t} | {} | {} | {} | {:.3} | {:.3} | {:.0}% | {:.0}% | {:.0}% | {:.0}% |",
                g.len(),
                ok.len(),
                g.iter().filter(|r| r.status == "ours_fail").count(),
                pct(&mut col(&ok, |r| r.word_f1), 0.5),
                pct(&mut col(&ok, |r| r.ssim_mean), 0.5),
                rate(&ok, |r| r.word_f1 >= TEXT_F1),
                rate(&ok, |r| r.ssim_mean >= SSIM),
                rate(&ok, |r| r.layout_pct >= LAYOUT),
                rate(&ok, |r| r.pages_ref == r.pages_ours)
            )
            .unwrap();
        }
    }
    writeln!(o, "\n### Failure reasons (ours failed, reference rendered)\n\n| App | Reason | Files |\n|---|---|---|").unwrap();
    for app in APPS {
        let mut c: std::collections::BTreeMap<String, usize> = Default::default();
        for r in rows.iter().filter(|r| r.app == app && r.status == "ours_fail") {
            let e = r.ours_err.clone().unwrap_or_default();
            let key: String = e.chars().filter(|c| !c.is_ascii_digit() || e.starts_with("timeout")).take(110).collect();
            *c.entry(key).or_default() += 1;
        }
        let mut v: Vec<_> = c.into_iter().collect();
        v.sort_by_key(|x| std::cmp::Reverse(x.1));
        for (k, n) in v.into_iter().take(8) {
            writeln!(o, "| {app} | `{}` | {n} |", k.replace('|', "/")).unwrap();
        }
    }
    print!("{o}");
    if let Some(p) = baseline {
        std::fs::write(p, serde_json::to_string_pretty(&serde_json::Value::Object(base)).unwrap() + "\n").map_err(|e| e.to_string())?;
    }
    Ok(())
}
