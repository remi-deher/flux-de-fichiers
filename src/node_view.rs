//! Échanges entre la centrale et un worker pour ses vues en lecture seule (ADR 0011, phase 3).
//!
//! La centrale expose des routes limitées au nœud de la clé présentée ; le worker n'a jamais de
//! proxy générique, seulement cette liste de routes.

use crate::domain::JobStatus;
use chrono::{DateTime, Utc};
use flux_shared::Role;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const OVERVIEW_PATH: &str = "/api/v1/node/overview";
pub const JOBS_PATH: &str = "/api/v1/node/jobs";
pub const PLUGINS_PATH: &str = "/api/v1/node/plugins";
pub const LOGIN_PATH: &str = "/api/v1/node/login";
pub const ACCOUNTS_PATH: &str = "/api/v1/node/accounts";
/// Nombre de tâches renvoyées pour un nœud.
pub const NODE_JOBS_LIMIT: i64 = 100;

/// Tâches d'un nœud, par statut.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobCounts {
    pub queued: i64,
    pub running: i64,
    pub succeeded: i64,
    pub failed: i64,
}

impl JobCounts {
    pub fn add(&mut self, status: JobStatus, count: i64) {
        let slot = match status {
            JobStatus::Queued => &mut self.queued,
            JobStatus::Running => &mut self.running,
            JobStatus::Succeeded => &mut self.succeeded,
            JobStatus::Failed => &mut self.failed,
        };
        *slot += count;
    }
}

/// Vue d'ensemble d'un nœud, calculée par la centrale.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeOverview {
    pub name: String,
    pub worker_id: Option<Uuid>,
    pub last_seen: Option<DateTime<Utc>>,
    pub jobs: JobCounts,
}

#[derive(Serialize, Deserialize)]
pub struct VerifyLoginRequest {
    pub username: String,
    pub password: String,
}

/// Compte dont la centrale a vérifié le mot de passe.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerifiedAccount {
    pub id: String,
    pub username: String,
    pub role: Role,
}

/// Compte copié vers un worker : haché Argon2id et version, jamais de mot de passe.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountCopy {
    pub id: String,
    pub username: String,
    pub password_hash: String,
    pub role: Role,
    pub disabled: bool,
    pub version: i64,
}

/// Copie complète des comptes : un compte absent a été supprimé, le worker remplace tout.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountSnapshot {
    pub accounts: Vec<AccountCopy>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_counts_accumulate_per_status() {
        let mut counts = JobCounts::default();
        counts.add(JobStatus::Queued, 2);
        counts.add(JobStatus::Failed, 1);
        counts.add(JobStatus::Queued, 1);
        assert_eq!(
            counts,
            JobCounts {
                queued: 3,
                running: 0,
                succeeded: 0,
                failed: 1
            }
        );
    }

    #[test]
    fn test_overview_uses_camel_case_for_the_browser() -> anyhow::Result<()> {
        let overview = NodeOverview {
            name: "nas".into(),
            worker_id: None,
            last_seen: None,
            jobs: JobCounts::default(),
        };
        let json = serde_json::to_value(&overview)?;
        assert!(json.get("workerId").is_some() && json.get("lastSeen").is_some());
        Ok(())
    }
}
