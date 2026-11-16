//! Ton sémantique (`MetricTone`) partagé par les tuiles, jauges et bandeaux KPI.

use serde::{Deserialize, Serialize};

/// Ton sémantique d'une tuile ; le thème le traduit en couleurs via `data-tone`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MetricTone {
    #[default]
    Neutral,
    Success,
    Warning,
    Danger,
}

impl MetricTone {
    pub const fn as_attr(self) -> &'static str {
        match self {
            Self::Neutral => "neutral",
            Self::Success => "success",
            Self::Warning => "warning",
            Self::Danger => "danger",
        }
    }
}
