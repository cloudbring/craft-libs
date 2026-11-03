#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Differential tests against a verbatim copy of pdfcraft's word diff (the oracle under
//! tests/oracles/pdfcraft.rs): cloudbring/pdfcraft 974f4be4b, `crates/compare/src/lib.rs`.
//! Every fixture of its compare tests is rebuilt as the token sequence its `compare` builds —
//! one `u64` per word, hashed by its `hash` — and `craft_diff::diff` must produce exactly the
//! same ops, token for token, plus the assertions its own tests make.

#[allow(dead_code)]
#[rustfmt::skip]
#[path = "oracles/pdfcraft.rs"]
mod pdfcraft;

use std::ops::Range;

use craft_diff::{Budget, Op};

/// pdfcraft's tokens: its `compare` hashes each word with its `hash`.
fn tokens(text: &str) -> Vec<u64> {
    text.split_whitespace().map(pdfcraft::hash).collect()
}

/// The shape both sides are compared in: kind and the two ranges.
#[derive(Debug, PartialEq, Eq)]
enum Kind {
    Equal,
    Insert,
    Delete,
    Replace,
}

fn ranges(op: &Op) -> (&Range<usize>, &Range<usize>) {
    match op {
        Op::Equal { old, new } | Op::Insert { old, new } | Op::Delete { old, new } | Op::Replace { old, new } => (old, new),
    }
}

/// Groups the oracle's per-token script the way `craft_diff::diff` groups its ops — the same
/// grouping pdfcraft's `compare` makes when it turns the script into `Change`s.
fn grouped(steps: &[pdfcraft::Op]) -> Vec<(Kind, Range<usize>, Range<usize>)> {
    let (mut i, mut j, mut k) = (0usize, 0usize, 0usize);
    let mut out = Vec::new();
    while k < steps.len() {
        if steps[k] == pdfcraft::Op::Equal {
            let len = steps[k..].iter().take_while(|&&s| s == pdfcraft::Op::Equal).count();
            out.push((Kind::Equal, i..i + len, j..j + len));
            i += len;
            j += len;
            k += len;
            continue;
        }
        let (old_start, new_start) = (i, j);
        while k < steps.len() && steps[k] != pdfcraft::Op::Equal {
            match steps[k] {
                pdfcraft::Op::Delete => i += 1,
                _ => j += 1,
            }
            k += 1;
        }
        let kind = match (i == old_start, j == new_start) {
            (false, false) => Kind::Replace,
            (false, true) => Kind::Delete,
            _ => Kind::Insert,
        };
        out.push((kind, old_start..i, new_start..j));
    }
    out
}

fn ours(ops: &[Op]) -> Vec<(Kind, Range<usize>, Range<usize>)> {
    ops.iter()
        .map(|op| {
            (
                match op {
                    Op::Equal { .. } => Kind::Equal,
                    Op::Insert { .. } => Kind::Insert,
                    Op::Delete { .. } => Kind::Delete,
                    Op::Replace { .. } => Kind::Replace,
                },
                ranges(op).0.clone(),
                ranges(op).1.clone(),
            )
        })
        .collect()
}

/// craft-diff must produce exactly pdfcraft's ops on `a` → `b` (pdfcraft has no work budget,
/// so the budget is never the reason for a difference).
fn assert_same(a: &[u64], b: &[u64]) -> Vec<Op> {
    let mut reference = Vec::new();
    pdfcraft::diff(a, b, &mut reference, 0);
    let ops = craft_diff::diff(a, b, &Budget::APPS);
    assert_eq!(grouped(&reference), ours(&ops), "craft-diff's ops differ from pdfcraft's on {a:?} → {b:?}");
    ops
}

/// The diff-quality metric of pdfcraft's #54 tests: how many words the ops flag as changed.
fn changed_words(ops: &[Op]) -> usize {
    ops.iter()
        .map(|op| match op {
            Op::Equal { .. } => 0,
            _ => ranges(op).0.len() + ranges(op).1.len(),
        })
        .sum()
}

