use crate::Outcome;
use craft_corpus::glob_match;
use serde::Deserialize;
use std::path::Path;

/// One registered intended difference.
///
/// A file matches when every given condition holds: `glob` matches the file name, and for a
/// `Regression` outcome `metric` equals the metric and the value lies in
/// `[min_value, max_value]`. At least one of `glob` and `metric` is required. Entries apply to
/// `Regression`, `OnlyOldFails` and `OnlyNewFails` (the latter two need `glob`, no `metric`).
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Expected {
    pub id: String,
    pub reason: String,
    /// Ticket, Lean theorem or spec clause justifying the change.
    pub evidence: String,
    pub glob: Option<String>,
    pub metric: Option<String>,
    pub min_value: Option<f64>,
    pub max_value: Option<f64>,
}

#[derive(Debug)]
pub enum RegistryError {
    Io(String),
    Parse(String),
    Invalid(String),
}

impl std::fmt::Display for RegistryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(m) | Self::Parse(m) | Self::Invalid(m) => f.write_str(m),
        }
    }
}

impl std::error::Error for RegistryError {}

#[derive(Deserialize)]
struct File {
    #[serde(default)]
    expected: Vec<Expected>,
}

/// The loaded `expected.toml`: `[[expected]]` tables.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Registry {
    entries: Vec<Expected>,
}

impl Registry {
    pub fn new(entries: Vec<Expected>) -> Result<Self, RegistryError> {
        let mut seen = std::collections::BTreeSet::new();
        for e in &entries {
            if e.id.trim().is_empty() || e.reason.trim().is_empty() || e.evidence.trim().is_empty() {
                return Err(RegistryError::Invalid(format!("entry `{}`: id, reason and evidence must be non-empty", e.id)));
            }
            if e.glob.is_none() && e.metric.is_none() {
                return Err(RegistryError::Invalid(format!("entry `{}`: needs `glob` and/or `metric`", e.id)));
            }
            if !seen.insert(e.id.as_str()) {
                return Err(RegistryError::Invalid(format!("duplicate id `{}`", e.id)));
            }
        }
        Ok(Self { entries })
    }

    pub fn from_toml_str(s: &str) -> Result<Self, RegistryError> {
        let f: File = toml::from_str(s).map_err(|e| RegistryError::Parse(e.to_string()))?;
        Self::new(f.expected)
    }

    pub fn from_path(p: &Path) -> Result<Self, RegistryError> {
        let s = std::fs::read_to_string(p).map_err(|e| RegistryError::Io(format!("{}: {e}", p.display())))?;
        Self::from_toml_str(&s)
    }

    pub fn entries(&self) -> &[Expected] {
        &self.entries
    }

    /// Index of the first entry matching `outcome` for file `name`.
    pub(crate) fn find(&self, name: &str, outcome: &Outcome) -> Option<usize> {
        self.entries.iter().position(|e| e.matches(name, outcome))
    }
}

impl Expected {
    fn matches(&self, name: &str, outcome: &Outcome) -> bool {
        if self.glob.as_deref().is_some_and(|g| !glob_match(g, name)) {
            return false;
        }
        match outcome {
            Outcome::Regression { metric, value, .. } => {
                self.metric.as_deref().is_none_or(|m| m == metric)
                    && self.min_value.is_none_or(|lo| *value >= lo)
                    && self.max_value.is_none_or(|hi| *value <= hi)
            }
            Outcome::OnlyOldFails { .. } | Outcome::OnlyNewFails { .. } => {
                self.metric.is_none() && self.min_value.is_none() && self.max_value.is_none()
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const T: &str = r#"
[[expected]]
id = "blend-fix"
reason = "premultiplied blend corrected"
evidence = "cloudbring/artcraft-algorithms#100"
glob = "blend/**"
metric = "max_abs"
max_value = 0.2
"#;

    fn reg(metric: &str, value: f64) -> Outcome {
        Outcome::Regression { metric: metric.into(), value, limit: 0.0 }
    }

    #[test]
    fn matches_glob_and_bound() {
        let r = Registry::from_toml_str(T).unwrap();
        assert_eq!(r.find("blend/a.png", &reg("max_abs", 0.1)), Some(0));
        assert_eq!(r.find("blend/a.png", &reg("max_abs", 0.3)), None);
        assert_eq!(r.find("blend/a.png", &reg("psnr", 0.1)), None);
        assert_eq!(r.find("other/a.png", &reg("max_abs", 0.1)), None);
        assert_eq!(r.find("blend/a.png", &Outcome::Identical), None);
    }

    #[test]
    fn rejects_bad_entries() {
        assert!(Registry::from_toml_str("[[expected]]\nid='a'\nreason='r'\nevidence='e'\n").is_err());
        let dup = format!("{T}{T}");
        assert!(Registry::from_toml_str(&dup).is_err());
        assert!(Registry::from_toml_str("[[expected]]\nid='a'\nreason='r'\nglob='*'\n").is_err());
        assert!(Registry::from_toml_str("[[expected]]\nid='a'\nreason='r'\nevidence='e'\nglob='*'\nbogus=1\n").is_err());
        assert!(Registry::from_toml_str("").unwrap().entries().is_empty());
    }

    #[test]
    fn glob_only_covers_one_sided_failures() {
        let r = Registry::from_toml_str("[[expected]]\nid='x'\nreason='r'\nevidence='e'\nglob='bad/*'\n").unwrap();
        assert_eq!(r.find("bad/f", &Outcome::OnlyOldFails { old_err: "e".into() }), Some(0));
        assert_eq!(r.find("bad/f", &reg("m", 1.0)), Some(0));
        assert_eq!(r.find("bad/f", &Outcome::BothFail { old_err: "a".into(), new_err: "b".into() }), None);
    }
}
