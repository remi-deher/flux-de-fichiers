//! Échanges entre le central et un worker pour ses vues en lecture seule (ADR 0011, phase 3).
//!
//! Le central expose des routes limitées au worker de la clé présentée ; le worker n'a jamais de
//! proxy générique, seulement cette liste de routes.

use flux_shared::Role;
pub use flux_shared::workers::{JobCounts, WorkerOverview};
use serde::{Deserialize, Serialize};

pub const OVERVIEW_PATH: &str = "/api/v1/worker/overview";
pub const JOBS_PATH: &str = "/api/v1/worker/jobs";
pub const PLUGINS_PATH: &str = "/api/v1/worker/plugins";
pub const LOGIN_PATH: &str = "/api/v1/worker/login";
pub const ACCOUNTS_PATH: &str = "/api/v1/worker/accounts";
/// Nombre de tâches renvoyées pour un worker.
pub const WORKER_JOBS_LIMIT: i64 = 100;

#[derive(Serialize, Deserialize)]
pub struct VerifyLoginRequest {
    pub username: String,
    pub password: String,
}

/// Compte dont le central a vérifié le mot de passe.
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
    use crate::domain::JobStatus;

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
        let overview = WorkerOverview {
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
