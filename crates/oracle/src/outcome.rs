use serde::Serialize;

/// Which implementation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SideId {
    Old,
    New,
}

/// Result of comparing old and new on one file.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind")]
pub enum Outcome {
    Identical,
    WithinTolerance {
        metric: String,
        value: f64,
    },
    /// A registered intended change (`oracle/expected.toml`).
    ExpectedDifference {
        id: String,
    },
    Regression {
        metric: String,
        value: f64,
        limit: f64,
    },
    BothFail {
        old_err: String,
        new_err: String,
    },
    OnlyOldFails {
        old_err: String,
    },
    OnlyNewFails {
        new_err: String,
    },
    Timeout {
        side: SideId,
    },
    Panic {
        side: SideId,
        msg: String,
    },
}

impl Outcome {
    /// Stable variant name, used for counts.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Identical => "Identical",
            Self::WithinTolerance { .. } => "WithinTolerance",
            Self::ExpectedDifference { .. } => "ExpectedDifference",
            Self::Regression { .. } => "Regression",
            Self::BothFail { .. } => "BothFail",
            Self::OnlyOldFails { .. } => "OnlyOldFails",
            Self::OnlyNewFails { .. } => "OnlyNewFails",
            Self::Timeout { .. } => "Timeout",
            Self::Panic { .. } => "Panic",
        }
    }

    /// All variant names, in report order.
    pub const NAMES: [&'static str; 9] =
        ["Identical", "WithinTolerance", "ExpectedDifference", "Regression", "BothFail", "OnlyOldFails", "OnlyNewFails", "Timeout", "Panic"];
}
