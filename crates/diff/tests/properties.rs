#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Properties over random token sequences: the ops cover both sides exactly and in order and
//! applying them to `a` rebuilds `b`; the same inputs give the same ops on every call; and a
/// spent budget degrades to wholesale replacement without ever breaking either.
use std::ops::Range;

use craft_diff::{Budget, Op};
use proptest::prelude::*;

fn ranges(op: &Op) -> (&Range<usize>, &Range<usize>) {
    match op {
        Op::Equal { old, new } | Op::Insert { old, new } | Op::Delete { old, new } | Op::Replace { old, new } => (old, new),
    }
}

/// Applying the ops to `a` (keeping `Equal` tokens, taking `Insert`/`Replace` tokens from
/// `b`) must rebuild `b`, with the ranges covering both sides exactly and in order and every
/// `Equal` range holding equal tokens.
fn applies(a: &[u64], b: &[u64], ops: &[Op]) -> Result<(), TestCaseError> {
    let (mut i, mut j, mut got) = (0usize, 0usize, Vec::new());
    for op in ops {
        let (old, new) = (ranges(op).0.clone(), ranges(op).1.clone());
        prop_assert_eq!((old.start, new.start), (i, j), "{:?}: the ops must cover both sides in order", op);
        match op {
            Op::Equal { .. } => {
                prop_assert_eq!(a[old.clone()].to_vec(), b[new.clone()].to_vec(), "{:?}: an Equal range must hold equal tokens", op);
                got.extend_from_slice(&b[new.clone()]);
            }
            Op::Delete { .. } => {}
            Op::Insert { .. } | Op::Replace { .. } => got.extend_from_slice(&b[new.clone()]),
        }
        i = old.end;
        j = new.end;
    }
    prop_assert_eq!((i, j), (a.len(), b.len()), "the ops must cover both sides exactly");
    prop_assert_eq!(got, b, "applying the ops to a must rebuild b");
    Ok(())
}

/// Two random token sequences over a shared vocabulary — vocabularies from a single repeated
/// token to thousands of distinct ones exercise the histogram and patience anchors alike.
fn pair() -> BoxedStrategy<(Vec<u64>, Vec<u64>)> {
    (1u64..=1_000u64, proptest::collection::vec(0u64..6, 0..120), proptest::collection::vec(0u64..1_000, 0..120))
        .prop_map(|(spread, a, b)| {
            let scale = |t: u64| t % spread.max(1);
            (a.into_iter().map(scale).collect::<Vec<_>>(), b.into_iter().map(scale).collect::<Vec<_>>())
        })
        .boxed()
}

proptest! {
    /// Ops apply for any pair, under the apps' budget, a tight budget, and a spent one.
    #[test]
    fn ops_rebuild_b((a, b) in pair()) {
        for budget in [Budget::APPS, Budget::new(37, 2, 5_000), Budget::new(0, 0, 0)] {
            let ops = craft_diff::diff(&a, &b, &budget);
            applies(&a, &b, &ops)?;
        }
    }

    /// The same inputs give the same ops on every call, whatever the budget.
    #[test]
    fn same_input_same_ops((a, b) in pair()) {
        for budget in [Budget::APPS, Budget::new(37, 2, 5_000), Budget::new(0, 0, 0)] {
            prop_assert_eq!(craft_diff::diff(&a, &b, &budget), craft_diff::diff(&a, &b, &budget));
        }
    }

    /// A spent budget falls back to replacing the middle wholesale, edges kept.
    #[test]
    fn spent_budget_replaces_the_middle((a, b) in pair()) {
        let ops = craft_diff::diff(&a, &b, &Budget::new(0, 0, 0));
        let pre = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
        let suf = a[pre..].iter().rev().zip(b[pre..].iter().rev()).take_while(|(x, y)| x == y).count();
        let mut want = Vec::new();
        if pre > 0 {
            want.push(Op::Equal { old: 0..pre, new: 0..pre });
        }
        if a.len() - suf > pre || b.len() - suf > pre {
            let (o, n) = (pre..a.len() - suf, pre..b.len() - suf);
            let op = if o.is_empty() {
                Op::Insert { old: o, new: n }
            } else if n.is_empty() {
                Op::Delete { old: o, new: n }
            } else {
                Op::Replace { old: o, new: n }
            };
            want.push(op);
        }
        if suf > 0 {
            want.push(Op::Equal { old: a.len() - suf..a.len(), new: b.len() - suf..b.len() });
        }
        prop_assert_eq!(ops, want);
    }

    /// Identical sides diff to one Equal op, whatever the budget.
    #[test]
    fn identical_sides_are_one_equal((a, _) in pair()) {
        for budget in [Budget::APPS, Budget::new(0, 0, 0)] {
            let want: Vec<Op> = if a.is_empty() { Vec::new() } else { vec![Op::Equal { old: 0..a.len(), new: 0..a.len() }] };
            prop_assert_eq!(craft_diff::diff(&a, &a, &budget), want);
        }
    }

    /// The apps' shape at scale: a long repeated vocabulary with scattered edits stays exact
    /// (and cheap) under the apps' budget.
    #[test]
    fn scattered_edits_stay_scattered(seed in any::<u64>()) {
        let vocab = 40usize;
        let len = 4_000usize;
        let a: Vec<u64> = (0..len).map(|i| (i % vocab) as u64).collect();
        let mut b = a.clone();
        let mut at = (seed % 997) as usize;
        let mut edits = 0usize;
        while at < len {
            b[at] = 1_000 + (at % 97) as u64;
            edits += 1;
            at += 503 + (seed as usize % 11);
        }
        let ops = craft_diff::diff(&a, &b, &Budget::APPS);
        applies(&a, &b, &ops)?;
        let changed: usize = ops.iter().map(|op| match op { Op::Equal { .. } => 0, _ => ranges(op).0.len() + ranges(op).1.len() }).sum();
        prop_assert!(changed <= 2 * edits, "{edits} edits inflated to {changed} changed tokens");
    }
}
