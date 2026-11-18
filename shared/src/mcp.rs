//! Contrats partagés de l'historique d'activité MCP (ADR 0015).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;

/// Statut d'exécution d'un appel MCP enregistré dans l'historique.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(sqlx::Type))]
#[cfg_attr(feature = "sqlx", sqlx(type_name = "TEXT", rename_all = "snake_case"))]
#[serde(rename_all = "snake_case")]
pub enum McpAuditOutcome {
    Ok,
    Error,
    Forbidden,
    RateLimited,
}

impl McpAuditOutcome {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Error => "error",
            Self::Forbidden => "forbidden",
            Self::RateLimited => "rate_limited",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim() {
            "ok" => Some(Self::Ok),
            "error" => Some(Self::Error),
            "forbidden" => Some(Self::Forbidden),
            "rate_limited" => Some(Self::RateLimited),
            _ => None,
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Ok => "Succès",
            Self::Error => "Erreur",
            Self::Forbidden => "Refusé",
            Self::RateLimited => "Limité",
        }
    }
}

impl fmt::Display for McpAuditOutcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Entrée du journal des actions effectuées via MCP.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpAuditEntry {
    pub id: i64,
    pub key_id: String,
    pub key_name: String,
    pub role: String,
    pub tool_name: String,
    pub target_summary: Option<String>,
    pub outcome: McpAuditOutcome,
    pub error_message: Option<String>,
    pub duration_ms: i64,
    pub created_at: DateTime<Utc>,
}

/// Réponse paginée de `GET /api/v1/mcp/audit`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpAuditListResponse {
    pub entries: Vec<McpAuditEntry>,
    pub total: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outcome_round_trips_on_wire_and_labels() {
        let cases = [
            (McpAuditOutcome::Ok, "ok", "Succès"),
            (McpAuditOutcome::Error, "error", "Erreur"),
            (McpAuditOutcome::Forbidden, "forbidden", "Refusé"),
            (McpAuditOutcome::RateLimited, "rate_limited", "Limité"),
        ];
        for (outcome, wire, label) in cases {
            assert_eq!(outcome.as_str(), wire);
            assert_eq!(outcome.label(), label);
            assert_eq!(McpAuditOutcome::parse(wire), Some(outcome));
            assert_eq!(
                serde_json::to_string(&outcome).ok().as_deref(),
                Some(format!("\"{wire}\"").as_str())
            );
        }
        assert_eq!(McpAuditOutcome::parse("unknown"), None);
    }
}
