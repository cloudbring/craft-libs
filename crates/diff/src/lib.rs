#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
//! craft-diff — the anchored, budgeted sequence diff shared by the Craft Apps' compare
//! (pdfcraft #54, wordcraft #53).
//!
//! One call, [`diff`], turns two token sequences into a list of [`Op`]s — ranges that are
//! equal, inserted, deleted or replaced — under a [`Budget`]. A token is anything hashable
//! and comparable (`T: Hash + Eq`); how a document becomes tokens (words, characters,
//! paragraphs) stays in each app.
//!
//! # The pipeline
//!
//! 1. **Common edge trim** — the shared prefix and suffix come off, O(n).
//! 2. **Bounded Myers** — a segment of at most [`Budget::max_direct`] tokens (old + new) is
//!    diffed directly with Myers' O((n+m)·D) algorithm (Myers 1986), saving each step's
//!    frontier slice so the script can be walked back: O(D²) cells per segment.
//! 3. **Patience anchors** — a larger segment splits at tokens that occur once on each
//!    side, in an order both sides agree on (the longest increasing subsequence of their
//!    positions): O(n log n).
//! 4. **Longest-common-run anchors** — where repeated vocabulary leaves nothing unique, the
//!    longest common run of repeated tokens becomes the anchor (rolling hash, binary-searched
//!    length, middle-out, verified by comparison): O((n+m)·log min(n,m)) expected hashing.
//! 5. **Recursion** — each gap between anchors recurses at the next depth, to
//!    [`Budget::max_depth`].
//! 6. **Deterministic replace fallback** — a segment with no anchor, past the depth budget,
//!    or once the work budget is spent, is declared replaced.
//!
//! # Complexity
//!
//! The edge trim, the token fingerprints and the grouping are O(n + m). Every stage that
//! can repeat superlinearly — the frontier cells Myers saves, the tokens the anchor search
//! hashes and verifies — is charged to [`Budget::work_units`] and stops at the budget with
//! the replace fallback, so a whole call is O(n + m + work_units) time (plus at most one
//! in-flight stage's own pass when the budget is crossed) and O(n + m + D²) memory, where
//! D ≤ max_direct: the largest saved-frontier trace spans at most
//! `max_direct² + 3·max_direct + 3` cells. There is no quadratic table: the O(n·m) LCS
//! table both apps once filled is gone, and the crate's tests observe the peak allocation
//! to prove it stays gone.
//!
//! # Determinism
//!
//! The same inputs on the same build always produce the same ops. Token fingerprints come
//! from `DefaultHasher::new()` — fixed keys, no per-run randomness — and no decision depends
//! on hash-table iteration order: anchor pairs are sorted by position, and candidate run
//! occurrences by distance then index. A hash coincidence can only lose an anchor, never
//! fake one: every anchor and every common run is verified by token equality before it is
//! used, and Myers compares tokens directly.
//!
//! Neither app detects moved blocks (wordcraft's `moves` compare option is accepted but not
//! honoured), so craft-diff has no `moved_blocks` either.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::ops::Range;

/// One difference between `a` and `b`, as ranges into each: `a[old]` and `b[new]`.
///
/// The ops cover both sides exactly and in order: an `Equal` range has the same length and
/// the same tokens on both sides, `Insert`/`Delete` have an empty range on the side they do
/// not touch, and a maximal run of deleted and inserted tokens between `Equal`s is one
/// `Replace` — the one change both apps render. Applying the ops to `a` (keeping `Equal`
/// tokens, taking `Insert`/`Replace` tokens from `b`) yields `b` exactly.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Op {
    /// `a[old]` equals `b[new]`.
    Equal { old: Range<usize>, new: Range<usize> },
    /// `b[new]` was inserted where `a` had nothing (`old` is empty, at the position).
    Insert { old: Range<usize>, new: Range<usize> },
    /// `a[old]` was deleted (`new` is empty, at the position).
    Delete { old: Range<usize>, new: Range<usize> },
    /// `a[old]` was replaced by `b[new]`.
    Replace { old: Range<usize>, new: Range<usize> },
}

