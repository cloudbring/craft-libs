use craft_fuzzy::{Matcher, Normalization};
// Verbatim: /Users/emm/ArtCraft/photocraft/crates/ui-egui/src/dialogs/palette.rs:9
mod photocraft {
    pub fn fuzzy_score(query: &str, text: &str) -> Option<i32> {
        if query.is_empty() {
            return Some(0);
        }
        let t: Vec<char> = text.to_lowercase().chars().collect();
        let mut score = 0;
        let mut ti = 0;
        let mut last = None;
        for qc in query.to_lowercase().chars() {
            let mut found = false;
            while ti < t.len() {
                if t[ti] == qc {
                    score += if last == Some(ti.wrapping_sub(1)) { 8 } else { 1 };
                    if ti == 0 || t[ti - 1] == ' ' {
                        score += 5;
                    }
                    last = Some(ti);
                    ti += 1;
                    found = true;
                    break;
                }
                ti += 1;
            }
            if !found {
                return None;
            }
        }
        Some(score - (t.len() as i32 / 8))
    }
}
// Verbatim: /Users/emm/ArtCraft/soundcraft/crates/ui-egui/src/palette.rs:34
mod soundcraft {
    fn score(hay: &str, needle: &str) -> Option<i32> {
        if needle.is_empty() {
            return Some(0);
        }
        let h = hay.to_lowercase();
        let n = needle.to_lowercase();
        if let Some(i) = h.find(&n) {
            return Some(1000 - i32::try_from(i).unwrap_or(999));
        }
        let mut it = h.chars();
        let mut gaps = 0;
        for c in n.chars() {
            let mut found = false;
            for hc in it.by_ref() {
                if hc == c {
                    found = true;
                    break;
                }
                gaps += 1;
            }
            if !found {
                return None;
            }
        }
        Some(500 - gaps)
    }
    pub(super) fn call(hay: &str, needle: &str) -> Option<i32> {
        score(hay, needle)
    }
}
// Verbatim: /Users/emm/ArtCraft/craft-anything/crates/ui-egui/src/palette.rs:40
mod anything {
    pub fn score(hay: &str, query: &str) -> Option<i32> {
        if query.is_empty() {
            return Some(0);
        }
        let hay = hay.to_lowercase();
        let mut score = 0i32;
        let mut last: Option<usize> = None;
        let mut chars = hay.char_indices();
        for q in query.to_lowercase().chars().filter(|c| !c.is_whitespace()) {
            let (i, _) = chars.by_ref().find(|(_, c)| *c == q)?;
            score += match last {
                Some(l) if i == l + 1 => 8,
                _ => 1,
            };
            if i == 0 {
                score += 6;
            }
            last = Some(i);
        }
        if hay.contains(&query.to_lowercase()) {
            score += 30;
        }
        Some(score - (hay.len() as i32 / 16))
    }
}

fn current(text: &str, query: &str) -> i64 {
    Matcher::new(text, Normalization::CaseInsensitive).match_query(query).map_or(i64::MIN, |m| m.score)
}

#[test]
fn agree_on_prefix_and_substring() {
    assert!(photocraft::fuzzy_score("op", "Open") > photocraft::fuzzy_score("op", "Copy"));
    assert!(soundcraft::call("Open", "op") > soundcraft::call("Copy", "op"));
    assert!(anything::score("Open", "op") > anything::score("Copy", "op"));
    assert!(current("Open", "op") > current("Copy", "op"));
}

#[test]
fn optimal_alignment_finds_later_word_start() {
    let result = Matcher::new("Paste Special", Normalization::CaseInsensitive).match_query("ps");
    assert_eq!(result.map(|m| m.positions), Some(vec![0, 6]));
    assert!(photocraft::fuzzy_score("ps", "Paste Special").is_some());
    assert!(anything::score("Paste Special", "ps").is_some());
    assert!(soundcraft::call("Paste Special", "ps").is_some());
}

#[test]
fn word_boundary_can_beat_an_interior_substring() {
    assert!(soundcraft::call("xxab", "ab") > soundcraft::call("A Big", "ab"));
    assert!(anything::score("xxab", "ab") > anything::score("A Big", "ab"));
    assert!(current("A Big", "ab") > current("xxab", "ab"));
}
