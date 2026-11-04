//! Exact byte comparison with first-difference offset.

/// Result of [`compare_bytes`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BytesDiff {
    pub equal: bool,
    /// Offset of the first differing byte; if one input is a prefix of the other, the shorter length.
    pub first_diff: Option<usize>,
    pub len_a: usize,
    pub len_b: usize,
}

pub fn compare_bytes(a: &[u8], b: &[u8]) -> BytesDiff {
    let first_diff = a.iter().zip(b).position(|(x, y)| x != y).or_else(|| (a.len() != b.len()).then(|| a.len().min(b.len())));
    BytesDiff { equal: first_diff.is_none(), first_diff, len_a: a.len(), len_b: b.len() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equal_and_differing() {
        assert_eq!(compare_bytes(b"abc", b"abc"), BytesDiff { equal: true, first_diff: None, len_a: 3, len_b: 3 });
        assert_eq!(compare_bytes(b"abcd", b"abXd").first_diff, Some(2));
        assert!(compare_bytes(&[], &[]).equal);
    }

    #[test]
    fn prefix_reports_shorter_length() {
        let d = compare_bytes(b"abc", b"abcde");
        assert!(!d.equal);
        assert_eq!((d.first_diff, d.len_a, d.len_b), (Some(3), 3, 5));
    }
}
