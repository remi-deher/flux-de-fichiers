//! Contrat du diagnostic réseau (`POST /api/v1/system/diagnostics`, réservé aux administrateurs).

use serde::{Deserialize, Serialize};

/// Verdict d'un contrôle : vert, orange ou rouge dans l'interface.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckStatus {
    Ok,
    Warning,
    Error,
}

impl CheckStatus {
    /// Gravité croissante, pour retenir le pire verdict d'un rapport.
    const fn severity(self) -> u8 {
        match self {
            Self::Ok => 0,
            Self::Warning => 1,
            Self::Error => 2,
        }
    }

    pub fn worst(self, other: Self) -> Self {
        if other.severity() > self.severity() {
            other
        } else {
            self
        }
    }
}

/// Contrôle réalisé par le central.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckId {
    PublicOrigin,
    Tls,
    ProxyHeaders,
    EventStream,
    Workers,
    Smtp,
}

impl CheckId {
    pub const fn title(self) -> &'static str {
        match self {
            Self::PublicOrigin => "Adresse publique",
            Self::Tls => "TLS",
            Self::ProxyHeaders => "En-têtes du proxy",
            Self::EventStream => "Progression en direct (SSE)",
            Self::Workers => "Workers joignables",
            Self::Smtp => "Courriel (SMTP)",
        }
    }
}

/// Origine du navigateur que l'interface transmet au central pour la comparer à `WEB_ORIGIN`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticsRequest {
    pub browser_origin: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticCheck {
    pub id: CheckId,
    pub status: CheckStatus,
    /// Ce qui a été constaté.
    pub explanation: String,
    /// Comment corriger ; absent quand le contrôle est vert.
    pub fix: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosticsReport {
    pub checks: Vec<DiagnosticCheck>,
}

impl DiagnosticsReport {
    pub fn overall(&self) -> CheckStatus {
        self.checks
            .iter()
            .fold(CheckStatus::Ok, |worst, check| worst.worst(check.status))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(status: CheckStatus) -> DiagnosticCheck {
        DiagnosticCheck {
            id: CheckId::Tls,
            status,
            explanation: String::new(),
            fix: None,
        }
    }

    #[test]
    fn the_report_keeps_the_worst_verdict() {
        let report = |statuses: &[CheckStatus]| DiagnosticsReport {
            checks: statuses.iter().copied().map(check).collect(),
        };
        assert_eq!(report(&[]).overall(), CheckStatus::Ok);
        assert_eq!(
            report(&[CheckStatus::Ok, CheckStatus::Warning]).overall(),
            CheckStatus::Warning
        );
        assert_eq!(
            report(&[CheckStatus::Error, CheckStatus::Warning]).overall(),
            CheckStatus::Error
        );
    }

    #[test]
    fn statuses_and_ids_serialize_in_snake_case() -> serde_json::Result<()> {
        assert_eq!(serde_json::to_string(&CheckStatus::Warning)?, "\"warning\"");
        assert_eq!(
            serde_json::to_string(&CheckId::EventStream)?,
            "\"event_stream\""
        );
        Ok(())
    }
}
