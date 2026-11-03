#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Differential tests against a verbatim copy of wordcraft's diff (the oracle under
//! tests/oracles/wordcraft.rs): cloudbring/wordcraft 4912b24b,
//! `crates/engine/src/cmd/tools/compare.rs`. The fixtures of its compare tests
//! (`crates/engine/src/cmd/tools/tests.rs`) are rebuilt as the token sequences its compare
//! feeds `lcs_ops` — paragraph texts at block level (`String`s), words with their trailing
//! spaces at word level, `char`s at character level — and `craft_diff::diff` must produce
//! exactly the same ops, token for token, plus the assertions its own tests make. One
//! divergence is deliberate and asserted: the app shares one work budget across a document's
//! pairs (pairs past it replaced whole); the crate budgets each call, so every pair diffs.

#[allow(dead_code)]
#[rustfmt::skip]
#[path = "oracles/wordcraft.rs"]
mod wordcraft;

use std::hash::Hash;
use std::ops::Range;

use craft_diff::{Budget, Op};

/// Block-level tokens: each paragraph's plain text, as the app's `key` builds it.
fn paras(text: &str) -> Vec<String> {
    text.split('\n').map(str::to_string).collect()
}

/// Word-level tokens: words with the spaces after them, as the app's `tokens` builds them
/// ("The ", "cat ", "sat.").
fn words(text: &str) -> Vec<String> {
    text.split_inclusive(' ').map(str::to_string).collect()
}