#[test]
fn identical_text_has_no_changes() {
    assert_eq!(assert_same(&tokens("the same words here"), &tokens("the same words here")), [Op::Equal { old: 0..4, new: 0..4 }]);
}

#[test]
fn insertions_deletions_and_replacements() {
    // pdfcraft's fox fixture: two replacements and two insertions.
    let ops = assert_same(&tokens("The quick brown fox jumps over the lazy dog"), &tokens("The quick red fox leaps over the very lazy dog today"));
    assert_eq!(ops.iter().filter(|op| matches!(op, Op::Replace { .. })).count(), 2);
    assert_eq!(ops.iter().filter(|op| matches!(op, Op::Insert { .. })).count(), 2);
    assert_eq!(changed_words(&ops), 6);
}

#[test]
fn deletions_across_pages_keep_their_words() {
    // The across-pages fixture's words (pages and rectangles are pdfcraft's rendering).
    let ops = assert_same(&tokens("alpha beta gamma delta epsilon"), &tokens("alpha gamma delta"));
    assert_eq!(ops.iter().filter(|op| matches!(op, Op::Delete { .. })).count(), 2, "beta and epsilon deleted");
    assert_eq!(changed_words(&ops), 2);
}

#[test]
fn one_line_replaced() {
    let ops = assert_same(&tokens("a b c"), &tokens("x y z"));
    assert_eq!(ops, [Op::Replace { old: 0..3, new: 0..3 }]);
}

#[test]
fn long_documents_with_scattered_edits() {
    // pdfcraft's 20,000-word fixture: four replacements and one deletion.
    let start = std::time::Instant::now();
    let old: Vec<u64> = (0..20_000).map(|i| pdfcraft::hash(&format!("w{i}"))).collect();
    let mut new = old.clone();
    for k in [100usize, 5000, 12_000, 19_000] {
        new[k] = pdfcraft::hash(&format!("changed{k}"));
    }
    new.remove(8000);
    let ops = assert_same(&old, &new);
    eprintln!("20,000-word pair: {:?}, {} changes", start.elapsed(), ops.iter().filter(|op| !matches!(op, Op::Equal { .. })).count());
    assert_eq!(ops.iter().filter(|op| !matches!(op, Op::Equal { .. })).count(), 5);
    assert_eq!(ops.iter().filter(|op| matches!(op, Op::Replace { .. })).count(), 4);
}

#[test]
fn very_different_documents_finish() {
    let start = std::time::Instant::now();
    let old: Vec<u64> = (0..30_000).map(|i| pdfcraft::hash(&format!("a{i}"))).collect();
    let new: Vec<u64> = (0..30_000).map(|i| pdfcraft::hash(&format!("b{i}"))).collect();
    let ops = assert_same(&old, &new);
    eprintln!("30,000-word disjoint pair: {:?}", start.elapsed());
    assert_eq!(ops, [Op::Replace { old: 0..30_000, new: 0..30_000 }]);
}

/// #54: 10,000 words over a 50-word vocabulary, 20 scattered replacements. No word is unique
/// in either document, so patience has no anchor and the whole middle used to collapse into
/// one replacement.
#[test]
fn repetitive_vocabulary_keeps_scattered_edits() {
    let start = std::time::Instant::now();
    let vocab: Vec<u64> = (0..50).map(|i| pdfcraft::hash(&format!("term{i}"))).collect();
    let old: Vec<u64> = (0..10_000).map(|i| vocab[i % 50]).collect();
    let mut new = old.clone();
    for k in 0..20 {
        new[199 + 487 * k] = vocab[(199 + 487 * k + 7) % 50];
    }
    let ops = assert_same(&old, &new);
    eprintln!(
        "10,000-word pair: {} changes, {} changed words, {:?}",
        ops.iter().filter(|op| !matches!(op, Op::Equal { .. })).count(),
        changed_words(&ops),
        start.elapsed()
    );
    assert!(changed_words(&ops) <= 40, "{} changed words", changed_words(&ops));
    assert_eq!(ops.iter().filter(|op| matches!(op, Op::Replace { .. })).count(), 20);
}

