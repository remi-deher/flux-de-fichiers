//! Contrats du journal applicatif lu par l'écran Activité (`GET /api/v1/logs`).

use serde::{Deserialize, Serialize};

pub const DEFAULT_LOGS_LIMIT: usize = 200;
pub const MAX_LOGS_LIMIT: usize = 500;

/// Niveau d'une ligne du journal, du plus bavard au plus grave.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum LogLevel {
    Trace,
    Debug,
    #[default]
    Info,
    Warn,
    Error,
}

impl LogLevel {
    pub const ALL: [Self; 5] = [
        Self::Trace,
        Self::Debug,
        Self::Info,
        Self::Warn,
        Self::Error,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Trace => "trace",
            Self::Debug => "debug",
            Self::Info => "info",
            Self::Warn => "warn",
            Self::Error => "error",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Trace => "Trace",
            Self::Debug => "Debug",
            Self::Info => "Info",
            Self::Warn => "Alerte",
            Self::Error => "Erreur",
        }
    }

    /// Accepte la casse de `tracing` (`WARN`) comme celle de l'API (`warn`).
    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|level| level.as_str().eq_ignore_ascii_case(raw.trim()))
    }
}

/// Champ structuré attaché à une ligne (`provider`, `error`, `elapsed`…).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogField {
    pub name: String,
    pub value: String,
}

/// Une ligne du journal du central.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogEntry {
    /// Horodatage RFC 3339 écrit par `tracing`.
    pub timestamp: String,
    pub level: LogLevel,
    /// Module émetteur, sans le préfixe du crate (`ai`, `mcp`, `query`…).
    pub target: String,
    pub message: String,
    #[serde(default)]
    pub fields: Vec<LogField>,
}

/// Filtres de `GET /api/v1/logs` ; tous sont optionnels.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogsQuery {
    /// Niveau minimal retenu.
    #[serde(default)]
    pub level: Option<LogLevel>,
    /// Texte recherché (message, module et champs), sans tenir compte de la casse.
    #[serde(default)]
    pub search: Option<String>,
    #[serde(default)]
    pub limit: Option<usize>,
}

/// Lignes les plus récentes d'abord.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogsResponse {
    pub entries: Vec<LogEntry>,
    /// Des lignes plus anciennes existent peut-être : la lecture est bornée.
    pub truncated: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_are_ordered_and_parse_both_cases() {
        assert!(LogLevel::Error > LogLevel::Warn && LogLevel::Warn > LogLevel::Info);
        for level in LogLevel::ALL {
            assert_eq!(LogLevel::parse(level.as_str()), Some(level));
            assert_eq!(LogLevel::parse(&level.as_str().to_uppercase()), Some(level));
            assert!(!level.label().is_empty());
        }
        assert_eq!(LogLevel::parse("fatal"), None);
    }
}