/// What one [`diff`] call may spend. The defaults are the constants both apps shipped:
/// pdfcraft `crates/compare/src/lib.rs` and wordcraft
/// `crates/engine/src/cmd/tools/compare.rs` (`DIRECT`, `MAX_DEPTH`, `MAX_COMPARE_CELLS`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Budget {
    /// Segments up to this total length (old + new tokens) are diffed directly with Myers'
    /// saved-frontier algorithm; larger ones are split at anchors first, which keeps the
    /// frontiers — O(D²) cells — small. Both apps: 3,000.
    pub max_direct: usize,
    /// How many times a segment may be split at anchors before what is left is declared
    /// replaced: the deterministic fallback that bounds the work on adversarial inputs.
    /// Both apps: 8.
    pub max_depth: usize,
    /// Work units one diff may spend: every frontier cell Myers saves and every token the
    /// anchor search hashes or verifies is one unit; past the budget the remaining segments
    /// are declared replaced. Wordcraft's `MAX_COMPARE_CELLS`: 50,000,000 (pdfcraft had no
    /// work budget; wordcraft shared one across a document's pairs — an app wanting that
    /// passes smaller per-call budgets).
    pub work_units: u64,
}

impl Budget {
    /// A budget from its three limits.
    pub const fn new(max_direct: usize, max_depth: usize, work_units: u64) -> Self {
        Self { max_direct, max_depth, work_units }
    }

    /// The apps' constants: direct segments of 3,000 tokens, depth 8, 50 million work units.
    pub const APPS: Self = Self { max_direct: 3000, max_depth: 8, work_units: 50_000_000 };
}

impl Default for Budget {
    fn default() -> Self {
        Self::APPS
    }
}

/// The per-token script the pipeline builds, before it is grouped into range [`Op`]s.
#[derive(Clone, Copy, PartialEq)]
enum Step {
    Equal,
    Delete,
    Insert,
}

/// The largest table-shaped allocation any diff has built, in cells — Myers' saved
/// frontiers. The quadratic LCS table is gone (#53); the tests observe this to prove it
/// stayed gone (wordcraft shipped the same counter).
#[cfg(test)]
static PEAK_TABLE_CELLS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// The edit script for `a` → `b`, grouped into range [`Op`]s, under `budget`.
pub fn diff<T: Hash + Eq>(a: &[T], b: &[T], budget: &Budget) -> Vec<Op> {
    let hashes = |tokens: &[T]| -> Vec<u64> { tokens.iter().map(token_hash).collect() };
    let (ha, hb) = (hashes(a), hashes(b));
    let mut steps = Vec::with_capacity(a.len().max(b.len()));
    let mut meter = Meter { budget, work: 0 };
    diff_seq(&Tokens { elems: a, hashes: &ha }, &Tokens { elems: b, hashes: &hb }, &mut steps, 0, &mut meter);
    group(&steps)
}

/// A token's fingerprint for the anchor search: `DefaultHasher` with its fixed keys — the
/// same hasher both apps use — so the search is deterministic within and across runs.
fn token_hash<T: Hash>(token: &T) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    token.hash(&mut hasher);
    hasher.finish()
}

/// One side of a segment being diffed: the tokens and their fingerprints, sliced in
/// lockstep so every trim of the tokens trims the fingerprints too.
struct Tokens<'a, T> {
    elems: &'a [T],
    hashes: &'a [u64],
}

impl<T> Tokens<'_, T> {
    fn len(&self) -> usize {
        self.elems.len()
    }

    fn slice(&self, from: usize, to: usize) -> Tokens<'_, T> {
        Tokens { elems: self.elems.get(from..to).unwrap_or(&[]), hashes: self.hashes.get(from..to).unwrap_or(&[]) }
    }
}

/// The accounting one diff call keeps: the work spent against [`Budget::work_units`]. Once
/// spent, every remaining segment takes the deterministic replace fallback — which is what
/// bounds the total work.
struct Meter<'a> {
    budget: &'a Budget,
    work: u64,
}

impl Meter<'_> {
    /// Charges `units` and reports whether the budget still holds.
    fn spend(&mut self, units: u64) -> bool {
        self.work = self.work.saturating_add(units);
        self.work <= self.budget.work_units
    }

    /// Whether the budget is already spent.
    fn spent(&self) -> bool {
        self.work > self.budget.work_units
    }
}

