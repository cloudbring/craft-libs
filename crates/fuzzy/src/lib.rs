#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![forbid(unsafe_code)]
//! Cached command-palette fuzzy matching. Positions are zero-based Unicode scalar indices.

mod nfkd;

/// Normalization applied to both query and candidates.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Normalization {
    /// Unicode lowercase only.
    #[default]
    CaseInsensitive,
    /// Compatibility decomposition, removal of marks, then lowercase (Unicode 13.0.0 table).
    NfkdFolded,
}

#[derive(Clone, Debug)]
struct Unit {
    lower: char,
    original: char,
    position: usize,
    bonus: i64,
    word: usize,
}

fn units(text: &str, normalization: Normalization) -> Vec<Unit> {
    let mut result = Vec::new();
    let mut previous: Option<char> = None;
    let mut word = 0usize;
    for (position, original) in text.chars().enumerate() {
        let bonus = if position == 0 {
            16
        } else if previous.is_some_and(|c| c == '/' || c == '\\') {
            15
        } else if previous.is_some_and(|c| !c.is_alphanumeric()) {
            12
        } else if previous.is_some_and(|c| c.is_lowercase()) && original.is_uppercase() {
            10
        } else {
            0
        };
        if position > 0 && bonus > 0 && original.is_alphanumeric() {
            word += 1;
        }
        let decomposed = if normalization == Normalization::NfkdFolded { nfkd::fold(original) } else { None };
        let mut first = true;
        for character in decomposed.into_iter().flat_map(str::chars).chain(decomposed.is_none().then_some(original)).flat_map(char::to_lowercase) {
            result.push(Unit { lower: character, original, position, bonus: if first { bonus } else { 0 }, word });
            first = false;
        }
        previous = Some(original);
    }
    result
}

/// Match score and strictly increasing indices in the original haystack.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Match {
    pub score: i64,
    pub positions: Vec<usize>,
}

/// A cached candidate. Construct once and reuse across query keystrokes.
#[derive(Clone, Debug)]
pub struct Matcher {
    text: String,
    normalized: Vec<Unit>,
    normalization: Normalization,
    length: usize,
}

impl Matcher {
    fn contiguous_match(&self, window: &[Unit], count: usize) -> Match {
        let count = i64::try_from(count).unwrap_or(i64::MAX);
        let baseline = count.saturating_mul(count).saturating_mul(1_000_000_000);
        let boundary = if window[0].bonus > 0 { 10_000_000 } else { 0 };
        let word = 1_000_i64.saturating_sub(i64::try_from(window[0].word).unwrap_or(i64::MAX).min(1_000)) * 1_001;
        let length = 1_000_i64.saturating_sub(i64::try_from(self.length).unwrap_or(i64::MAX).min(1_000));
        let mut positions: Vec<_> = window.iter().map(|unit| unit.position).collect();
        positions.dedup();
        Match { score: baseline.saturating_add(100_000_000 + boundary + word + length), positions }
    }

