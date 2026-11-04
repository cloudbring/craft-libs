//! Old-vs-new differentials for the craft-archive zip swap (#120) and the craft-cfb swap (#123).
//!
//! ```text
//! ARTCRAFT_CORPUS=/Users/emm/ArtCraft/corpus CARGO_TARGET_DIR=... \
//!   cargo test --release --manifest-path tools/container-oracle/Cargo.toml -- --ignored --nocapture
//! ```
//!
//! Output (report.md, details.txt, report.json, failures/) goes to `CONTAINER_ORACLE_OUT`
//! (default `target/container-oracle`). `hostile_inputs_*` need no corpus and run by default.

use std::path::{Path, PathBuf};

use container_oracle::cfbside::*;
use container_oracle::docside;
use container_oracle::harness::*;
use container_oracle::hostile;
use container_oracle::zipside::*;
use craft_oracle::Registry;

type Inputs = Vec<(String, Vec<u8>)>;

fn registry() -> Registry {
    Registry::from_path(&Path::new(env!("CARGO_MANIFEST_DIR")).join("expected.toml")).expect("expected.toml")
}

fn out_dir() -> PathBuf {
    PathBuf::from(std::env::var_os("CONTAINER_ORACLE_OUT").unwrap_or_else(|| "target/container-oracle".into()))
}

// -- legacy-archive wrappers: the reader sees what the pre-swap writer produced from the same entries.
fn vc_legacy_store_old(b: &[u8]) -> Result<Res<VcRead>, String> {
    vectorcraft_read::old(&legacy::vectorcraft_store(b)?)
}
fn vc_legacy_store_new(b: &[u8]) -> Result<Res<VcRead>, String> {
    vectorcraft_read::new(&legacy::vectorcraft_store(b)?)
}
fn vc_legacy_deflate_old(b: &[u8]) -> Result<Res<VcRead>, String> {
    vectorcraft_read::old(&legacy::vectorcraft_deflate(b)?)
}
fn vc_legacy_deflate_new(b: &[u8]) -> Result<Res<VcRead>, String> {
    vectorcraft_read::new(&legacy::vectorcraft_deflate(b)?)
}
fn ec_legacy_old(b: &[u8]) -> Result<Res<Entries>, String> {
    effectcraft_read::old(&legacy::effectcraft_store(b)?)
}
fn ec_legacy_new(b: &[u8]) -> Result<Res<Entries>, String> {
    effectcraft_read::new(&legacy::effectcraft_store(b)?)
}
fn doc_wrapped_old(b: &[u8]) -> Result<docside::DocObs, String> {
    docside::old::run(&docside::wrap_pcraft(b)?)
}
fn doc_wrapped_new(b: &[u8]) -> Result<docside::DocObs, String> {
    docside::new::run(&docside::wrap_pcraft(b)?)
}

fn zip_scenarios(suite: &mut Suite, tag: &str, inputs: impl Fn() -> Inputs) {
    let n = |s: &str| format!("{s}{tag}");
    suite.run(&n("pdfcraft-write"), "pdfcraft", "zip", inputs(), pdfcraft::old, pdfcraft::new, judge_write_l);
    suite.run(&n("effectcraft-write"), "effectcraft", "zip", inputs(), effectcraft_write::old, effectcraft_write::new, judge_write_l);
    suite.run(&n("vectorcraft-write-store"), "vectorcraft", "zip", inputs(), vectorcraft_write::old_store, vectorcraft_write::new_store, judge_write_l);
    suite.run(&n("vectorcraft-write-deflate"), "vectorcraft", "zip", inputs(), vectorcraft_write::old_deflate, vectorcraft_write::new_deflate, judge_write_l);
    suite.run(&n("vectorcraft-read"), "vectorcraft", "zip", inputs(), vectorcraft_read::old, vectorcraft_read::new, judge_vc_l);
    suite.run(&n("effectcraft-read"), "effectcraft", "zip", inputs(), effectcraft_read::old, effectcraft_read::new, judge_entries_l);
    suite.run(&n("vectorcraft-read-legacy-store"), "vectorcraft", "zip", inputs(), vc_legacy_store_old, vc_legacy_store_new, judge_vc_l);
    suite.run(&n("vectorcraft-read-legacy-deflate"), "vectorcraft", "zip", inputs(), vc_legacy_deflate_old, vc_legacy_deflate_new, judge_vc_l);
    suite.run(&n("effectcraft-read-legacy-store"), "effectcraft", "zip", inputs(), ec_legacy_old, ec_legacy_new, judge_entries_l);
    suite.run(&n("craftdoc-raw"), "craft-doc", "zip", inputs(), docside::old::run, docside::new::run, docside::judge);
}

