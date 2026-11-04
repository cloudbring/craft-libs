//! Layout metrics between two lists of positioned text lines: which lines of a reference are found
//! with the same text in a test rendering (same line break), and how far the matched lines moved.
//!
//! Ported from wordcraft `xtask visual-diff` (`lcs`, the per-page `dy`/`dx`/`same_break`/
//! `page_start_same` columns and the `dx_diffs` and `break_diffs` lists). The lines come from
//! anywhere (`pdftotext -bbox-layout`, a layout engine dump); this module does no I/O.
//!
//! Matching is the longest common subsequence of whitespace-normalised line texts, so it respects
//! reading order. Deviations from the original: none in the numbers; the report is a typed struct
//! instead of JSON and the thresholds are parameters.

/// One text line: page (0-based), top `y`, left `x` (same unit on both sides), text.
#[derive(Clone, Debug, PartialEq)]
pub struct TextLine {
    pub page: usize,
    pub y: f64,
    pub x: f64,
    pub text: String,
}

impl TextLine {
    pub fn new(page: usize, x: f64, y: f64, text: impl Into<String>) -> Self {
        Self { page, y, x, text: text.into() }
    }
}

/// Collapse runs of whitespace to one space and trim, the form lines are matched on.
pub fn normalize_line(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Past this many table cells (`(n+1)*(m+1)`, 4 bytes each: 400 MB) [`lcs`] keeps only the equal
/// leading and trailing lines.
pub const LCS_CELL_LIMIT: usize = 100_000_000;

/// Longest common subsequence of two line lists as matched `(a, b)` index pairs in order. Equal
/// lines at the start and end are matched first, which keeps the table small for near-identical
/// documents.
pub fn lcs(a: &[String], b: &[String]) -> Vec<(usize, usize)> {
    lcs_capped(a, b, LCS_CELL_LIMIT)
}

fn lcs_capped(a: &[String], b: &[String], cell_limit: usize) -> Vec<(usize, usize)> {
    let pre = a.iter().zip(b).take_while(|(x, y)| x == y).count();
    let suf = a[pre..].iter().rev().zip(b[pre..].iter().rev()).take_while(|(x, y)| x == y).count();
    let (ma, mb) = (&a[pre..a.len() - suf], &b[pre..b.len() - suf]);
    let (n, m) = (ma.len(), mb.len());
    let mut out: Vec<(usize, usize)> = (0..pre).map(|i| (i, i)).collect();
    let tail = |out: &mut Vec<(usize, usize)>| out.extend((0..suf).map(|k| (a.len() - suf + k, b.len() - suf + k)));
    if (n + 1).saturating_mul(m + 1) > cell_limit {
        tail(&mut out);
        return out;
    }
    let w = m + 1;
    let mut t = vec![0u32; (n + 1) * w];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            t[i * w + j] = if ma[i] == mb[j] { t[(i + 1) * w + j + 1] + 1 } else { t[(i + 1) * w + j].max(t[i * w + j + 1]) };
        }
    }
    let (mut i, mut j) = (0, 0);
    while i < n && j < m {
        if ma[i] == mb[j] {
            out.push((pre + i, pre + j));
            (i, j) = (i + 1, j + 1);
        } else if t[(i + 1) * w + j] >= t[i * w + j + 1] {
            i += 1;
        } else {
            j += 1;
        }
    }
    tail(&mut out);
    out
}

/// Matched `(reference index, test index)` pairs of two line lists, by normalised text.
pub fn match_lines(reference: &[TextLine], test: &[TextLine]) -> Vec<(usize, usize)> {
    let norm = |l: &[TextLine]| l.iter().map(|x| normalize_line(&x.text)).collect::<Vec<_>>();
    lcs(&norm(reference), &norm(test))
}

/// Per-page numbers; `page` is 0-based. Reference lines are those on this page of the reference.
#[derive(Clone, Debug, PartialEq)]
pub struct PageLayout {
    pub page: usize,
    pub lines_reference: usize,
    pub lines_test: usize,
    /// Percentage of this page's reference lines found with the same text; `None` without lines.
    pub same_break_pct: Option<f64>,
    /// Median `|Δy|` of matched lines that are on this page in both; the upper median for an even
    /// count (index `n / 2` of the sorted values). `None` when there are none.
    pub median_dy: Option<f64>,
    /// Largest `|Δx|` over this page's matched lines (0 when none).
    pub max_dx: f64,
    /// Whether the page's first reference line is matched to the first line of the same page in the
    /// test. `None` when the reference page has no lines.
    pub page_start_same: Option<bool>,
}

/// A matched line that moved horizontally.
#[derive(Clone, Debug, PartialEq)]
pub struct DxOutlier {
    pub page: usize,
    /// `test.x - reference.x`.
    pub dx: f64,
    /// Index into the reference lines.
    pub reference_line: usize,
}

/// A run of lines between two matched lines that differ: a different line break.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BreakRun {
    /// Reference line range `[start, end)`.
    pub reference: (usize, usize),
    /// Test line range `[start, end)`.
    pub test: (usize, usize),
}