    pub fn new(text: impl Into<String>, normalization: Normalization) -> Self {
        let text = text.into();
        Self { normalized: units(&text, normalization), length: text.chars().count(), text, normalization }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    /// Finds the highest-scoring subsequence, or `None` if it cannot match.
    pub fn match_query(&self, query: &str) -> Option<Match> {
        if query.is_ascii() && self.normalization == Normalization::CaseInsensitive {
            let mut haystack = self.normalized.iter();
            for byte in query.bytes() {
                let lower = char::from(byte.to_ascii_lowercase());
                if !haystack.any(|unit| unit.lower == lower) {
                    return None;
                }
            }
            if !query.is_empty() && query.len() <= self.normalized.len() {
                let contiguous = self
                    .normalized
                    .windows(query.len())
                    .filter(|window| window.iter().zip(query.bytes()).all(|(unit, byte)| unit.lower == char::from(byte.to_ascii_lowercase())))
                    .max_by_key(|window| (window[0].bonus > 0, std::cmp::Reverse(window[0].word)));
                if let Some(window) = contiguous {
                    return Some(self.contiguous_match(window, query.len()));
                }
            }
        }
        let needle = units(query, self.normalization);
        if needle.is_empty() {
            return Some(Match { score: 0, positions: Vec::new() });
        }
        let width = self.normalized.len();
        if needle.len() > width {
            return None;
        }
        let count = i64::try_from(needle.len()).unwrap_or(i64::MAX);
        let baseline = count.saturating_mul(count).saturating_mul(1_000_000_000);
        let contiguous = self
            .normalized
            .windows(needle.len())
            .enumerate()
            .filter(|(_, window)| window.iter().zip(&needle).all(|(candidate, query)| candidate.lower == query.lower))
            .max_by_key(|(_, window)| (window[0].bonus > 0, std::cmp::Reverse(window[0].word)));
        if let Some((_, window)) = contiguous {
            return Some(self.contiguous_match(window, needle.len()));
        }
        let mut previous = vec![None; width];
        let mut parents = vec![vec![None; width]; needle.len()];
        for (row, query_unit) in needle.iter().enumerate() {
            let mut current = vec![None; width];
            let mut distant: Option<(i64, usize)> = None;
            for column in 0..width {
                if column >= 9
                    && let Some(score) = previous[column - 9]
                    && distant.is_none_or(|(best, _)| score > best)
                {
                    distant = Some((score, column - 9));
                }
                let candidate = &self.normalized[column];
                if candidate.lower != query_unit.lower {
                    continue;
                }
                let reward = 20 + candidate.bonus + i64::from(candidate.original == query_unit.original);
                if row == 0 {
                    current[column] = Some(reward - (column.min(8) as i64));
                    continue;
                }
                let mut best = distant.map(|(score, index)| (score - 8, index));
                for (index, entry) in previous.iter().enumerate().take(column).skip(column.saturating_sub(8)) {
                    if let Some(score) = *entry {
                        let gap = column - index - 1;
                        let score = score + if gap == 0 { 14 } else { -(gap as i64) };
                        if best.is_none_or(|(value, _)| score > value) {
                            best = Some((score, index));
                        }
                    }
                }
                if let Some((score, index)) = best {
                    current[column] = Some(score + reward);
                    parents[row][column] = Some(index);
                }
            }
            previous = current;
        }
        let (mut column, score) = previous
            .iter()
            .enumerate()
            .filter_map(|(index, score)| score.map(|value| (index, value)))
            .max_by_key(|&(index, score)| (score, std::cmp::Reverse(index)))?;
        let mut positions = Vec::with_capacity(needle.len());
        for row in (0..needle.len()).rev() {
            positions.push(self.normalized[column].position);
            if row > 0 {
                column = parents[row][column]?;
            }
        }
        positions.reverse();
        positions.dedup();
        Some(Match { score: baseline.saturating_add(score.clamp(0, 99_999)), positions })
    }
}

/// A ranked result, referring to the input candidate's original index.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RankedMatch {
    pub index: usize,
    pub matched: Match,
}

/// Rank by score descending, then character length ascending, then input order.
/// An empty query preserves input order, irrespective of length.
pub fn rank(items: &[Matcher], query: &str) -> Vec<RankedMatch> {
    let mut matches: Vec<_> =
        items.iter().enumerate().filter_map(|(index, item)| item.match_query(query).map(|matched| RankedMatch { index, matched })).collect();
    if !units(query, items.first().map_or(Normalization::CaseInsensitive, |item| item.normalization)).is_empty() {
        matches.sort_by(|left, right| {
            right
                .matched
                .score
                .cmp(&left.matched.score)
                .then_with(|| items[left.index].length.cmp(&items[right.index].length))
                .then_with(|| left.index.cmp(&right.index))
        });
    }
    matches
}