/// The anchored diff of a segment (both apps' `diff`/`diff_seq`): the common start and end
/// are matched first, then a segment within [`Budget::max_direct`] is diffed directly with
/// [`myers`]; a larger one is split at patience anchors (tokens unique on both sides, in an
/// order both sides agree on) and, where nothing is unique because the vocabulary repeats,
/// at the longest common run of repeated tokens; each gap recurses at the next depth. A
/// segment with no anchor, past [`Budget::max_depth`], or once the work budget is spent, is
/// declared replaced — the deterministic fallback.
fn diff_seq<T: Eq>(a: &Tokens<'_, T>, b: &Tokens<'_, T>, out: &mut Vec<Step>, depth: usize, meter: &mut Meter<'_>) {
    let pre = a.elems.iter().zip(b.elems).take_while(|(x, y)| x == y).count();
    let suf = a.elems[pre..].iter().rev().zip(b.elems[pre..].iter().rev()).take_while(|(x, y)| x == y).count();
    let (a, b) = (a.slice(pre, a.len() - suf), b.slice(pre, b.len() - suf));
    out.extend(std::iter::repeat_n(Step::Equal, pre));
    let replace = |out: &mut Vec<Step>| {
        out.extend(std::iter::repeat_n(Step::Delete, a.len()));
        out.extend(std::iter::repeat_n(Step::Insert, b.len()));
    };
    if meter.spent() {
        replace(out);
    } else if a.len() + b.len() <= meter.budget.max_direct {
        myers(&a, &b, out, meter);
    } else if depth <= meter.budget.max_depth {
        let mut split = patience_anchors(&a, &b, meter);
        if split.is_empty() {
            split = histogram_anchors(&a, &b, meter);
        }
        if split.is_empty() {
            // A long stretch with nothing in common, or beyond the depth budget: one replacement.
            replace(out);
        } else {
            let (mut i, mut j) = (0, 0);
            for (x, y) in split {
                diff_seq(&a.slice(i, x), &b.slice(j, y), out, depth + 1, meter);
                out.push(Step::Equal);
                i = x + 1;
                j = y + 1;
            }
            diff_seq(&a.slice(i, a.len()), &b.slice(j, b.len()), out, depth + 1, meter);
        }
    } else {
        replace(out);
    }
    out.extend(std::iter::repeat_n(Step::Equal, suf));
}

/// Patience anchors: tokens that occur once on each side, in an order both sides agree on —
/// the longest increasing subsequence of their positions. Occurrences are counted by
/// fingerprint (a coincidence only inflates a count, dropping the candidate); the surviving
/// pairs are verified by equality before they anchor anything.
fn patience_anchors<T: Eq>(a: &Tokens<'_, T>, b: &Tokens<'_, T>, meter: &mut Meter<'_>) -> Vec<(usize, usize)> {
    let _ = meter.spend((a.len() + b.len()) as u64);
    if meter.spent() {
        return Vec::new();
    }
    // (occurrences in a, occurrences in b, last position in a, last position in b)
    let mut count: HashMap<u64, (u32, u32, usize, usize)> = HashMap::new();
    for (i, h) in a.hashes.iter().enumerate() {
        let e = count.entry(*h).or_insert((0, 0, 0, 0));
        e.0 += 1;
        e.2 = i;
    }
    for (j, h) in b.hashes.iter().enumerate() {
        if let Some(e) = count.get_mut(h) {
            e.1 += 1;
            e.3 = j;
        }
    }
    let mut pairs: Vec<(usize, usize)> =
        count.values().filter(|e| e.0 == 1 && e.1 == 1).map(|e| (e.2, e.3)).filter(|&(i, j)| a.elems.get(i) == b.elems.get(j)).collect();
    pairs.sort();
    // Longest increasing subsequence of the b positions (patience sorting with back links).
    let mut tails: Vec<usize> = Vec::new();
    let mut prev = vec![usize::MAX; pairs.len()];
    for (k, &(_, y)) in pairs.iter().enumerate() {
        let pos = tails.partition_point(|&t| pairs[t].1 < y);
        if pos > 0 {
            prev[k] = tails[pos - 1];
        }
        if pos == tails.len() {
            tails.push(k);
        } else {
            tails[pos] = k;
        }
    }
    let mut out = Vec::new();
    let mut k = tails.last().copied().unwrap_or(usize::MAX);
    while k != usize::MAX {
        if let Some(&(i, j)) = pairs.get(k) {
            out.push((i, j));
        }
        k = prev[k];
    }
    out.reverse();
    out
}

