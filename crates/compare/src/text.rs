//! Text comparison: word-level F1 and reading-order score.
//!
//! Ported from pdfcraft's `xtask` text oracle (`f1`, `order_score`). Words are lowercased
//! alphanumeric runs. Deviation from the original: two empty texts score 1.0 (the original
//! scored 0.0 for any empty side).

use std::collections::BTreeMap;

/// Cap on words fed to the quadratic reading-order table.
pub const MAX_ORDER_WORDS: usize = 5000;

/// Lowercased alphanumeric words in order.
pub fn words(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).map(str::to_lowercase).collect()
}

fn bag(words: &[String]) -> BTreeMap<&str, usize> {
    let mut m = BTreeMap::new();
    for w in words {
        *m.entry(w.as_str()).or_insert(0) += 1;
    }
    m
}

/// Word-level F1 (multiset overlap; order-blind).
pub fn word_f1(a: &str, b: &str) -> f64 {
    let (wa, wb) = (words(a), words(b));
    if wa.is_empty() && wb.is_empty() {
        return 1.0;
    }
    let (ba, bb) = (bag(&wa), bag(&wb));
    let common: usize = ba.iter().map(|(w, n)| (*n).min(bb.get(w).copied().unwrap_or(0))).sum();
    let (na, nb) = (wa.len(), wb.len());
    if na == 0 || nb == 0 || common == 0 {
        return 0.0;
    }
    let (p, r) = (common as f64 / na as f64, common as f64 / nb as f64);
    2.0 * p * r / (p + r)
}

/// Reading-order score: longest common subsequence of the word sequences over the longer
/// length. Catches columns read interleaved, which F1 cannot see. Inputs longer than
/// [`MAX_ORDER_WORDS`] are truncated.
pub fn reading_order_score(a: &str, b: &str) -> f64 {
    let (wa, wb) = (words(a), words(b));
    if wa.is_empty() && wb.is_empty() {
        return 1.0;
    }
    let a = &wa[..wa.len().min(MAX_ORDER_WORDS)];
    let b = &wb[..wb.len().min(MAX_ORDER_WORDS)];
    let (n, m) = (a.len(), b.len());
    if n == 0 || m == 0 {
        return 0.0;
    }
    let (mut prev, mut cur) = (vec![0u32; m + 1], vec![0u32; m + 1]);
    for x in a {
        for (j, y) in b.iter().enumerate() {
            cur[j + 1] = if x == y { prev[j].saturating_add(1) } else { cur[j].max(prev[j + 1]) };
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    f64::from(prev[m]) / n.max(m) as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn f1_hand_cases() {
        assert_eq!(word_f1("a b c", "a b c"), 1.0);
        assert_eq!(word_f1("a b c", "x y z"), 0.0);
        // common 2; p = 2/3, r = 2/2 -> F1 = 0.8
        assert!((word_f1("a b c", "a b") - 0.8).abs() < 1e-12);
        // Multiset: "a a" vs "a": common 1, p = 1/2, r = 1 -> 2/3.
        assert!((word_f1("a a", "a") - 2.0 / 3.0).abs() < 1e-12);
        assert_eq!(word_f1("", "a"), 0.0);
        assert_eq!(word_f1("", ""), 1.0);
        assert_eq!(word_f1("Hello, WORLD!", "hello world"), 1.0);
    }

    #[test]
    fn reading_order_sees_what_f1_cannot() {
        let columns = "left one left two left three right one right two right three";
        let woven = "left one right one left two right two left three right three";
        assert_eq!(word_f1(woven, columns), 1.0);
        assert!(reading_order_score(woven, columns) < 0.7);
        assert_eq!(reading_order_score(columns, columns), 1.0);
        assert_eq!(reading_order_score("", columns), 0.0);
        assert_eq!(reading_order_score("", ""), 1.0);
        // LCS of "a b c d" vs "a c d" is 3 over max length 4.
        assert!((reading_order_score("a b c d", "a c d") - 0.75).abs() < 1e-12);
    }
}
