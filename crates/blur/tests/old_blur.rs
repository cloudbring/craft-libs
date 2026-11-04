//! Ticket cloudbring/artcraft-algorithms#203 (claim C13): effective sigma of each app's OLD blur
//! against craft-blur, from the second central moment of the impulse response.
//!
//! The old kernels are vendored under `old/` (header: repo, SHA, path). Where an old blur is the
//! Kovesi three-box cascade, its variance must equal the Lean-generated exact value for that
//! cascade (`vectors/lean-blur.csv`, kernel `box3`) and sit inside the Lean-proved bound
//! `|var/sigma^2 - 1| <= (sigma + 1) / (3 sigma^2)` (`CraftBlur.relative_error_le`).
//! The measured table is pinned in `vectors/blur-old-apps.csv`; regenerate it (after reviewing why
//! it changed) with `BLUR_BLESS=1 cargo test -p craft-blur --test old_blur`, and print the
//! Markdown table for docs/verification/blur-variance.md with `-- --nocapture`.

mod old;

use craft_blur::{Cancel, Edge, Gaussian, Quality, Scratch, Shape};

const SIGMAS: [f64; 8] = [0.5, 1.0, 2.0, 4.0, 8.0, 16.0, 32.0, 64.0];

struct App {
    name: &'static str,
    impulse: fn(f64) -> Vec<f64>,
    /// True where the app runs its three-box cascade (so the Lean box model applies).
    boxes: fn(f64) -> bool,
}

fn apps() -> Vec<App> {
    vec![
        App { name: "photocraft", impulse: old::photocraft::impulse, boxes: |s| s as f32 > 4.0 },
        App { name: "photocraft (fxutil)", impulse: old::photocraft::impulse_fxutil, boxes: |_| false },
        App { name: "lightcraft", impulse: old::lightcraft::impulse, boxes: |s| s > 0.3 },
        App { name: "effectcraft", impulse: old::effectcraft::impulse, boxes: |s| s >= 3.0 },
        App { name: "filmcraft", impulse: old::filmcraft::impulse, boxes: |s| s > 0.3 },
        App { name: "materialcraft", impulse: old::materialcraft::impulse, boxes: |s| s >= 3.0 },
        App { name: "vectorcraft", impulse: old::vectorcraft::impulse, boxes: |s| s >= 0.2 },
        App { name: "texturecraft (r=round(sigma))", impulse: old::texturecraft::impulse, boxes: |_| false },
    ]
}

fn new_variance(sigma: f64) -> f64 {
    let sigma = sigma as f32;
    let n = 2 * ((24.0 * sigma).ceil() as usize + 8) + 1;
    let mut line = vec![0.0_f32; n];
    line[n / 2] = 1.0;
    let gaussian = Gaussian::new(sigma, Quality::Recursive).expect("valid sigma");
    gaussian.filter_rows(&mut line, Shape::new(n, 1, 1).expect("shape"), Edge::Clamp, &mut Scratch::default(), Cancel::Never).expect("filter");
    old::variance(&line.iter().map(|v| f64::from(*v)).collect::<Vec<_>>())
}

fn lean_box_variance(sigma: f64) -> Option<f64> {
    include_str!("vectors/lean-blur.csv").lines().skip(1).find_map(|line| {
        let f: Vec<&str> = line.split(',').collect();
        let (n, d) = f[1].split_once('/')?;
        (f[0] == "box3" && n.parse::<f64>().ok()? / d.parse::<f64>().ok()? == sigma).then(|| f[5].parse().ok()).flatten()
    })
}

fn measured_rows() -> Vec<(String, f64, f64)> {
    let mut out = Vec::new();
    for app in apps() {
        for sigma in SIGMAS {
            out.push((app.name.to_string(), sigma, old::variance(&(app.impulse)(sigma))));
        }
    }
    for sigma in SIGMAS {
        out.push(("craft-blur".to_string(), sigma, new_variance(sigma)));
    }
    out
}

#[test]
fn old_blurs_against_lean_and_craft_blur() {
    let rows = measured_rows();
    let apps = apps();
    for (name, sigma, variance) in &rows {
        let relative = variance / (sigma * sigma) - 1.0;
        if name == "craft-blur" {
            assert!(relative.abs() < 1e-4, "craft-blur sigma={sigma}: relative variance error {relative:e}");
            continue;
        }
        let Some(app) = apps.iter().find(|a| a.name == name) else { continue };
        if !(app.boxes)(*sigma) {
            continue;
        }
        let lean = lean_box_variance(*sigma).expect("Lean row for every grid sigma");
        assert!((variance - lean).abs() < 1e-9, "{name} sigma={sigma}: old {variance} vs Lean box3 {lean}");
        let bound = (sigma + 1.0) / (3.0 * sigma * sigma);
        assert!(relative.abs() <= bound + 1e-12, "{name} sigma={sigma}: |{relative}| exceeds proved bound {bound}");
    }
}

#[test]
fn table_is_pinned_and_printed() {
    let rows = measured_rows();
    let mut csv = String::from("app,sigma,var\n");
    for (name, sigma, variance) in &rows {
        csv.push_str(&format!("{name},{sigma},{variance:.12}\n"));
    }
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/vectors/blur-old-apps.csv");
    if std::env::var_os("BLUR_BLESS").is_some() {
        std::fs::write(path, &csv).expect("write blessed table");
    }
    let pinned = std::fs::read_to_string(path).expect("tests/vectors/blur-old-apps.csv (run with BLUR_BLESS=1)");
    assert_eq!(pinned.lines().count(), csv.lines().count());
    for (a, b) in pinned.lines().zip(csv.lines()).skip(1) {
        let (a, b): (Vec<&str>, Vec<&str>) = (a.split(',').collect(), b.split(',').collect());
        assert_eq!((a[0], a[1]), (b[0], b[1]));
        let (x, y): (f64, f64) = (a[2].parse().expect("pinned"), b[2].parse().expect("measured"));
        assert!((x - y).abs() <= 1e-9 * x.max(1.0), "{} sigma={}: pinned {x} measured {y}", a[0], a[1]);
    }
    // Markdown table: sigma_eff / sigma - 1 (percent) per app and requested sigma.
    println!("| app | {} |", SIGMAS.map(|s| format!("{s}")).join(" | "));
    println!("|---|{}|", SIGMAS.map(|_| "---").join("|"));
    let mut names: Vec<&str> = rows.iter().map(|r| r.0.as_str()).collect();
    names.dedup();
    for name in names {
        let cells: Vec<String> = SIGMAS
            .iter()
            .map(|s| {
                let v = rows.iter().find(|r| r.0 == name && r.1 == *s).map_or(f64::NAN, |r| r.2);
                format!("{:.3} ({:+.1}%)", v.sqrt(), 100.0 * (v.sqrt() / s - 1.0))
            })
            .collect();
        println!("| {name} | {} |", cells.join(" | "));
    }
}