/// Histogram anchors for a segment patience cannot split because no token is unique: the
/// longest common run of repeated tokens becomes the anchor — repetition makes such runs
/// long, and their length pins them in place where single occurrences could be displaced —
/// so stretches of boilerplate, tables and legal text still match instead of collapsing into
/// one replacement. One pair per token of the run; empty when the sides share no token.
fn histogram_anchors<T: Eq>(a: &Tokens<'_, T>, b: &Tokens<'_, T>, meter: &mut Meter<'_>) -> Vec<(usize, usize)> {
    let Some((i, j, len)) = longest_common_run(a, b, meter) else { return Vec::new() };
    (0..len).map(|k| (i + k, j + k)).collect()
}

/// The longest run of tokens common to `a` and `b`: `(start in a, start in b, length)`, or
/// `None` when they share no token. The length is binary-searched with a rolling hash of the
/// token fingerprints, and every candidate is verified by comparing the tokens, so a hash
/// coincidence cannot fake a run. Candidates scan from the middle outwards, and of the
/// occurrences that share a candidate's hash the one closest to it is taken, so a run stays
/// in place instead of landing on a displaced repetition; both keep the two sides of a
/// split balanced and the recursion depth near logarithmic.
fn longest_common_run<T: Eq>(a: &Tokens<'_, T>, b: &Tokens<'_, T>, meter: &mut Meter<'_>) -> Option<(usize, usize, usize)> {
    const B: u64 = 0x9E37_79B9_7F4A_7C15;
    let check = |meter: &mut Meter<'_>, len: usize| -> Option<(usize, usize, usize)> {
        if len == 0 || len > a.len() || len > b.len() || meter.spent() {
            return None;
        }
        let top = B.wrapping_pow(len as u32 - 1);
        let roll = |h: u64, out: u64, inn: u64| h.wrapping_sub(out.wrapping_mul(top)).wrapping_mul(B).wrapping_add(inn);
        let mut windows: HashMap<u64, Vec<usize>> = HashMap::with_capacity(a.len() - len + 1);
        let mut hash = 0u64;
        for &w in &a.hashes[..len] {
            hash = hash.wrapping_mul(B).wrapping_add(w);
        }
        for i in 0..a.len() - len + 1 {
            windows.entry(hash).or_default().push(i);
            if i < a.len() - len {
                hash = roll(hash, a.hashes[i], a.hashes[i + len]);
            }
        }
        let last = b.len() - len;
        let mut hs: Vec<u64> = Vec::with_capacity(last + 1);
        let mut hash = 0u64;
        for &w in &b.hashes[..len] {
            hash = hash.wrapping_mul(B).wrapping_add(w);
        }
        for j in 0..last {
            hs.push(hash);
            hash = roll(hash, b.hashes[j], b.hashes[j + len]);
        }
        hs.push(hash);
        let _ = meter.spend((a.len() + b.len() - 2 * len + 2) as u64);
        if meter.spent() {
            return None;
        }
        let mid = last / 2;
        let mut order: Vec<usize> = Vec::with_capacity(last + 1);
        order.push(mid);
        for m in 1..=mid.max(last - mid) {
            if mid >= m {
                order.push(mid - m);
            }
            if mid + m <= last {
                order.push(mid + m);
            }
        }
        for j in order {
            let Some(at) = windows.get(&hs[j]) else { continue };
            // The closest occurrence first: a displaced repetition of the run must not
            // steal the anchor, and the first match ends the verification early.
            let _ = meter.spend(at.len() as u64 + len as u64);
            if meter.spent() {
                return None;
            }
            let mut at = at.clone();
            at.sort_unstable_by_key(|&i| (i.abs_diff(j), i));
            if let Some(i) = at.into_iter().find(|&i| a.elems.get(i..i + len) == b.elems.get(j..j + len)) {
                return Some((i, j, len));
            }
        }
        None
    };
    let mut best = None;
    let (mut lo, mut hi) = (1usize, a.len().min(b.len()));
    while lo <= hi {
        let len = lo + (hi - lo) / 2;
        match check(meter, len) {
            Some(found) => {
                best = Some(found);
                lo = len + 1;
            }
            None => hi = len - 1,
        }
    }
    best
}

