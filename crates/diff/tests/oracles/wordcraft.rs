//! Verbatim copy of wordcraft's diff algorithm: cloudbring/wordcraft 4912b24b,
//! `crates/engine/src/cmd/tools/compare.rs` lines 200-529 — the `Op` script enum, the
//! `DiffKey` trait with its `String` and `char` impls, `MAX_LCS`, `MAX_COMPARE_CELLS`,
//! `DIRECT`, `MAX_DEPTH`, the test `PEAK_TABLE_CELLS` counter, `lcs_ops`, `diff_seq`,
//! `myers`, `patience_anchors`, `histogram_anchors` and `longest_common_run`, plus the
//! character-level `NOISE_MATCH_LEN`/`collapse_noise_matches`/`flatten_region` block
//! (lines 529-593), to port its char-level fixtures. Only the
//! visibility changed (`pub(super)` and the private `MAX_COMPARE_CELLS`/`MAX_DEPTH` made
//! `pub`); not a line of the algorithm was touched. The differential tests compare
//! `craft_diff::diff` against `lcs_ops`, token for token, on the fixtures of wordcraft's
//! own compare tests. The character-level `collapse_noise_matches` (lines 535+) is
//! wordcraft's rendering layer and stays there.

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Op {
    Same,
    Del,
    Ins,
}

/// What the diff can hash an element to: a cheap fingerprint for finding anchors. A hash
/// coincidence can only lose an anchor (every anchor is verified by equality), never fake one.
pub trait DiffKey: PartialEq {
    fn diff_hash(&self) -> u64;
}

fn text_hash(s: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    s.hash(&mut h);
    h.finish()
}

impl DiffKey for String {
    fn diff_hash(&self) -> u64 {
        text_hash(self)
    }
}

impl DiffKey for char {
    fn diff_hash(&self) -> u64 {
        *self as u64
    }
}

/// Longest sequence per side one comparison is budgeted for: the side of the quadratic table
/// whose cells [`MAX_COMPARE_CELLS`] counts. The anchored diff charges a middle this size
/// before splitting it, so the budget means what it always meant.
pub const MAX_LCS: usize = 4000;

/// Comparison cells one compare may spend over all its tables (blocks and every changed
/// paragraph pair); once spent, the remaining middles are compared as deleted and inserted.
pub const MAX_COMPARE_CELLS: u64 = 50_000_000;

/// Segments up to this size (old + new elements) are diffed directly with Myers' algorithm;
/// larger ones are split at anchors first. That keeps Myers' saved frontiers — O(D²) memory —
/// bounded by DIRECT² cells, under the MAX_LCS × MAX_LCS table the quadratic LCS filled.
pub const DIRECT: usize = 3000;

/// How many times a segment may be split at anchors before what is left is declared replaced:
/// the deterministic fallback that bounds the work on adversarial inputs.
pub const MAX_DEPTH: usize = 8;

