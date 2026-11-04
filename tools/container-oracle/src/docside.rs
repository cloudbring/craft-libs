//! craft-doc (#120): the crate one commit before its zip swap (`craft_doc_old`) against the
//! workspace crate, through the public `sniff` / `info` / `renditions` / `outline` API.

use std::hash::{Hash, Hasher};

use craft_compare::Verdict;

#[path = "../../../crates/doc/tests/common/mod.rs"]
pub mod common;

pub fn sample_pcraft() -> Vec<u8> {
    common::pcraft(&common::png(3, 3, 1), &common::png(6, 6, 2))
}

/// A `.pcraft` bundle carrying the first entries of a corpus archive as payload (`blobs/<name>`):
/// real compressed data and real names inside a structure craft-doc accepts.
pub fn wrap_pcraft(corpus_zip: &[u8]) -> Result<Vec<u8>, String> {
    let src = crate::zipside::source_entries(corpus_zip)?;
    let thumb = common::png(3, 3, 1);
    let composite = common::png(6, 6, 2);
    let names: Vec<String> = src.iter().take(16).map(|(n, _)| format!("blobs/{n}")).collect();
    let mut entries: Vec<common::Entry> = Vec::new();
    for (name, (_, data)) in names.iter().zip(src.iter()) {
        if data.len() <= 2 << 20 {
            entries.push(if data.len() > 64 { common::deflated(name, data) } else { common::store(name, data) });
        }
    }
    entries.push(common::store("thumb.png", &thumb));
    entries.push(common::deflated("composite/preview.png", &composite));
    let layers = format!("[{}]", common::pcraft_layer("Background", true, "Normal", 1.0, common::raster()));
    let manifest = common::pcraft_manifest(1, &layers, r#","thumbnail":"thumb.png","composite":"composite/preview.png""#);
    entries.push(common::deflated("manifest.json", manifest.as_bytes()));
    Ok(common::zip(&entries))
}

#[derive(Clone, Debug, PartialEq)]
pub struct DocObs {
    /// Calls and their results; errors reduced to their variant.
    pub text: String,
    /// The same with full error messages.
    pub msgs: String,
}

fn variant(s: &str) -> String {
    s.split(['(', ' ', '{']).next().unwrap_or("").to_string()
}

macro_rules! doc_side {
    ($fn:ident, $krate:ident) => {
        pub fn $fn(bytes: &[u8]) -> Result<DocObs, String> {
            use $krate::{Limits, info, outline, renditions, sniff};
            let mut l = Limits::default();
            l.max_json = 4 << 20;
            l.max_entry = 16 << 20;
            l.max_total = 32 << 20;
            let (mut text, mut msgs) = (format!("sniff {:?}\n", sniff(bytes)), String::new());
            fn put<T: std::fmt::Debug, E: std::fmt::Debug + std::fmt::Display>(label: &str, r: Result<T, E>, text: &mut String, msgs: &mut String) {
                match r {
                    Ok(v) => {
                        let s = format!("{v:?}");
                        let mut h = std::collections::hash_map::DefaultHasher::new();
                        s.hash(&mut h);
                        text.push_str(&format!("{label} ok {} {:x}\n", s.len(), h.finish()));
                    }
                    Err(e) => {
                        text.push_str(&format!("{label} err {}\n", variant(&format!("{e:?}"))));
                        msgs.push_str(&format!("{label}: {e}\n"));
                    }
                }
            }
            put("info", info(bytes, &l), &mut text, &mut msgs);
            put("renditions", renditions(bytes, &l), &mut text, &mut msgs);
            put("outline", outline(bytes, &l), &mut text, &mut msgs);
            Ok(DocObs { text, msgs })
        }
    };
}
pub mod old {
    use super::*;
    doc_side!(run, craft_doc_old);
}
pub mod new {
    use super::*;
    doc_side!(run, craft_doc);
}

/// `text` differs -> Exceeds (by which call); only the error wording differs -> Within.
pub fn judge(old: &DocObs, new: &DocObs) -> Verdict {
    if old.text != new.text {
        let mut classes = std::collections::BTreeSet::new();
        for (a, b) in old.text.lines().zip(new.text.lines()) {
            if a != b {
                let call = a.split(' ').next().unwrap_or("");
                let kind = match (a.contains(" ok "), b.contains(" ok ")) {
                    (true, true) => "result",
                    (true, false) => "old_ok_new_err",
                    (false, true) => "old_err_new_ok",
                    (false, false) => "err_variant",
                };
                classes.insert(format!("{call}_{kind}"));
            }
        }
        return Verdict::Exceeds { metric: classes.into_iter().collect::<Vec<_>>().join("+"), value: 1.0, limit: 0.0 };
    }
    if old.msgs != new.msgs {
        return Verdict::Within { metric: "error_message".into(), value: 1.0 };
    }
    Verdict::Identical
}