/// Myers' O((n+m)·D) shortest-edit algorithm (Myers 1986): the furthest-reaching path per
/// diagonal, one step at a time, saving each step's frontier slice so the script can be
/// walked back. Only the diagonals a step spans are saved — at most
/// `max_direct² + 3·max_direct + 3` cells per segment — and every cell is charged to the
/// work budget as it is saved.
fn myers<T: PartialEq>(a: &Tokens<'_, T>, b: &Tokens<'_, T>, out: &mut Vec<Step>, meter: &mut Meter<'_>) {
    let (a, b) = (a.elems, b.elems);
    let (n, m) = (a.len(), b.len());
    if n == 0 || m == 0 {
        out.extend(std::iter::repeat_n(Step::Delete, n).chain(std::iter::repeat_n(Step::Insert, m)));
        return;
    }
    let max = n + m;
    let off = max as isize + 1;
    let mut v = vec![0usize; 2 * max + 3];
    let mut trace: Vec<Vec<usize>> = Vec::new();
    #[cfg(test)]
    let mut cells = 0u64;
    let mut found = false;
    'outer: for d in 0..=max as isize {
        let lo = (off - d - 1).max(0) as usize;
        let hi = (off + d + 1) as usize;
        let width = (hi - lo + 1) as u64;
        #[cfg(test)]
        {
            cells += width;
        }
        trace.push(v.get(lo..=hi).unwrap_or_default().to_vec());
        if !meter.spend(width) {
            // Past the work budget: the segment is declared replaced.
            break;
        }
        let mut k = -d;
        while k <= d {
            let i = (k + off) as usize;
            let mut x = if k == -d || (k != d && v[i - 1] < v[i + 1]) { v[i + 1] } else { v[i - 1] + 1 };
            let mut y = (x as isize - k) as usize;
            while x < n && y < m && a[x] == b[y] {
                x += 1;
                y += 1;
            }
            v[i] = x;
            if x >= n && y >= m {
                found = true;
                break 'outer;
            }
            k += 2;
        }
    }
    #[cfg(test)]
    PEAK_TABLE_CELLS.fetch_max(cells, std::sync::atomic::Ordering::Relaxed);
    if !found {
        // The budget ran out mid-search, or (unreachable for a bounded segment, where the
        // frontier reaches the end by d = n+m) the ends were never met: declare the segment
        // replaced rather than panicking over it.
        out.extend(std::iter::repeat_n(Step::Delete, n).chain(std::iter::repeat_n(Step::Insert, m)));
        return;
    }
    // Walk back through the saved frontiers.
    let mut ops = Vec::new();
    let (mut x, mut y) = (n as isize, m as isize);
    for d in (0..trace.len() as isize).rev() {
        let v = &trace[d as usize];
        let k = x - y;
        let base = (off - d - 1).max(0);
        let at = |k: isize| (k + off - base) as usize;
        let prev_k = if k == -d || (k != d && v[at(k - 1)] < v[at(k + 1)]) { k + 1 } else { k - 1 };
        let prev_x = v[at(prev_k)] as isize;
        let prev_y = prev_x - prev_k;
        while x > prev_x && y > prev_y {
            ops.push(Step::Equal);
            x -= 1;
            y -= 1;
        }
        if d > 0 {
            ops.push(if x == prev_x { Step::Insert } else { Step::Delete });
        }
        x = prev_x;
        y = prev_y;
    }
    ops.reverse();
    out.extend(ops);
}