fn cfb_scenarios(suite: &mut Suite, tag: &str, inputs: impl Fn() -> Inputs) {
    let n = |s: &str| format!("{s}{tag}");
    suite.run(&n("gridcraft-sniff"), "gridcraft", "cfb", inputs(), gridcraft::sniff_old, gridcraft::sniff_new, judge_bool);
    suite.run(&n("gridcraft-is-xls"), "gridcraft", "cfb", inputs(), gridcraft::is_xls_old, gridcraft::is_xls_new, judge_bool);
    suite.run(&n("gridcraft-streams"), "gridcraft", "cfb", inputs(), gridcraft::streams_old, gridcraft::streams_new, judge_streams_l);
    suite.run(&n("deckcraft-sniff"), "deckcraft", "cfb", inputs(), deckcraft::sniff_old, deckcraft::sniff_new, judge_bool);
    suite.run(&n("wordcraft-sniff"), "wordcraft", "cfb", inputs(), wordcraft::sniff_old, wordcraft::sniff_new, judge_bool);
    suite.run(&n("wordcraft-streams"), "wordcraft", "cfb", inputs(), wordcraft::streams_old, wordcraft::streams_new, judge_streams_l);
    suite.run(&n("filmcraft-tree"), "filmcraft", "cfb", inputs(), filmcraft::old, filmcraft::new, judge_film_l);
    suite.run(&n("reference-vs-craft-cfb"), "reference", "cfb", inputs(), tree_reference, tree_craft_cfb, judge_tree_l);
}

fn hostile_zip_inputs() -> Inputs {
    static C: std::sync::OnceLock<Inputs> = std::sync::OnceLock::new();
    C.get_or_init(|| hostile::zip_cases(&hostile::base_zip(), true)).clone()
}
fn hostile_cfb_inputs() -> Inputs {
    static C: std::sync::OnceLock<Inputs> = std::sync::OnceLock::new();
    C.get_or_init(hostile::cfb_cases).clone()
}

fn run_hostile(suite: &mut Suite) {
    // Cases are generated once per scenario (cheap, and the bombs are not held in memory between runs).
    zip_scenarios(suite, ".hostile", hostile_zip_inputs);
    suite.run(
        "craftdoc-pcraft.hostile",
        "craft-doc",
        "zip",
        hostile::zip_cases(&hostile::pcraft_base(), false),
        docside::old::run,
        docside::new::run,
        docside::judge,
    );
    cfb_scenarios(suite, ".hostile", hostile_cfb_inputs);
}

fn finish(suite: &Suite, what: &str) {
    suite.write();
    eprintln!("{}", suite.markdown());
    assert_eq!(suite.failures(), 0, "{what}: unexpected differences; see {}", suite.out.join("details.txt").display());
}

#[test]
fn hostile_inputs_old_and_new_agree_or_are_registered() {
    let mut suite = Suite::new(registry(), out_dir().join("hostile"));
    run_hostile(&mut suite);
    finish(&suite, "hostile");
}

#[test]
#[ignore = "needs ARTCRAFT_CORPUS (the corpus root holding the office-* trees)"]
fn old_and_new_agree_on_the_office_corpus_and_hostile_inputs() {
    let Some(root) = std::env::var_os("ARTCRAFT_CORPUS") else {
        eprintln!("ARTCRAFT_CORPUS is not set; nothing to compare");
        return;
    };
    let files = discover(Path::new(&root));
    eprintln!("corpus: {} zip, {} cfb, {} other", files.zip.len(), files.cfb.len(), files.other.len());
    assert!(!files.zip.is_empty() && !files.cfb.is_empty(), "no corpus files under {:?}", root);
    let mut suite = Suite::new(registry(), out_dir());
    // Zip scenarios over every PK file; the doc scenario also over bundles wrapped around each file.
    zip_scenarios(&mut suite, "", || read_lazy(&files.zip).collect());
    suite.run("craftdoc-wrapped", "craft-doc", "zip", read_lazy(&files.zip), doc_wrapped_old, doc_wrapped_new, docside::judge);
    cfb_scenarios(&mut suite, "", || read_lazy(&files.cfb).collect());
    run_hostile(&mut suite);
    let _ = std::fs::write(
        suite.out.join("corpus.txt"),
        format!("zip {}\ncfb {}\nother {}\n{}", files.zip.len(), files.cfb.len(), files.other.len(), files.other.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>().join("\n")),
    );
    finish(&suite, "corpus");
}