/// The largest table-shaped allocation any diff has built, in cells — Myers' saved frontiers.
/// The quadratic LCS table is gone (#53); the tests observe this to prove it stayed gone.
#[cfg(test)]
pub static PEAK_TABLE_CELLS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Edit script between two sequences (#53): the common start and end are matched first, then
/// the middle is diffed by the anchored recursion [`diff_seq`] — bounded Myers between anchors,
/// so long matching middles keep their matching islands instead of being replaced wholesale.
/// The middle is charged to `budget` as the table the quadratic LCS would have filled for it;
/// when it doesn't fit, the middle is deleted and inserted whole, as before.
pub fn lcs_ops<T: DiffKey>(a: &[T], b: &[T], budget: &mut u64) -> Vec<Op> {
    let pre = a.iter().zip(b).take_while(|(x, y)| x == y).count();
    let (ra, rb) = (a.get(pre..).unwrap_or_default(), b.get(pre..).unwrap_or_default());
    let suf = ra.iter().rev().zip(rb.iter().rev()).take_while(|(x, y)| x == y).count();
    let (a, b) = (ra.get(..ra.len() - suf).unwrap_or_default(), rb.get(..rb.len() - suf).unwrap_or_default());
    let mut ops = vec![Op::Same; pre];
    let cells = (a.len().min(MAX_LCS) as u64).saturating_mul(b.len().min(MAX_LCS) as u64);
    if cells > *budget {
        ops.extend(std::iter::repeat_n(Op::Del, a.len()));
        ops.extend(std::iter::repeat_n(Op::Ins, b.len()));
    } else {
        *budget -= cells;
        diff_seq(a, b, &mut ops, 0);
    }
    ops.extend(std::iter::repeat_n(Op::Same, suf));
    ops
}

/// The anchored diff of a middle: a segment within [`DIRECT`] is diffed directly with
/// [`myers`]; a larger one is split at patience anchors (elements unique on both sides, in an
/// order both sides agree on) and, where nothing is unique because the vocabulary repeats, at
/// the longest common run of repeated elements; each gap recurses at the next depth. A segment
/// with no anchor, or past [`MAX_DEPTH`], is declared replaced — the deterministic fallback.
fn diff_seq<T: DiffKey>(a: &[T], b: &[T], out: &mut Vec<Op>, depth: usize) {
    let pre = a.iter().zip(b).take_while(|(x, y)| x == y).count();
    let (ra, rb) = (a.get(pre..).unwrap_or_default(), b.get(pre..).unwrap_or_default());
    let suf = ra.iter().rev().zip(rb.iter().rev()).take_while(|(x, y)| x == y).count();
    let (a, b) = (ra.get(..ra.len() - suf).unwrap_or_default(), rb.get(..rb.len() - suf).unwrap_or_default());
    out.extend(std::iter::repeat_n(Op::Same, pre));
    let replace = |out: &mut Vec<Op>| {
        out.extend(std::iter::repeat_n(Op::Del, a.len()));
        out.extend(std::iter::repeat_n(Op::Ins, b.len()));
    };
    if a.len() + b.len() <= DIRECT {
        out.extend(myers(a, b));
    } else if depth <= MAX_DEPTH {
        let mut split = patience_anchors(a, b);
        if split.is_empty() {
            split = histogram_anchors(a, b);
        }
        if split.is_empty() {
            replace(out);
        } else {
            let (mut i, mut j) = (0usize, 0usize);
            for (x, y) in split {
                diff_seq(a.get(i..x).unwrap_or_default(), b.get(j..y).unwrap_or_default(), out, depth + 1);
                out.push(Op::Same);
                i = x + 1;
                j = y + 1;
            }
            diff_seq(a.get(i..).unwrap_or_default(), b.get(j..).unwrap_or_default(), out, depth + 1);
        }
    } else {
        replace(out);
    }
    out.extend(std::iter::repeat_n(Op::Same, suf));
}

/// Myers' O((n+m)·D) shortest-edit algorithm (Myers 1986): the furthest-reaching path per
/// diagonal, one step at a time, saving each step's frontier slice so the script can be walked
/// back. Frontiers are saved as `u32` (a segment spans at most `DIRECT` elements, so positions
/// fit), and only the diagonals a step spans: at most DIRECT² cells in total.
fn myers<T: PartialEq>(a: &[T], b: &[T]) -> Vec<Op> {
    let (n, m) = (a.len(), b.len());
    if n == 0 || m == 0 {
        return std::iter::repeat_n(Op::Del, n).chain(std::iter::repeat_n(Op::Ins, m)).collect();
    }
    let max = n + m;
    let off = max as isize + 1;
    let mut v = vec![0usize; 2 * max + 3];
    let mut trace: Vec<Vec<u32>> = Vec::new();
    let mut found = false;
    'outer: for d in 0..=max as isize {
        let (lo, hi) = ((off - d - 1).max(0) as usize, (off + d + 1) as usize);
        trace.push(v.get(lo..=hi).unwrap_or_default().iter().map(|&x| x as u32).collect());
        let mut k = -d;
        while k <= d {
            let i = (k + off) as usize;
            let mut x = if k == -d || (k != d && v.get(i - 1).copied().unwrap_or(0) < v.get(i + 1).copied().unwrap_or(0)) {
                v.get(i + 1).copied().unwrap_or(0)
            } else {
                v.get(i - 1).copied().unwrap_or(0) + 1
            };
            let mut y = (x as isize - k) as usize;
            while x < n && y < m && a.get(x) == b.get(y) {
                x += 1;
                y += 1;
            }
            if let Some(slot) = v.get_mut(i) {
                *slot = x;
            }
            if x >= n && y >= m {
                found = true;
                break 'outer;
            }
            k += 2;
        }
    }
    #[cfg(test)]
    PEAK_TABLE_CELLS.fetch_max(trace.iter().map(|s| s.len() as u64).sum::<u64>(), std::sync::atomic::Ordering::Relaxed);
    if !found {
        // Unreachable for bounded segments (the frontier reaches the end by d = n+m); never
        // panic over it: declare the segment replaced.
        return std::iter::repeat_n(Op::Del, n).chain(std::iter::repeat_n(Op::Ins, m)).collect();
    }
    // Walk back through the saved frontiers.
    let mut ops = Vec::new();
    let (mut x, mut y) = (n as isize, m as isize);
    for d in (0..trace.len() as isize).rev() {
        let v = &trace[d as usize];
        let k = x - y;
        let base = (off - d - 1).max(0);
        let at = |k: isize| (k + off - base) as usize;
        let left = v.get(at(k - 1)).copied().unwrap_or(0);
        let right = v.get(at(k + 1)).copied().unwrap_or(0);
        let prev_k = if k == -d || (k != d && left < right) { k + 1 } else { k - 1 };
        let prev_x = v.get(at(prev_k)).copied().unwrap_or(0) as isize;
        let prev_y = prev_x - prev_k;
        while x > prev_x && y > prev_y {
            ops.push(Op::Same);
            x -= 1;
            y -= 1;
        }
        if d > 0 {
            ops.push(if x == prev_x { Op::Ins } else { Op::Del });
        }
        x = prev_x;
        y = prev_y;
    }
    ops.reverse();
    ops
}

