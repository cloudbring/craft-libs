//! Corpus floors: metric -> minimum count, in `corpus-floors.toml`. Counts may rise, never fall.
//!
//! ```toml
//! [floors]
//! "pngsuite.files" = 62
//! "pngsuite.decoded" = 60
//! ```

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// Metric name to minimum count.
pub type Counts = BTreeMap<String, u64>;

/// A floors file.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Floors {
    /// The floors, sorted by metric.
    #[serde(default)]
    pub floors: Counts,
}

/// A floor that was broken.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violation {
    /// Metric name.
    pub metric: String,
    /// The floor.
    pub floor: u64,
    /// The measured value; `None` when the metric was not reported at all.
    pub actual: Option<u64>,
}

impl std::fmt::Display for Violation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.actual {
            Some(a) => write!(f, "{}: {a} is below the floor {}", self.metric, self.floor),
            None => write!(f, "{}: not reported (floor {})", self.metric, self.floor),
        }
    }
}

/// One floor moved by [`Floors::raise`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Raised {
    /// Metric name.
    pub metric: String,
    /// Previous floor; `None` for a new metric.
    pub from: Option<u64>,
    /// New floor.
    pub to: u64,
}

impl Floors {
    /// Parses floors text.
    ///
    /// # Errors
    /// [`Error::Floors`] for bad TOML.
    pub fn parse(text: &str) -> Result<Self> {
        toml::from_str(text).map_err(|e| Error::Floors(e.to_string()))
    }

    /// Reads a floors file.
    ///
    /// # Errors
    /// I/O or parse errors.
    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path).map_err(|e| Error::io(format!("read {}", path.display()), e))?;
        Self::parse(&text)
    }

    /// The file text, sorted, with a header comment.
    ///
    /// # Errors
    /// [`Error::Floors`] if serialisation fails.
    pub fn to_toml(&self) -> Result<String> {
        let body = toml::to_string(self).map_err(|e| Error::Floors(e.to_string()))?;
        Ok(format!("# Corpus floors: minimum counts. They only rise (`craft-corpus floors raise`); lowering one needs a reviewed reason.\n{body}"))
    }

    /// Writes the file.
    ///
    /// # Errors
    /// I/O errors.
    pub fn save(&self, path: &Path) -> Result<()> {
        std::fs::write(path, self.to_toml()?).map_err(|e| Error::io(format!("write {}", path.display()), e))
    }

    /// Every floor must be met by `counts`; a floor with no reported count is a violation.
    ///
    /// # Errors
    /// The list of broken floors.
    pub fn check(&self, counts: &Counts) -> std::result::Result<(), Vec<Violation>> {
        let v: Vec<Violation> = self
            .floors
            .iter()
            .filter_map(|(metric, &floor)| {
                let actual = counts.get(metric).copied();
                (actual.is_none_or(|a| a < floor)).then(|| Violation { metric: metric.clone(), floor, actual })
            })
            .collect();
        if v.is_empty() { Ok(()) } else { Err(v) }
    }

    /// Raises floors to `counts` where the count is higher, and adds metrics not yet present.
    /// Never lowers a floor and never removes one.
    pub fn raise(&mut self, counts: &Counts) -> Vec<Raised> {
        let mut changes = Vec::new();
        for (metric, &count) in counts {
            let from = self.floors.get(metric).copied();
            if from.is_none_or(|f| count > f) {
                self.floors.insert(metric.clone(), count);
                changes.push(Raised { metric: metric.clone(), from, to: count });
            }
        }
        changes
    }

    /// Ratchet for CI: compared with the floors on the base branch, no floor may be lower or gone.
    ///
    /// # Errors
    /// One [`Violation`] per floor lowered or removed (`actual` is the new floor, `None` if gone).
    pub fn check_not_lowered(&self, base: &Self) -> std::result::Result<(), Vec<Violation>> {
        let v: Vec<Violation> = base
            .floors
            .iter()
            .filter_map(|(metric, &floor)| {
                let actual = self.floors.get(metric).copied();
                (actual.is_none_or(|a| a < floor)).then(|| Violation { metric: metric.clone(), floor, actual })
            })
            .collect();
        if v.is_empty() { Ok(()) } else { Err(v) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn counts(p: &[(&str, u64)]) -> Counts {
        p.iter().map(|(k, v)| ((*k).to_string(), *v)).collect()
    }

    fn floors(p: &[(&str, u64)]) -> Floors {
        Floors { floors: counts(p) }
    }

    #[test]
    fn round_trips_through_toml() {
        let f = floors(&[("a.files", 3), ("b.files", 9)]);
        let text = f.to_toml().unwrap();
        assert!(text.contains("[floors]"));
        assert_eq!(Floors::parse(&text).unwrap(), f);
    }

    #[test]
    fn check_passes_at_or_above_and_fails_below_or_missing() {
        let f = floors(&[("a", 5), ("b", 2)]);
        assert!(f.check(&counts(&[("a", 5), ("b", 7), ("extra", 1)])).is_ok());
        let v = f.check(&counts(&[("a", 4)])).unwrap_err();
        assert_eq!(v, [Violation { metric: "a".into(), floor: 5, actual: Some(4) }, Violation { metric: "b".into(), floor: 2, actual: None }]);
        assert!(v[0].to_string().contains("below the floor 5"));
    }

    #[test]
    fn raise_only_goes_up_and_adds_new_metrics() {
        let mut f = floors(&[("a", 5), ("b", 5)]);
        let ch = f.raise(&counts(&[("a", 9), ("b", 3), ("c", 1)]));
        assert_eq!(f, floors(&[("a", 9), ("b", 5), ("c", 1)]));
        assert_eq!(ch, [Raised { metric: "a".into(), from: Some(5), to: 9 }, Raised { metric: "c".into(), from: None, to: 1 }]);
        assert!(f.raise(&counts(&[("a", 9)])).is_empty());
    }

    #[test]
    fn ratchet_refuses_lowered_or_removed_floors() {
        let base = floors(&[("a", 5), ("b", 5)]);
        assert!(floors(&[("a", 5), ("b", 6), ("c", 1)]).check_not_lowered(&base).is_ok());
        let v = floors(&[("a", 4)]).check_not_lowered(&base).unwrap_err();
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].actual, Some(4));
        assert_eq!(v[1].actual, None);
    }

    #[test]
    fn rejects_unknown_keys() {
        assert!(Floors::parse("[floor]\na = 1\n").is_err());
    }
}