fn chars(text: &str) -> Vec<char> {
    text.chars().collect()
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
/// grouping the app's compare makes when it walks a run of deletions and insertions.
fn grouped(steps: &[wordcraft::Op]) -> Vec<(Kind, Range<usize>, Range<usize>)> {
    let (mut i, mut j, mut k) = (0usize, 0usize, 0usize);
    let mut out = Vec::new();
    while k < steps.len() {
        if steps[k] == wordcraft::Op::Same {
            let len = steps[k..].iter().take_while(|&&s| s == wordcraft::Op::Same).count();
            out.push((Kind::Equal, i..i + len, j..j + len));
            i += len;
            j += len;
            k += len;
            continue;
        }
        let (old_start, new_start) = (i, j);
        while k < steps.len() && steps[k] != wordcraft::Op::Same {
            match steps[k] {
                wordcraft::Op::Del => i += 1,
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

/// craft-diff must produce exactly wordcraft's `lcs_ops` ops on `a` → `b` (a fresh
/// `MAX_COMPARE_CELLS` budget per call, as the crate budgets each call).
fn assert_same<T: wordcraft::DiffKey + Hash + Eq>(a: &[T], b: &[T]) -> Vec<Op> {
    let mut budget = wordcraft::MAX_COMPARE_CELLS;
    let reference = wordcraft::lcs_ops(a, b, &mut budget);
    let ops = craft_diff::diff(a, b, &Budget::APPS);
    assert_eq!(grouped(&reference), ours(&ops), "craft-diff's ops differ from wordcraft's lcs_ops");
    ops
}

/// The diff-quality metric of #53: how many tokens the ops flag as changed.
fn changed(ops: &[Op]) -> usize {
    ops.iter()
        .map(|op| match op {
            Op::Equal { .. } => 0,
            _ => ranges(op).0.len() + ranges(op).1.len(),
        })
        .sum()
}

#[test]
fn compare_marks_changes() {
    let ops = assert_same(&paras("The cat sat.\nKeep this.\nOld line."), &paras("The dog sat.\nKeep this.\nNew line here."));
    assert_eq!(ops.iter().filter(|op| !matches!(op, Op::Equal { .. })).count(), 2);
}

#[test]
fn compare_options_and_levels() {
    // Case and double-space differences count at block level: two changed paragraphs, as the
    // app's run pairing counts them (one Replace op of two pairs).
    assert_eq!(doc_diff("The Cat sat.\nTwo  spaces here.", "The cat sat.\nTwo spaces here.").0, 2);
    // Character level marks only the changed letters: "colour" → "color" is the 'u'.
    let ops = assert_same(&chars("colour"), &chars("color"));
    let delete = ops
        .iter()
        .find_map(|op| match op {
            Op::Delete { old, .. } => Some(old.clone()),
            _ => None,
        })
        .unwrap();
    assert_eq!(delete, 4..5, "only the 'u' is deleted");
}

#[test]
fn collapse_noise_matches_merges_short_interior_runs_but_keeps_prefix_suffix() {
    // wordcraft's own test: "01.09" vs "19.10" — the raw script finds spurious single-character
    // matches, and the app folds them when rendering (collapse_noise_matches stays in wordcraft;
    // the crate's ops match the raw script exactly). A plain single-character replacement
    // flanked by a real common prefix and suffix is untouched.
    let ops = assert_same(&chars("01.09"), &chars("19.10"));
    assert!(ops.iter().any(|op| matches!(op, Op::Equal { .. })), "the raw script does find spurious matches here, or this test proves nothing");
    let mut budget = wordcraft::MAX_COMPARE_CELLS;
    let raw = wordcraft::lcs_ops(&chars("01.09"), &chars("19.10"), &mut budget);
    let collapsed = wordcraft::collapse_noise_matches(&raw);
    assert_eq!(collapsed.len(), 10, "five deletions, five insertions — the app's clean replacement");
    let ops = assert_same(&chars("abcxdef"), &chars("abcydef"));
    let mut budget = wordcraft::MAX_COMPARE_CELLS;
    let raw = wordcraft::lcs_ops(&chars("abcxdef"), &chars("abcydef"), &mut budget);
    assert_eq!(wordcraft::collapse_noise_matches(&raw), raw, "a genuine interior replacement has no noise to fold");
    assert_eq!(ops, [Op::Equal { old: 0..3, new: 0..3 }, Op::Replace { old: 3..4, new: 3..4 }, Op::Equal { old: 4..7, new: 4..7 }]);
}

#[test]
fn compare_at_word_level_still_matches_a_whole_unchanged_word() {
    // A single matched *word* between two edits is a real, meaningful match (unlike a single
    // matched character) and must survive as its own Equal op.
    let (a, b) = (words("the cat sat"), words("a cat ran"));
    let ops = assert_same(&a, &b);
    let cat = ops
        .iter()
        .find_map(|op| match op {
            Op::Equal { old, .. } if old.len() == 1 && a[old.start] == "cat " => Some(old.clone()),
            _ => None,
        })
        .unwrap();
    assert_eq!(cat, 1..2);
    assert_eq!(changed(&ops), 4);
}

#[test]
fn compare_bounds_total_diff_work() {
    // The budget fixture: many long, slightly different paragraphs. Each pair alone fits one
    // comparison, so with a budget per call (the crate's contract) every pair is diffed word
    // by word — where the app, sharing its 50M budget across the document's pairs, replaces
    // the later ones whole. That sharing is the app's to keep: it passes smaller per-call
    // work budgets, or budgets its calls, itself.
    let words_per_para = 3000;
    let paras_n = 12;
    let doc = |rev: bool| {
        (0..paras_n)
            .map(|p| (0..words_per_para).map(|w| format!("{}{}w{}", if w % 10 == 0 && rev { 'x' } else { 'p' }, p, w)).collect::<Vec<_>>().join(" "))
            .collect::<Vec<_>>()
    };
    let (original, revised) = (doc(false), doc(true));
    let start = std::time::Instant::now();
    for (a, b) in original.iter().zip(&revised) {
        let (wa, wb) = (words(a), words(b));
        let ops = assert_same(&wa, &wb);
        assert_eq!(changed(&ops), 600, "every 10th word changed: 300 deleted, 300 inserted");
    }
    eprintln!("{} pairs of {} words: {:?}", paras_n, words_per_para, start.elapsed());
}

/// #53's diff-quality metric over a document: paragraphs paired at block level, changed
/// pairs diffed word by word (the app's `word_diff`), unpaired paragraphs counted whole —
/// (changes, changed words), exactly as its tests count them.
fn doc_diff(old: &str, new: &str) -> (usize, usize) {
    let (a, b) = (paras(old), paras(new));
    let ops = assert_same(&a, &b);
    let (mut changes, mut changed_words) = (0usize, 0usize);
    for op in &ops {
        let (o, n) = (ranges(op).0.clone(), ranges(op).1.clone());
        if matches!(op, Op::Equal { .. }) {
            continue;
        }
        changes += o.len().max(n.len());
        let paired = o.len().min(n.len());
        for x in 0..paired {
            changed_words += changed(&assert_same(&words(&a[o.start + x]), &words(&b[n.start + x])));
        }
        for p in &a[o.start + paired..o.end] {
            changed_words += p.split_whitespace().count();
        }
        for p in &b[n.start + paired..n.end] {
            changed_words += p.split_whitespace().count();
        }
    }
    (changes, changed_words)
}

/// #53: the anchored diff must not enlarge the diff on the existing fixtures.
#[test]
fn compare_fixture_changed_word_counts_hold() {
    let cases = [
        ("one word replaced", "The cat sat.", "The dog sat.", 1, 2),
        ("a line replaced", "The cat sat.\nKeep this.\nOld line.", "The dog sat.\nKeep this.\nNew line here.", 2, 7),
        ("deleted and inserted paragraphs", "alpha\nbeta\ngamma\ndelta", "alpha\ngamma\ndelta epsilon", 2, 4),
        ("a whole line replaced", "a b c", "x y z", 1, 6),
        ("a matched word between edits", "the cat sat", "a cat ran", 1, 4),
    ];
    for (name, original, revised, changes, words) in cases {
        assert_eq!(doc_diff(original, revised), (changes, words), "{name}");
    }
}

/// #53: two 10,000-paragraph documents over a 50-word vocabulary, 20 scattered single-word
/// changes. No paragraph is unique in either document, so there is nothing to patience-anchor
/// on; the anchored diff keeps exactly the 20 changed paragraphs.
#[test]
fn compare_repetitive_vocabulary_keeps_scattered_edits() {
    let vocab: Vec<String> = (0..50).map(|i| format!("term{i}")).collect();
    let old: Vec<String> = (0..10_000).map(|i| vocab[i % 50].clone()).collect();
    let mut new = old.clone();
    for k in 0..20 {
        new[199 + 487 * k] = vocab[(199 + 487 * k + 7) % 50].clone();
    }
    let start = std::time::Instant::now();
    let ops = assert_same(&old, &new);
    eprintln!(
        "10,000-paragraph pair: {} changes, {} changed words, {:?}",
        ops.iter().filter(|op| !matches!(op, Op::Equal { .. })).count(),
        changed(&ops),
        start.elapsed()
    );
    assert_eq!(ops.iter().filter(|op| matches!(op, Op::Replace { .. })).count(), 20, "the matching middle was replaced wholesale");
    assert!(changed(&ops) <= 40, "{} changed words", changed(&ops));
}

/// #53: the worst shapes at 50,000 paragraphs a side — repeated vocabulary with scattered
/// edits, a block swap of repeated paragraphs, and two documents with nothing in common — all
/// finish bounded and fast, with no quadratic table above the frontier bound (asserted
/// against the peak counter in craft-diff's own unit tests).
#[test]
fn compare_fifty_thousand_paragraph_pairs_stay_bounded() {
    let start = std::time::Instant::now();
    let vocab: Vec<String> = (0..50).map(|i| format!("w{i}")).collect();
    let old: Vec<String> = (0..50_000).map(|i| vocab[i % 50].clone()).collect();
    let mut new = old.clone();
    for k in 0..100 {
        new[101 + 431 * k] = vocab[(101 + 431 * k + 11) % 50].clone();
    }
    let ops = assert_same(&old, &new);
    eprintln!("50,000-paragraph pair: {} changes, {} changed words", ops.iter().filter(|op| !matches!(op, Op::Equal { .. })).count(), changed(&ops));
    assert!(changed(&ops) <= 200, "{} changed words", changed(&ops));

    let block = |first: &str, second: &str| (0..50_000).map(|i| if i < 25_000 { first } else { second }.to_string()).collect::<Vec<_>>();
    // A block swap of repeated paragraphs has no monotone match beyond one half.
    assert_eq!(changed(&assert_same(&block("alpha", "beta"), &block("beta", "alpha"))), 50_000);

    let old: Vec<String> = (0..50_000).map(|i| format!("x{i}")).collect();
    let new: Vec<String> = (0..50_000).map(|i| format!("y{i}")).collect();
    assert_eq!(assert_same(&old, &new).len(), 1, "nothing in common: one replacement");

    let elapsed = start.elapsed();
    eprintln!("50,000-paragraph pairs: {elapsed:?}");
    assert!(elapsed < std::time::Duration::from_secs(30), "{elapsed:?}");
}
