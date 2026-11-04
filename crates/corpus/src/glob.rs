//! Minimal path globs for git sources: `*` and `?` stay inside one path segment, `**` crosses `/`.

/// True when `path` matches `pattern`.
#[must_use]
pub fn glob_match(pattern: &str, path: &str) -> bool {
    matches(pattern.as_bytes(), path.as_bytes())
}

fn matches(p: &[u8], s: &[u8]) -> bool {
    match p.split_first() {
        None => s.is_empty(),
        Some((b'*', rest)) => {
            if let Some(after) = rest.strip_prefix(b"*") {
                // `**` (optionally followed by `/`) matches any run, slashes included.
                let after = after.strip_prefix(b"/").unwrap_or(after);
                (0..=s.len()).any(|i| s.get(i..).is_some_and(|tail| matches(after, tail)))
            } else {
                let stop = s.iter().position(|&c| c == b'/').unwrap_or(s.len());
                (0..=stop).any(|i| s.get(i..).is_some_and(|tail| matches(rest, tail)))
            }
        }
        Some((b'?', rest)) => s.split_first().is_some_and(|(&c, tail)| c != b'/' && matches(rest, tail)),
        Some((&c, rest)) => s.split_first().is_some_and(|(&d, tail)| c == d && matches(rest, tail)),
    }
}

#[cfg(test)]
mod tests {
    use super::glob_match;

    #[test]
    fn star_stays_in_segment() {
        assert!(glob_match("a/*.png", "a/x.png"));
        assert!(!glob_match("a/*.png", "a/b/x.png"));
        assert!(glob_match("a/*", "a/"));
    }

    #[test]
    fn double_star_crosses_segments() {
        assert!(glob_match("a/**/x.png", "a/b/c/x.png"));
        assert!(glob_match("a/**/x.png", "a/x.png"));
        assert!(glob_match("**", "any/thing/at/all"));
        assert!(!glob_match("a/**/x.png", "b/x.png"));
    }

    #[test]
    fn question_and_literals() {
        assert!(glob_match("img?.png", "img1.png"));
        assert!(!glob_match("img?.png", "img12.png"));
        assert!(!glob_match("img?.png", "img/.png"));
        assert!(glob_match("exact", "exact"));
        assert!(!glob_match("exact", "exactly"));
    }
}
