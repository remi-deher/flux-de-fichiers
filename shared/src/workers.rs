//! Contrat des workers et de la configuration système lue par l'interface (ADR 0014).

use crate::jobs::JobStatus;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Nom par défaut du worker intégré au central (`LOCAL_WORKER_NAME` le change).
pub const INTEGRATED_WORKER_NAME: &str = "central-local";

/// Délai sans contact au-delà duquel un worker est présenté hors ligne.
pub const WORKER_ONLINE_SECONDS: i64 = 120;

/// Silence au-delà duquel un worker distant déclenche une alerte de connectivité.
pub const WORKER_SILENT_ALERT_SECONDS: i64 = 600;

/// Connectivité d'un worker d'après l'ancienneté de son dernier contact.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Liveness {
    Online,
    Offline,
    /// Silencieux depuis plus de `WORKER_SILENT_ALERT_SECONDS`.
    Silent,
}

/// Télémétrie matérielle synthétique remontée par un worker (sans dépendance OS).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkerTelemetry {
    pub timestamp: DateTime<Utc>,
    pub logical_cores: u32,
    pub cpu_usage_percent: f32,
    pub ram_total_bytes: u64,
    pub ram_used_bytes: u64,
    pub storage_total_bytes: u64,
    pub storage_available_bytes: u64,
    #[serde(default)]
    pub gpu_devices: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Worker {
    pub id: Uuid,
    pub name: String,
    pub last_seen: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_addr: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub telemetry: Option<WorkerTelemetry>,
}

impl Worker {
    /// Le worker intégré au central, sous son nom par défaut.
    pub fn is_integrated(&self) -> bool {
        self.name == INTEGRATED_WORKER_NAME
    }

    /// Secondes écoulées depuis le dernier contact (jamais négatif).
    pub fn silence_seconds(&self, now: DateTime<Utc>) -> i64 {
        (now - self.last_seen).num_seconds().max(0)
    }

    pub fn liveness(&self, now: DateTime<Utc>) -> Liveness {
        let silence = self.silence_seconds(now);
        if silence >= WORKER_SILENT_ALERT_SECONDS {
            Liveness::Silent
        } else if silence >= WORKER_ONLINE_SECONDS {
            Liveness::Offline
        } else {
            Liveness::Online
        }
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

/// Tâches d'un worker, par statut.
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

/// Vue d'ensemble d'un worker, calculée par le central.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkerOverview {
    pub name: String,
    pub worker_id: Option<Uuid>,
    pub last_seen: Option<DateTime<Utc>>,
    pub jobs: JobCounts,
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
            version: None,
            remote_addr: None,
            telemetry: None,
        };
        assert!(worker(10).is_online(now));
        assert!(!worker(WORKER_ONLINE_SECONDS).is_online(now));
    }

    #[test]
    fn liveness_escalates_from_online_to_silent() {
        let now = DateTime::<Utc>::UNIX_EPOCH + chrono::Duration::seconds(10_000);
        let worker = |seconds_ago: i64| Worker {
            id: Uuid::nil(),
            name: "nas".into(),
            last_seen: now - chrono::Duration::seconds(seconds_ago),
            version: None,
            remote_addr: None,
            telemetry: None,
        };
        assert_eq!(worker(0).liveness(now), Liveness::Online);
        assert_eq!(
            worker(WORKER_ONLINE_SECONDS).liveness(now),
            Liveness::Offline
        );
        assert_eq!(
            worker(WORKER_SILENT_ALERT_SECONDS).liveness(now),
            Liveness::Silent
        );
        assert_eq!(worker(-30).silence_seconds(now), 0);
    }
}