/// Result of [`compare_layout`].
#[derive(Clone, Debug, PartialEq)]
pub struct LayoutReport {
    pub lines_reference: usize,
    pub lines_test: usize,
    /// Matched `(reference, test)` line indices.
    pub matches: Vec<(usize, usize)>,
    /// Percentage of reference lines found with the same text; 100 when both lists are empty.
    pub same_break_pct: f64,
    pub pages: Vec<PageLayout>,
    /// Pages whose first line is the same on both sides.
    pub page_starts_same: usize,
    pub breaks: Vec<BreakRun>,
}

impl LayoutReport {
    /// Matched lines with `|Δx|` above `threshold`, largest first.
    pub fn dx_outliers(&self, reference: &[TextLine], test: &[TextLine], threshold: f64) -> Vec<DxOutlier> {
        let mut v: Vec<DxOutlier> = self
            .matches
            .iter()
            .filter_map(|&(i, j)| {
                let dx = test.get(j)?.x - reference.get(i)?.x;
                (dx.abs() > threshold).then(|| DxOutlier { page: reference[i].page, dx, reference_line: i })
            })
            .collect();
        v.sort_by(|a, b| b.dx.abs().total_cmp(&a.dx.abs()));
        v
    }

    /// Largest per-page [`PageLayout::median_dy`] (0 when no page has one).
    pub fn max_median_dy(&self) -> f64 {
        self.pages.iter().filter_map(|p| p.median_dy).fold(0.0, f64::max)
    }

    /// Largest `|Δx|` over all pages.
    pub fn max_dx(&self) -> f64 {
        self.pages.iter().map(|p| p.max_dx).fold(0.0, f64::max)
    }
}