/// Patience anchors: elements that occur once on each side, in an order both sides agree on —
/// the longest increasing subsequence of their positions. Occurrences are counted by hash (a
/// coincidence only inflates a count, dropping the element as an anchor); the surviving pairs
/// are verified by equality before they anchor anything.
fn patience_anchors<T: DiffKey>(a: &[T], b: &[T]) -> Vec<(usize, usize)> {
    use std::collections::HashMap;
    // (occurrences in a, occurrences in b, last position in a, last position in b)
    let mut count: HashMap<u64, (u32, u32, usize, usize)> = HashMap::new();
    for (i, x) in a.iter().enumerate() {
        let e = count.entry(x.diff_hash()).or_insert((0, 0, 0, 0));
        e.0 += 1;
        e.2 = i;
    }
    for (j, y) in b.iter().enumerate() {
        if let Some(e) = count.get_mut(&y.diff_hash()) {
            e.1 += 1;
            e.3 = j;
        }
    }
    let mut pairs: Vec<(usize, usize)> =
        count.values().filter(|e| e.0 == 1 && e.1 == 1).map(|e| (e.2, e.3)).filter(|&(i, j)| a.get(i) == b.get(j)).collect();
    pairs.sort();
    // Longest increasing subsequence of the b positions (patience sorting with back links).
    let mut tails: Vec<usize> = Vec::new();
    let mut prev = vec![usize::MAX; pairs.len()];
    for (k, &(_, y)) in pairs.iter().enumerate() {
        let pos = tails.partition_point(|&t| pairs.get(t).is_some_and(|p| p.1 < y));
        if pos > 0 {
            prev[k] = tails.get(pos - 1).copied().unwrap_or(usize::MAX);
        }
        if pos == tails.len() {
            tails.push(k);
        } else if let Some(slot) = tails.get_mut(pos) {
            *slot = k;
        }
    }
    let mut out = Vec::new();
    let mut k = tails.last().copied().unwrap_or(usize::MAX);
    while k != usize::MAX {
        if let Some(&(i, j)) = pairs.get(k) {
            out.push((i, j));
        }
        k = prev.get(k).copied().unwrap_or(usize::MAX);
    }
    out.reverse();
    out
}

/// Histogram anchors for a segment patience cannot split because no element is unique: the
/// longest common run of repeated elements becomes the anchor — repetition makes such runs
/// long, and their length pins them in place where single occurrences could be displaced — so
/// stretches of boilerplate, tables and legal text still match instead of collapsing into one
/// replacement. One pair per element of the run; empty when the sides share no element.
fn histogram_anchors<T: DiffKey>(a: &[T], b: &[T]) -> Vec<(usize, usize)> {
    let Some((i, j, len)) = longest_common_run(a, b) else { return Vec::new() };
    (0..len).map(|k| (i + k, j + k)).collect()
}