#[test]
fn fixture_changed_word_counts_hold() {
    // #54: the extra anchoring must not enlarge the diff on the existing fixtures.
    let contract = "The contract starts on Monday. Payment is due within thirty days. Signed by both parties.";
    let revised = "The contract starts on Tuesday. Payment is due within thirty days of invoice. Signed by both parties.";
    let cases = [
        ("identical", 0usize, "the same words here", "the same words here"),
        ("replacements", 6, "The quick brown fox jumps over the lazy dog", "The quick red fox leaps over the very lazy dog today"),
        ("across pages", 2, "alpha beta gamma delta epsilon", "alpha gamma delta"),
        ("one line replaced", 6, "a b c", "x y z"),
        ("contract", 6, contract, revised),
    ];
    for (name, expected, old, new) in cases {
        let ops = assert_same(&tokens(old), &tokens(new));
        assert_eq!(changed_words(&ops), expected, "{name}: {} changed words", changed_words(&ops));
    }
    // The two large fixtures, built as in their tests.
    let old: Vec<u64> = (0..20_000).map(|i| pdfcraft::hash(&format!("w{i}"))).collect();
    let mut new = old.clone();
    for k in [100usize, 5000, 12_000, 19_000] {
        new[k] = pdfcraft::hash(&format!("changed{k}"));
    }
    new.remove(8000);
    assert_eq!(changed_words(&assert_same(&old, &new)), 9, "scattered edits");
    let old: Vec<u64> = (0..30_000).map(|i| pdfcraft::hash(&format!("a{i}"))).collect();
    let new: Vec<u64> = (0..30_000).map(|i| pdfcraft::hash(&format!("b{i}"))).collect();
    assert_eq!(changed_words(&assert_same(&old, &new)), 60_000, "very different");
}

#[test]
fn fifty_thousand_word_pairs_stay_within_budget() {
    // #54: the worst shapes at 50,000 words a side — repeated vocabulary with scattered
    // edits (histogram anchors, then Myers per gap), a block swap of repeated words, and two
    // documents with nothing in common — all finish inside the budget. (The peak-table
    // assertion of its test lives in craft-diff's own unit tests, which watch the counter.)
    let start = std::time::Instant::now();
    let vocab: Vec<u64> = (0..50).map(|i| pdfcraft::hash(&format!("w{i}"))).collect();
    let old: Vec<u64> = (0..50_000).map(|i| vocab[i % 50]).collect();
    let mut new = old.clone();
    for k in 0..100 {
        new[101 + 431 * k] = vocab[(101 + 431 * k + 11) % 50];
    }
    let ops = assert_same(&old, &new);
    eprintln!("50,000-word pair: {} changes, {} changed words", ops.iter().filter(|op| !matches!(op, Op::Equal { .. })).count(), changed_words(&ops));
    assert!(changed_words(&ops) <= 200, "{} changed words", changed_words(&ops));

    let block = |first: &str, second: &str| (0..50_000).map(|i| pdfcraft::hash(if i < 25_000 { first } else { second })).collect::<Vec<_>>();
    // A block swap of repeated words has no monotone matches beyond one half: 50,000.
    assert_eq!(changed_words(&assert_same(&block("alpha", "beta"), &block("beta", "alpha"))), 50_000);

    let old: Vec<u64> = (0..50_000).map(|i| pdfcraft::hash(&format!("x{i}"))).collect();
    let new: Vec<u64> = (0..50_000).map(|i| pdfcraft::hash(&format!("y{i}"))).collect();
    assert_eq!(assert_same(&old, &new).len(), 1);

    let elapsed = start.elapsed();
    eprintln!("50,000-word pairs: {elapsed:?}");
    assert!(elapsed < std::time::Duration::from_secs(30), "{elapsed:?}");
}
