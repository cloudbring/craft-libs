use craft_fuzzy::{Matcher, Normalization, rank};
use proptest::prelude::*;

fn item(text: &str) -> Matcher {
    Matcher::new(text, Normalization::CaseInsensitive)
}
fn score(text: &str, query: &str) -> i64 {
    item(text).match_query(query).map_or(i64::MIN, |m| m.score)
}

#[test]
fn rankings_and_empty_query() {
    let items: Vec<_> = ["Drop shadow", "Copy", "Open"].iter().map(|text| item(text)).collect();
    assert_eq!(rank(&items, "op").iter().map(|m| m.index).collect::<Vec<_>>(), [2, 1, 0]);
    assert_eq!(rank(&items, "").iter().map(|m| m.index).collect::<Vec<_>>(), [0, 1, 2]);
    assert!(item("Smart Sharpen").match_query("ssh").is_some());
    assert!(item("Open").match_query("xyz").is_none());
    assert!(score("Foo Bar", "fb") > score("Foobar", "fb"));
    assert!(score("abcd", "abc") > score("a b c d", "abc"));
}

#[test]
fn normalization_and_highlights() {
    let matcher = Matcher::new("É ﬁsh 🐟", Normalization::NfkdFolded);
    let result = matcher.match_query("e fish");
    assert_eq!(result.map(|m| m.positions), Some(vec![0, 1, 2, 3, 4]));
    assert_eq!(item("a🦀B").match_query("🦀b").map(|m| m.positions), Some(vec![1, 2]));
    assert_eq!(Matcher::new("cafe\u{301}", Normalization::NfkdFolded).match_query("café").map(|m| m.positions), Some(vec![0, 1, 2, 3]));
}

#[test]
fn exact_and_ties() {
    assert!(score("ab", "ab") > score("a b", "ab"));
    let items: Vec<_> = ["abxx", "ab", "ab", "abx"].iter().map(|text| item(text)).collect();
    assert_eq!(rank(&items, "ab").iter().map(|entry| entry.index).collect::<Vec<_>>(), [1, 2, 3, 0]);
}

proptest! {
    #[test]
    fn positions_are_increasing_and_valid(hay in "[a-zA-Z ]{0,40}", query in "[a-zA-Z ]{0,12}") {
        if let Some(found) = item(&hay).match_query(&query) {
            prop_assert!(found.positions.windows(2).all(|pair| pair[0] < pair[1]));
            let chars: Vec<_> = hay.chars().collect();
            let matched: String = found.positions.iter().map(|&index| chars[index].to_ascii_lowercase()).collect();
            prop_assert_eq!(matched, query.to_ascii_lowercase());
        }
    }

    #[test]
    fn adding_a_matching_character_never_reduces_score(hay in "[a-d]{1,30}", query in "[a-d]{0,8}", extra in "[a-d]") {
        let longer = format!("{query}{extra}");
        if let (Some(short), Some(long)) = (item(&hay).match_query(&query), item(&hay).match_query(&longer)) {
            prop_assert!(long.score >= short.score);
        }
    }
}

#[test]
#[ignore = "manual benchmark: 10,000 commands × 100 queries"]
fn benchmark_palette() {
    let items: Vec<_> = (0..10_000).map(|index| item(&format!("Open document {index} shadow"))).collect();
    let start = std::time::Instant::now();
    for index in 0..100 {
        std::hint::black_box(rank(&items, &format!("op{}", index % 10)));
    }
    eprintln!("10k commands × 100 queries: {:?}", start.elapsed());
}