/// The longest run of elements common to `a` and `b`: `(start in a, start in b, length)`, or
/// `None` when they share no element. The length is binary-searched with a rolling hash of the
/// element hashes, and every candidate is verified by comparing the elements, so a hash
/// coincidence cannot fake a run. Candidates scan from the middle outwards, and of the
/// occurrences that share a candidate's hash the one closest to it is taken, so a run stays
/// in place instead of landing on a displaced repetition; both keep the two sides of a split
/// balanced and the recursion depth near logarithmic.
fn longest_common_run<T: DiffKey>(a: &[T], b: &[T]) -> Option<(usize, usize, usize)> {
    use std::collections::HashMap;
    const B: u64 = 0x9E37_79B9_7F4A_7C15;
    let (ha, hb): (Vec<u64>, Vec<u64>) = (a.iter().map(DiffKey::diff_hash).collect(), b.iter().map(DiffKey::diff_hash).collect());
    let check = |len: usize| -> Option<(usize, usize, usize)> {
        if len == 0 || len > a.len() || len > b.len() {
            return None;
        }
        let top = B.wrapping_pow(len as u32 - 1);
        let roll = |h: u64, out: u64, inn: u64| h.wrapping_sub(out.wrapping_mul(top)).wrapping_mul(B).wrapping_add(inn);
        let mut windows: HashMap<u64, Vec<usize>> = HashMap::with_capacity(a.len() - len + 1);
        let mut hash = 0u64;
        for &w in &ha[..len] {
            hash = hash.wrapping_mul(B).wrapping_add(w);
        }
        for i in 0..a.len() - len + 1 {
            windows.entry(hash).or_default().push(i);
            if i < a.len() - len {
                hash = roll(hash, ha[i], ha[i + len]);
            }
        }
        let last = b.len() - len;
        let mut hs: Vec<u64> = Vec::with_capacity(last + 1);
        let mut hash = 0u64;
        for &w in &hb[..len] {
            hash = hash.wrapping_mul(B).wrapping_add(w);
        }
        for j in 0..last {
            hs.push(hash);
            hash = roll(hash, hb[j], hb[j + len]);
        }
        hs.push(hash);
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
            // The closest occurrence first: a displaced repetition of the run must not steal
            // the anchor, and the first match ends the verification early.
            let mut at = at.clone();
            at.sort_unstable_by_key(|&i| (i.abs_diff(j), i));
            if let Some(i) = at.into_iter().find(|&i| a.get(i..i + len) == b.get(j..j + len)) {
                return Some((i, j, len));
            }
        }
        None
    };
    let mut best = None;
    let (mut lo, mut hi) = (1usize, a.len().min(b.len()));
    while lo <= hi {
        let len = lo + (hi - lo) / 2;
        match check(len) {
            Some(found) => {
                best = Some(found);
                lo = len + 1;
            }
            None => hi = len - 1,
        }
    }
    best
}

/// How short an interior match can be, inside a run of edits, before it's folded into the
/// surrounding delete/insert instead of staying its own little island of "same" text. Two short
/// strings that happen to share a character in a different order (common with digits in dates,
/// IDs, phone numbers) otherwise come out of `lcs_ops` as a true shortest edit script, but render
/// as a "ransom note" of alternating single-character deletions and insertions where a reader
/// (and Word) would expect one clean replacement.
const NOISE_MATCH_LEN: usize = 1;

/// Folds single-token matches that sit strictly inside a run of edits into the surrounding
/// delete/insert, so a character-level diff doesn't fragment a replaced run. A matched run that
/// opens or closes the whole sequence is the genuine common prefix/suffix `lcs_ops` already
/// stripped before running its table, so it's left untouched regardless of length; only a short
/// match produced by the table's own backtracking, which can only ever be interior, is a
/// candidate.
#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Keep,
    Fold,
}

pub fn collapse_noise_matches(ops: &[Op]) -> Vec<Op> {
    let mut runs: Vec<(Op, usize, Kind)> = Vec::new();
    let mut k = 0;
    while k < ops.len() {
        let op = ops[k];
        let len = ops[k..].iter().take_while(|o| **o == op).count();
        let kind = if op == Op::Same && k != 0 && k + len != ops.len() && len <= NOISE_MATCH_LEN { Kind::Fold } else { Kind::Keep };
        runs.push((op, len, kind));
        k += len;
    }
    if !runs.iter().any(|(_, _, kind)| *kind == Kind::Fold) {
        return ops.to_vec();
    }
    let mut out = Vec::with_capacity(ops.len());
    let mut i = 0;
    while i < runs.len() {
        if runs[i].0 == Op::Same && runs[i].2 == Kind::Keep {
            out.extend(std::iter::repeat_n(Op::Same, runs[i].1));
            i += 1;
            continue;
        }
        let start = i;
        while i < runs.len() && !(runs[i].0 == Op::Same && runs[i].2 == Kind::Keep) {
            i += 1;
        }
        let region = &runs[start..i];
        if region.iter().any(|(_, _, kind)| *kind == Kind::Fold) {
            flatten_region(&mut out, region);
        } else {
            for (op, len, _) in region {
                out.extend(std::iter::repeat_n(*op, *len));
            }
        }
    }
    out
}

/// Flattens a region with a folded run: every original character first (in its own order), then
/// every revised character (in its own order) — one clean delete, one clean insert.
fn flatten_region(out: &mut Vec<Op>, region: &[(Op, usize, Kind)]) {
    let dels: usize = region.iter().map(|(op, len, kind)| if *kind == Kind::Fold || *op == Op::Del { *len } else { 0 }).sum();
    let inss: usize = region.iter().map(|(op, len, kind)| if *kind == Kind::Fold || *op == Op::Ins { *len } else { 0 }).sum();
    out.extend(std::iter::repeat_n(Op::Del, dels));
    out.extend(std::iter::repeat_n(Op::Ins, inss));
}