/// Match `reference` and `test` lines and measure per-page movement.
pub fn compare_layout(reference: &[TextLine], test: &[TextLine]) -> LayoutReport {
    let pairs = match_lines(reference, test);
    let mut matched = vec![None; reference.len()];
    for &(a, b) in &pairs {
        matched[a] = Some(b);
    }
    let page_count = reference.iter().chain(test).map(|l| l.page + 1).max().unwrap_or(0);
    let mut pages = Vec::with_capacity(page_count);
    for p in 0..page_count {
        let ref_ix: Vec<usize> = (0..reference.len()).filter(|&i| reference[i].page == p).collect();
        let same: Vec<(usize, usize)> = ref_ix.iter().filter_map(|&i| matched[i].map(|j| (i, j))).collect();
        let mut dys: Vec<f64> = same.iter().filter(|&&(_, j)| test[j].page == p).map(|&(i, j)| (reference[i].y - test[j].y).abs()).collect();
        dys.sort_by(f64::total_cmp);
        let max_dx = same.iter().map(|&(i, j)| (test[j].x - reference[i].x).abs()).fold(0.0, f64::max);
        let page_start_same = ref_ix.first().map(|&first| matched[first].is_some_and(|j| test[j].page == p && (j == 0 || test[j - 1].page != p)));
        pages.push(PageLayout {
            page: p,
            lines_reference: ref_ix.len(),
            lines_test: test.iter().filter(|l| l.page == p).count(),
            same_break_pct: (!ref_ix.is_empty()).then(|| 100.0 * same.len() as f64 / ref_ix.len() as f64),
            median_dy: dys.get(dys.len() / 2).copied(),
            max_dx,
            page_start_same,
        });
    }
    let mut breaks = Vec::new();
    let (mut i0, mut j0) = (0usize, 0usize);
    for &(a, b) in pairs.iter().chain(std::iter::once(&(reference.len(), test.len()))) {
        if a > i0 || b > j0 {
            breaks.push(BreakRun { reference: (i0, a), test: (j0, b) });
        }
        (i0, j0) = (a + 1, b + 1);
    }
    LayoutReport {
        lines_reference: reference.len(),
        lines_test: test.len(),
        same_break_pct: if reference.is_empty() {
            if test.is_empty() { 100.0 } else { 0.0 }
        } else {
            100.0 * pairs.len() as f64 / reference.len() as f64
        },
        page_starts_same: pages.iter().filter(|p| p.page_start_same == Some(true)).count(),
        matches: pairs,
        pages,
        breaks,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(s: &[&str]) -> Vec<String> {
        s.iter().map(|x| (*x).to_string()).collect()
    }

    fn lines(page: usize, rows: &[(f64, f64, &str)]) -> Vec<TextLine> {
        rows.iter().map(|&(x, y, t)| TextLine::new(page, x, y, t)).collect()
    }

    #[test]
    fn lcs_matches_lines_in_order() {
        let a = v(&["intro", "one two", "three", "end"]);
        let b = v(&["intro", "one", "two three", "end"]);
        assert_eq!(lcs(&a, &b), vec![(0, 0), (3, 3)]);
        assert_eq!(lcs(&a, &a).len(), 4);
        assert!(lcs(&a, &[]).is_empty());
    }

    #[test]
    fn lcs_picks_a_longest_subsequence_not_just_ends() {
        let a = v(&["x", "a", "b", "c", "y"]);
        let b = v(&["q", "a", "z", "b", "c", "w"]);
        assert_eq!(lcs(&a, &b), vec![(1, 1), (2, 3), (3, 4)]);
    }

    #[test]
    fn lcs_over_the_cell_limit_keeps_only_equal_ends() {
        let a = v(&["s", "a", "b", "e"]);
        let b = v(&["s", "b", "a", "e"]);
        assert_eq!(lcs_capped(&a, &b, 4), vec![(0, 0), (3, 3)]);
        assert_eq!(lcs_capped(&a, &b, 1000).len(), 3);
    }

    #[test]
    fn normalisation_ignores_whitespace_runs() {
        assert_eq!(normalize_line("  a \t b\n c "), "a b c");
        let r = lines(0, &[(72.0, 10.0, "Hello   world")]);
        let t = lines(0, &[(72.0, 10.0, "Hello world ")]);
        assert_eq!(match_lines(&r, &t), vec![(0, 0)]);
    }

    #[test]
    fn identical_layouts_have_zero_movement() {
        let r = lines(0, &[(72.0, 80.0, "one"), (72.0, 95.0, "two")]);
        let rep = compare_layout(&r, &r);
        assert_eq!(rep.same_break_pct, 100.0);
        assert_eq!(rep.pages.len(), 1);
        assert_eq!(rep.pages[0].median_dy, Some(0.0));
        assert_eq!(rep.max_dx(), 0.0);
        assert_eq!(rep.pages[0].page_start_same, Some(true));
        assert_eq!(rep.page_starts_same, 1);
        assert!(rep.breaks.is_empty());
    }

    #[test]
    fn median_dy_and_max_dx_over_matched_lines() {
        let r = lines(0, &[(72.0, 100.0, "a"), (72.0, 120.0, "b"), (72.0, 140.0, "c"), (72.0, 160.0, "d")]);
        let t = lines(0, &[(72.0, 101.0, "a"), (80.0, 123.0, "b"), (70.0, 140.0, "c"), (72.5, 169.0, "d")]);
        let rep = compare_layout(&r, &t);
        let p = &rep.pages[0];
        // |dy| = 1, 3, 0, 9 sorted 0 1 3 9; index 4/2 = 2 -> 3.
        assert_eq!(p.median_dy, Some(3.0));
        assert_eq!(p.max_dx, 8.0);
        assert_eq!(rep.same_break_pct, 100.0);
        let out = rep.dx_outliers(&r, &t, 6.0);
        assert_eq!(out, vec![DxOutlier { page: 0, dx: 8.0, reference_line: 1 }]);
        assert_eq!(rep.dx_outliers(&r, &t, 1.0).len(), 2);
    }

    #[test]
    fn a_reflowed_line_is_a_break_difference() {
        let r = lines(0, &[(72.0, 80.0, "intro"), (72.0, 95.0, "one two"), (72.0, 110.0, "three"), (72.0, 125.0, "end")]);
        let t = lines(0, &[(72.0, 80.0, "intro"), (72.0, 95.0, "one"), (72.0, 110.0, "two three"), (72.0, 125.0, "end")]);
        let rep = compare_layout(&r, &t);
        assert_eq!(rep.matches, vec![(0, 0), (3, 3)]);
        assert_eq!(rep.same_break_pct, 50.0);
        assert_eq!(rep.breaks, vec![BreakRun { reference: (1, 3), test: (1, 3) }]);
        assert_eq!(rep.pages[0].same_break_pct, Some(50.0));
    }

    #[test]
    fn page_start_differs_when_the_first_line_moves_pages() {
        let mut r = lines(0, &[(72.0, 80.0, "a")]);
        r.extend(lines(1, &[(72.0, 80.0, "b"), (72.0, 95.0, "c")]));
        let mut t = lines(0, &[(72.0, 80.0, "a"), (72.0, 95.0, "b")]);
        t.extend(lines(1, &[(72.0, 80.0, "c")]));
        let rep = compare_layout(&r, &t);
        assert_eq!(rep.pages.len(), 2);
        assert_eq!(rep.pages[0].page_start_same, Some(true));
        assert_eq!(rep.pages[1].page_start_same, Some(false));
        assert_eq!(rep.page_starts_same, 1);
        // "c" is on page 1 in both, "b" is on page 1 in the reference only: no dy for b.
        assert_eq!(rep.pages[1].median_dy, Some(15.0));
        assert_eq!(rep.pages[1].lines_test, 1);
    }

    #[test]
    fn empty_inputs() {
        let rep = compare_layout(&[], &[]);
        assert_eq!((rep.same_break_pct, rep.pages.len()), (100.0, 0));
        let r = lines(0, &[(0.0, 0.0, "a")]);
        let rep = compare_layout(&r, &[]);
        assert_eq!(rep.same_break_pct, 0.0);
        assert_eq!(rep.pages[0].median_dy, None);
        assert_eq!(rep.pages[0].page_start_same, Some(false));
        assert_eq!(rep.breaks, vec![BreakRun { reference: (0, 1), test: (0, 0) }]);
    }
}
