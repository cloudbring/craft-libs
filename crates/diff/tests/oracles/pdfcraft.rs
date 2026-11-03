//! Verbatim copy of pdfcraft's word-diff algorithm: cloudbring/pdfcraft 974f4be4b,
//! `crates/compare/src/lib.rs` lines 78-322 — the `Op` script enum, `DIRECT`, `MAX_DEPTH`,
//! `diff`, `anchors`, `histogram_anchors`, `longest_common_run`, `myers` and the word
//! `hash`. Only the visibility changed (`enum Op`, `fn diff`, `fn hash` made `pub`); not a
//! line of the algorithm was touched. The differential tests compare `craft_diff::diff`
//! against it, token for token, on the fixtures of pdfcraft's own compare tests.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    Equal,
    Delete,
    Insert,
}

/// Segments longer than this (old + new words) are first split at anchors, so Myers' saved
/// frontiers (O(D²) memory) stay small: the cost budget a gap must fit to be diffed directly.
const DIRECT: usize = 3000;

/// How many times a segment may be split at anchors before what is left is declared replaced:
/// the deterministic fallback that bounds the work on adversarial inputs.
const MAX_DEPTH: usize = 8;

/// The edit script for `a` → `b`.
pub fn diff(a: &[u64], b: &[u64], out: &mut Vec<Op>, depth: usize) {
    let prefix = a.iter().zip(b).take_while(|(x, y)| x == y).count();
    let suffix = a[prefix..].iter().rev().zip(b[prefix..].iter().rev()).take_while(|(x, y)| x == y).count();
    out.extend(std::iter::repeat_n(Op::Equal, prefix));
    let (ma, mb) = (&a[prefix..a.len() - suffix], &b[prefix..b.len() - suffix]);
    let replace = |out: &mut Vec<Op>| out.extend(std::iter::repeat_n(Op::Delete, ma.len()).chain(std::iter::repeat_n(Op::Insert, mb.len())));
    if ma.len() + mb.len() <= DIRECT {
        out.extend(myers(ma, mb));
    } else {
        let mut split = Vec::new();
        if depth <= MAX_DEPTH {
            split = anchors(ma, mb);
            if split.is_empty() {
                split = histogram_anchors(ma, mb);
            }
        }
        if split.is_empty() {
            // A long stretch with nothing in common, or beyond the depth budget: one replacement.
            replace(out);
        } else {
            let (mut i, mut j) = (0, 0);
            for (x, y) in split {
                diff(&ma[i..x], &mb[j..y], out, depth + 1);
                out.push(Op::Equal);
                i = x + 1;
                j = y + 1;
            }
            diff(&ma[i..], &mb[j..], out, depth + 1);
        }
    }
    out.extend(std::iter::repeat_n(Op::Equal, suffix));
}

/// Patience anchors: words that occur once in each side, in an order both sides agree on
/// (the longest increasing subsequence of their positions).
fn anchors(a: &[u64], b: &[u64]) -> Vec<(usize, usize)> {
    use std::collections::HashMap;
    let mut count: HashMap<u64, (usize, usize, usize, usize)> = HashMap::new();
    for (i, h) in a.iter().enumerate() {
        let e = count.entry(*h).or_insert((0, 0, 0, 0));
        e.0 += 1;
        e.2 = i;
    }
    for (j, h) in b.iter().enumerate() {
        if let Some(e) = count.get_mut(h) {
            e.1 += 1;
            e.3 = j;
        }
    }
    let mut pairs: Vec<(usize, usize)> = count.values().filter(|e| e.0 == 1 && e.1 == 1).map(|e| (e.2, e.3)).collect();
    pairs.sort();
    // LIS on the b positions (patience sorting with back links).
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
        out.push(pairs[k]);
        k = prev[k];
    }
    out.reverse();
    out
}

/// Histogram anchors (#54) for a segment patience cannot split because no word is unique:
/// the longest common run of repeated words becomes the anchor — repetition makes such runs
/// long, and their length pins them in place where single occurrences could be displaced —
/// so stretches of boilerplate, tables and legal text still match instead of collapsing into
/// one replacement. One pair per word of the run; empty when the sides share no word.
fn histogram_anchors(a: &[u64], b: &[u64]) -> Vec<(usize, usize)> {
    let Some((i, j, len)) = longest_common_run(a, b) else { return Vec::new() };
    (0..len).map(|k| (i + k, j + k)).collect()
}

