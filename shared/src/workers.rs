//! Contrat des workers et de la configuration système lue par l'interface (ADR 0014).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Nom par défaut du worker intégré au central (`LOCAL_WORKER_NAME` le change).
pub const INTEGRATED_WORKER_NAME: &str = "central-local";

/// Délai sans contact au-delà duquel un worker est présenté hors ligne.
pub const WORKER_ONLINE_SECONDS: i64 = 120;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(sqlx::FromRow))]
pub struct Worker {
    pub id: Uuid,
    pub name: String,
    pub last_seen: DateTime<Utc>,
}

impl Worker {
    /// Le worker intégré au central, sous son nom par défaut.
    pub fn is_integrated(&self) -> bool {
        self.name == INTEGRATED_WORKER_NAME
    }

    /// En ligne si son dernier contact date de moins de `WORKER_ONLINE_SECONDS`.
    pub fn is_online(&self, now: DateTime<Utc>) -> bool {
        (now - self.last_seen).num_seconds() < WORKER_ONLINE_SECONDS
    }
}

/// Réponse de `GET /api/v1/config`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SystemConfig {
    pub max_file_bytes: u64,
    pub local_worker: bool,
    pub version: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presence_follows_the_last_contact() {
        let now = DateTime::<Utc>::UNIX_EPOCH + chrono::Duration::seconds(1_000);
        let worker = |seconds_ago: i64| Worker {
            id: Uuid::nil(),
            name: "nas".into(),
            last_seen: now - chrono::Duration::seconds(seconds_ago),
        };
        assert!(worker(10).is_online(now));
        assert!(!worker(WORKER_ONLINE_SECONDS).is_online(now));
    }
}
