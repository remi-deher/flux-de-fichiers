//! Contrat des baux : joignabilité du titulaire, protocole et actions irréversibles (ADR 0027).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Silence du titulaire au-delà duquel une tâche en cours devient injoignable.
pub const UNREACHABLE_AFTER_SECONDS: i64 = 30;
/// Délai d'injoignabilité par défaut avant la remise en file.
pub const DEFAULT_UNREACHABLE_TIMEOUT_SECONDS: i64 = 600;
pub const MIN_UNREACHABLE_TIMEOUT_SECONDS: i64 = 60;
pub const MAX_UNREACHABLE_TIMEOUT_SECONDS: i64 = 86_400;
/// En-tête de la réponse au battement : délai d'injoignabilité courant, en secondes.
pub const UNREACHABLE_TIMEOUT_HEADER: &str = "x-unreachable-timeout-seconds";
/// Longueur maximale (en caractères) de l'identifiant de nœud d'une action irréversible.
pub const ACTION_NODE_ID_MAX_CHARS: usize = 128;

/// Le délai d'injoignabilité est-il dans les bornes admises ?
pub fn valid_unreachable_timeout(seconds: i64) -> bool {
    (MIN_UNREACHABLE_TIMEOUT_SECONDS..=MAX_UNREACHABLE_TIMEOUT_SECONDS).contains(&seconds)
}

/// Protocole de bail annoncé par le worker qui réserve (valeur numérique sur le fil).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LeaseProtocol {
    /// Worker antérieur : aucun accord demandé, jamais de remise en file automatique.
    #[default]
    Legacy,
    /// Battement tolérant, accord avant toute action irréversible, restitution.
    Reachability,
}

impl LeaseProtocol {
    pub const LATEST: Self = Self::Reachability;

    pub const fn number(self) -> i64 {
        match self {
            Self::Legacy => 0,
            Self::Reachability => 1,
        }
    }

    /// Une valeur inconnue (worker plus récent) vaut le protocole le plus récent connu.
    pub const fn from_number(number: i64) -> Self {
        if number <= Self::Legacy.number() {
            Self::Legacy
        } else {
            Self::LATEST
        }
    }
}

/// Action que le worker n'exécute qu'avec l'accord du central.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IrreversibleAction {
    /// Opération en place sur la copie de travail (`mkvpropedit`).
    InPlace,
    /// Appel HTTP externe qui n'est pas en lecture seule.
    HttpCall,
    /// Exécution du flux d'échec.
    FailureFlow,
}

impl IrreversibleAction {
    pub const ALL: [Self; 3] = [Self::InPlace, Self::HttpCall, Self::FailureFlow];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InPlace => "in_place",
            Self::HttpCall => "http_call",
            Self::FailureFlow => "failure_flow",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|item| item.as_str() == value)
    }

    /// Exclut-elle la remise en file automatique ? Une opération en place ne touche que la copie de
    /// travail de la tentative perdue.
    pub const fn blocks_requeue(self) -> bool {
        !matches!(self, Self::InPlace)
    }
}

/// Joignabilité du titulaire d'une tâche.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LeaseReachability {
    Reachable,
    Unreachable,
    /// Tâche en file pour un worker désigné qui est hors ligne.
    AwaitingWorker,
}

/// État du bail d'une tâche listée (`GET /api/v1/jobs/leases`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobLeaseInfo {
    pub job_id: Uuid,
    pub reachability: LeaseReachability,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unreachable_since: Option<DateTime<Utc>>,
    /// Instant où la tâche sera reprise au titulaire s'il ne revient pas.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocols_round_trip_and_unknown_values_mean_the_latest() {
        for protocol in [LeaseProtocol::Legacy, LeaseProtocol::Reachability] {
            assert_eq!(LeaseProtocol::from_number(protocol.number()), protocol);
        }
        assert_eq!(LeaseProtocol::from_number(99), LeaseProtocol::LATEST);
        assert_eq!(LeaseProtocol::from_number(-1), LeaseProtocol::Legacy);
    }

    #[test]
    fn actions_round_trip_and_only_in_place_allows_a_requeue() -> Result<(), serde_json::Error> {
        for action in IrreversibleAction::ALL {
            assert_eq!(IrreversibleAction::parse(action.as_str()), Some(action));
            let json = serde_json::to_string(&action)?;
            assert_eq!(json, format!("\"{}\"", action.as_str()));
            let in_place = action == IrreversibleAction::InPlace;
            assert_eq!(action.blocks_requeue(), !in_place);
        }
        assert_eq!(IrreversibleAction::parse("script"), None);
        Ok(())
    }

    #[test]
    fn the_unreachable_timeout_is_bounded() {
        assert!(valid_unreachable_timeout(
            DEFAULT_UNREACHABLE_TIMEOUT_SECONDS
        ));
        assert!(!valid_unreachable_timeout(
            MIN_UNREACHABLE_TIMEOUT_SECONDS - 1
        ));
        assert!(!valid_unreachable_timeout(
            MAX_UNREACHABLE_TIMEOUT_SECONDS + 1
        ));
    }
}