/// The longest run of words common to `a` and `b`: `(start in a, start in b, length)`, or
/// `None` when they share no word. The length is binary-searched with a rolling hash of the
/// word hashes, and every candidate is verified by comparing the words, so a hash
/// coincidence cannot fake a run. Candidates scan from the middle outwards, and of the
/// occurrences that share a candidate's hash the one closest to it is taken, so a run stays
/// in place instead of landing on a displaced repetition; both keep the two sides of a
/// split balanced and the recursion depth near logarithmic.
fn longest_common_run(a: &[u64], b: &[u64]) -> Option<(usize, usize, usize)> {
    use std::collections::HashMap;
    const B: u64 = 0x9E37_79B9_7F4A_7C15;
    let check = |len: usize| -> Option<(usize, usize, usize)> {
        if len == 0 || len > a.len() || len > b.len() {
            return None;
        }
        let top = B.wrapping_pow(len as u32 - 1);
        let roll = |h: u64, out: u64, inn: u64| h.wrapping_sub(out.wrapping_mul(top)).wrapping_mul(B).wrapping_add(inn);
        let mut windows: HashMap<u64, Vec<usize>> = HashMap::with_capacity(a.len() - len + 1);
        let mut hash = 0u64;
        for &w in &a[..len] {
            hash = hash.wrapping_mul(B).wrapping_add(w);
        }
        for i in 0..a.len() - len + 1 {
            windows.entry(hash).or_default().push(i);
            if i < a.len() - len {
                hash = roll(hash, a[i], a[i + len]);
            }
        }
        let last = b.len() - len;
        let mut hs: Vec<u64> = Vec::with_capacity(last + 1);
        let mut hash = 0u64;
        for &w in &b[..len] {
            hash = hash.wrapping_mul(B).wrapping_add(w);
        }
        for j in 0..last {
            hs.push(hash);
            hash = roll(hash, b[j], b[j + len]);
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
            // The closest occurrence first: a displaced repetition of the run must not
            // steal the anchor, and the first match ends the verification early.
            let mut at = at.clone();
            at.sort_unstable_by_key(|&i| (i.abs_diff(j), i));
            if let Some(i) = at.into_iter().find(|&i| a[i..i + len] == b[j..j + len]) {
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

/// Myers' diff of `a` and `b`: the edit script.
fn myers(a: &[u64], b: &[u64]) -> Vec<Op> {
    let (n, m) = (a.len(), b.len());
    if n == 0 || m == 0 {
        return std::iter::repeat_n(Op::Delete, n).chain(std::iter::repeat_n(Op::Insert, m)).collect();
    }
    let max = n + m;
    let off = max as isize + 1;
    let mut v = vec![0usize; 2 * max + 3];
    // Frontier d only spans diagonals -d..=d: save just that slice.
    let mut trace: Vec<Vec<usize>> = Vec::new();
    let mut found = false;
    'outer: for d in 0..=max as isize {
        trace.push(v[(off - d - 1).max(0) as usize..=(off + d + 1) as usize].to_vec());
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
    if !found {
        // Too different: replace everything.
        return std::iter::repeat_n(Op::Delete, n).chain(std::iter::repeat_n(Op::Insert, m)).collect();
    }
    // Walk back through the saved frontiers.
    let mut ops = Vec::new();
    let (mut x, mut y) = (n as isize, m as isize);
    for d in (0..trace.len() as isize).rev() {
        let v = &trace[d as usize];
        let k = x - y;
        let base = (off - d - 1).max(0);
        let i = |k: isize| (k + off - base) as usize;
        let prev_k = if k == -d || (k != d && v[i(k - 1)] < v[i(k + 1)]) { k + 1 } else { k - 1 };
        let prev_x = v[i(prev_k)] as isize;
        let prev_y = prev_x - prev_k;
        while x > prev_x && y > prev_y {
            ops.push(Op::Equal);
            x -= 1;
            y -= 1;
        }
        if d > 0 {
            ops.push(if x == prev_x { Op::Insert } else { Op::Delete });
        }
        x = prev_x;
        y = prev_y;
    }
    ops.reverse();
    ops
}

pub fn hash(s: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    s.hash(&mut h);
    h.finish()
}