/// Groups the per-token script into range [`Op`]s: a maximal run of `Equal` steps is one
/// `Equal`, and a maximal run of delete/insert steps between them is one `Replace` — or
/// `Insert`/`Delete` when one side is empty. That is the grouping both apps make when they
/// turn the script into changes.
fn group(steps: &[Step]) -> Vec<Op> {
    let mut out = Vec::new();
    let (mut i, mut j, mut k) = (0usize, 0usize, 0usize);
    while k < steps.len() {
        if steps[k] == Step::Equal {
            let len = steps[k..].iter().take_while(|&&s| s == Step::Equal).count();
            out.push(Op::Equal { old: i..i + len, new: j..j + len });
            i += len;
            j += len;
            k += len;
            continue;
        }
        let (old_start, new_start) = (i, j);
        while k < steps.len() && steps[k] != Step::Equal {
            match steps[k] {
                Step::Delete => i += 1,
                _ => j += 1,
            }
            k += 1;
        }
        let (old, new) = (old_start..i, new_start..j);
        out.push(match (old.is_empty(), new.is_empty()) {
            (false, false) => Op::Replace { old, new },
            (false, true) => Op::Delete { old, new },
            (true, false) => Op::Insert { old, new },
            (true, true) => continue,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::Ordering;

    /// The ops must cover `a` and `b` exactly, in order, with equal tokens on every `Equal`,
    /// and applying them to `a` must rebuild `b`.
    fn applies(a: &[u64], b: &[u64], ops: &[Op]) {
        let (mut i, mut j, mut got) = (0usize, 0usize, Vec::new());
        for op in ops {
            let (old, new) = ranges(op);
            assert_eq!((old.start, new.start), (i, j), "{op:?}: the ops must cover both sides in order");
            match op {
                Op::Equal { .. } => {
                    assert_eq!(a[old.clone()], b[new.clone()], "{op:?}: an Equal range must hold equal tokens");
                    got.extend_from_slice(&b[new.clone()]);
                }
                Op::Delete { .. } => {}
                Op::Insert { .. } | Op::Replace { .. } => got.extend_from_slice(&b[new.clone()]),
            }
            i = old.end;
            j = new.end;
        }
        assert_eq!((i, j), (a.len(), b.len()), "the ops must cover both sides exactly");
        assert_eq!(got, b, "applying the ops to a must rebuild b");
    }

    fn ranges(op: &Op) -> (&Range<usize>, &Range<usize>) {
        match op {
            Op::Equal { old, new } | Op::Insert { old, new } | Op::Delete { old, new } | Op::Replace { old, new } => (old, new),
        }
    }

    #[test]
    fn empty_and_pure_sides() {
        assert_eq!(diff::<u64>(&[], &[], &Budget::APPS), []);
        assert_eq!(diff(&[], &[7, 8], &Budget::APPS), [Op::Insert { old: 0..0, new: 0..2 }]);
        assert_eq!(diff(&[7, 8], &[], &Budget::APPS), [Op::Delete { old: 0..2, new: 0..0 }]);
        assert_eq!(diff(&[7, 8], &[7, 8], &Budget::APPS), [Op::Equal { old: 0..2, new: 0..2 }]);
    }

    #[test]
    fn grouped_ops_match_the_plain_fixtures() {
        // Replacement, insertion and deletion between clean equals, as pdfcraft's fox
        // fixture has them, on tokens small enough to verify by hand.
        let cases: &[(&[u64], &[u64], &[Op])] = &[
            (
                &[5, 1, 2, 6],
                &[5, 30, 40, 6],
                &[Op::Equal { old: 0..1, new: 0..1 }, Op::Replace { old: 1..3, new: 1..3 }, Op::Equal { old: 3..4, new: 3..4 }],
            ),
            (
                &[5, 1, 2, 6],
                &[5, 1, 9, 6],
                &[Op::Equal { old: 0..2, new: 0..2 }, Op::Replace { old: 2..3, new: 2..3 }, Op::Equal { old: 3..4, new: 3..4 }],
            ),
            (
                &[5, 1, 2, 6],
                &[5, 1, 2, 7, 6],
                &[Op::Equal { old: 0..3, new: 0..3 }, Op::Insert { old: 3..3, new: 3..4 }, Op::Equal { old: 3..4, new: 4..5 }],
            ),
            (&[5, 1, 2, 6], &[5, 6], &[Op::Equal { old: 0..1, new: 0..1 }, Op::Delete { old: 1..3, new: 1..1 }, Op::Equal { old: 3..4, new: 1..2 }]),
            (
                &[5, 1, 2, 3, 6],
                &[5, 6],
                &[Op::Equal { old: 0..1, new: 0..1 }, Op::Delete { old: 1..4, new: 1..1 }, Op::Equal { old: 4..5, new: 1..2 }],
            ),
        ];
        for (a, b, want) in cases {
            let ops = diff(a, b, &Budget::APPS);
            assert_eq!(&ops, want);
            applies(a, b, &ops);
        }
    }

    #[test]
    fn interleaved_edits_group_into_one_replace() {
        // Whatever order Myers interleaves deletes and inserts inside a run, the grouping is
        // one clean replacement.
        let ops = diff(&[10, 20], &[30, 40], &Budget::APPS);
        assert_eq!(ops, [Op::Replace { old: 0..2, new: 0..2 }]);
        let ops = diff(&[1, 10, 20, 2], &[1, 30, 40, 2], &Budget::APPS);
        assert_eq!(ops, [Op::Equal { old: 0..1, new: 0..1 }, Op::Replace { old: 1..3, new: 1..3 }, Op::Equal { old: 3..4, new: 3..4 }]);
    }

    #[test]
    fn depth_budget_falls_back_to_replace() {
        // Disjoint vocabularies larger than max_direct have no anchor: one replacement.
        let a: Vec<u64> = (0..100).collect();
        let b: Vec<u64> = (1000..1100).collect();
        let ops = diff(&a, &b, &Budget::new(10, 8, u64::MAX));
        assert_eq!(ops, [Op::Replace { old: 0..100, new: 0..100 }]);
        applies(&a, &b, &ops);
    }

    /// `diff` with the work a call charged, for the budget-bound assertions.
    fn diff_measuring<T: Hash + Eq>(a: &[T], b: &[T], budget: &Budget) -> (Vec<Op>, u64) {
        let hashes = |tokens: &[T]| -> Vec<u64> { tokens.iter().map(token_hash).collect() };
        let (ha, hb) = (hashes(a), hashes(b));
        let mut steps = Vec::with_capacity(a.len().max(b.len()));
        let mut meter = Meter { budget, work: 0 };
        diff_seq(&Tokens { elems: a, hashes: &ha }, &Tokens { elems: b, hashes: &hb }, &mut steps, 0, &mut meter);
        (group(&steps), meter.work)
    }

    #[test]
    fn work_budget_falls_back_without_breaking_the_script() {
        // pdfcraft's 10k/50-vocabulary/20-edit shape, with a budget far too small to finish:
        // the ops still apply (segments past the budget are replaced wholesale), stay
        // deterministic, and the work stays within the budget plus one in-flight stage's own
        // pass (one Myers d-step or one longest-common-run window sweep, O(n + m)).
        let a: Vec<u64> = (0..10_000).map(|i| (i % 50) as u64).collect();
        let mut b = a.clone();
        for k in 0..20 {
            b[199 + 487 * k] = 90 + k as u64;
        }
        let tight = Budget::new(3000, 8, 10_000);
        let (first, work) = diff_measuring(&a, &b, &tight);
        assert_eq!(first, diff(&a, &b, &tight), "a spent budget must still be deterministic");
        applies(&a, &b, &first);
        assert!(work <= tight.work_units + 2 * (a.len() + b.len()) as u64, "{work} work units spent past the budget");
    }

    #[test]
    fn peak_table_cells_stay_bounded() {
        // The 50k worst shapes of #53/#54: repeated vocabulary with scattered edits, a block
        // swap of repeated tokens, and two documents with nothing in common. No quadratic
        // table is built above the frontier bound — where a 50,000 × 50,000 compare would
        // want 2.5 billion cells.
        PEAK_TABLE_CELLS.store(0, Ordering::Relaxed);
        let start = std::time::Instant::now();
        let vocab: Vec<u64> = (0..50).collect();
        let a: Vec<u64> = (0..50_000).map(|i| vocab[i % 50]).collect();
        let mut b = a.clone();
        for k in 0..100 {
            b[101 + 431 * k] = 70;
        }
        let ops = diff(&a, &b, &Budget::APPS);
        applies(&a, &b, &ops);
        assert!(ops.iter().filter(|op| matches!(op, Op::Replace { .. })).count() <= 100);

        let block = |first: u64, second: u64| (0..50_000).map(|i| if i < 25_000 { first } else { second }).collect::<Vec<_>>();
        let swapped = diff(&block(1, 2), &block(2, 1), &Budget::APPS);
        let changed: usize = swapped
            .iter()
            .map(|op| match op {
                Op::Equal { .. } => 0,
                _ => ranges(op).0.len() + ranges(op).1.len(),
            })
            .sum();
        assert_eq!(changed, 50_000, "a block swap of repeated tokens has no monotone match beyond one half");

        let a: Vec<u64> = (0..50_000).map(|i| i as u64).collect();
        let b: Vec<u64> = (0..50_000).map(|i| 1_000_000 + i as u64).collect();
        let ops = diff(&a, &b, &Budget::APPS);
        assert_eq!(ops.len(), 1, "nothing in common: one replacement");
        applies(&a, &b, &ops);

        let peak = PEAK_TABLE_CELLS.load(Ordering::Relaxed);
        let bound = Budget::APPS.max_direct as u64;
        eprintln!("50,000-token worst shapes: {:?}, peak table {peak} cells", start.elapsed());
        // Myers' frontiers for a segment of at most max_direct tokens span at most
        // max_direct² + 3·max_direct + 3 cells.
        assert!(peak <= bound * bound + 3 * bound + 3, "{peak} table cells");
        assert!(start.elapsed() < std::time::Duration::from_secs(30), "{:?}", start.elapsed());
    }
}
