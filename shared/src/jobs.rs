//! Contrat des tâches : partagé par l'API du central et le client web (ADR 0014).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use uuid::Uuid;

/// Nombre maximal de tentatives d'une tâche, relances manuelles comprises.
pub const MAX_ATTEMPTS: i32 = 3;

/// Statuts persistés par `jobs.status` (CHECK SQL : queued, running, succeeded, failed).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(sqlx::Type), sqlx(rename_all = "snake_case"))]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
}

impl JobStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
        }
    }
}

impl fmt::Display for JobStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for JobStatus {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "queued" => Ok(Self::Queued),
            "running" => Ok(Self::Running),
            "succeeded" => Ok(Self::Succeeded),
            "failed" => Ok(Self::Failed),
            _ => Err(()),
        }
    }
}

/// Nature d'un événement notifié sur le flux SSE.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobEventType {
    JobCreated,
    JobClaimed,
    JobHeartbeat,
    JobProgress,
    JobSucceeded,
    JobFailed,
    JobCanceled,
    JobRetried,
}

impl JobEventType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::JobCreated => "job_created",
            Self::JobClaimed => "job_claimed",
            Self::JobHeartbeat => "job_heartbeat",
            Self::JobProgress => "job_progress",
            Self::JobSucceeded => "job_succeeded",
            Self::JobFailed => "job_failed",
            Self::JobCanceled => "job_canceled",
            Self::JobRetried => "job_retried",
        }
    }
}

impl fmt::Display for JobEventType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Codes d'erreur persistés dans `jobs.error_code`.
///
/// Seuls `remux_failed`, `source_integrity`, `transfer_failed` et
/// `probe_unavailable` peuvent être déclarés par un worker. `lease_expired` et
/// `missing_source_fingerprint` sont écrits par le central.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(sqlx::Type), sqlx(rename_all = "snake_case"))]
#[serde(rename_all = "snake_case")]
pub enum JobErrorCode {
    RemuxFailed,
    SourceIntegrity,
    TransferFailed,
    ProbeUnavailable,
    LeaseExpired,
    MissingSourceFingerprint,
}

impl JobErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::RemuxFailed => "remux_failed",
            Self::SourceIntegrity => "source_integrity",
            Self::TransferFailed => "transfer_failed",
            Self::ProbeUnavailable => "probe_unavailable",
            Self::LeaseExpired => "lease_expired",
            Self::MissingSourceFingerprint => "missing_source_fingerprint",
        }
    }

    pub fn is_worker_reportable(self) -> bool {
        matches!(
            self,
            Self::RemuxFailed
                | Self::SourceIntegrity
                | Self::TransferFailed
                | Self::ProbeUnavailable
        )
    }

    /// Traduit un libellé interne de traitement en code déclarable par un worker.
    pub fn from_worker_label(label: &str) -> Self {
        match label {
            "source_integrity" => Self::SourceIntegrity,
            "remux_failed" => Self::RemuxFailed,
            "probe_unavailable" => Self::ProbeUnavailable,
            _ => Self::TransferFailed,
        }
    }
}

impl fmt::Display for JobErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for JobErrorCode {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "remux_failed" => Ok(Self::RemuxFailed),
            "source_integrity" => Ok(Self::SourceIntegrity),
            "transfer_failed" => Ok(Self::TransferFailed),
            "probe_unavailable" => Ok(Self::ProbeUnavailable),
            "lease_expired" => Ok(Self::LeaseExpired),
            "missing_source_fingerprint" => Ok(Self::MissingSourceFingerprint),
            _ => Err(()),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(sqlx::FromRow))]
pub struct Job {
    pub id: Uuid,
    pub source_name: String,
    pub source_sha256: Option<String>,
    pub source_bytes: Option<i64>,
    pub output_sha256: Option<String>,
    pub output_bytes: Option<i64>,
    pub warning: bool,
    pub target_worker_id: Option<Uuid>,
    pub worker_id: Option<Uuid>,
    pub status: JobStatus,
    pub attempts: i32,
    #[serde(skip_serializing)]
    pub lease_token: Option<Uuid>,
    pub leased_until: Option<DateTime<Utc>>,
    pub output_name: Option<String>,
    pub error_code: Option<JobErrorCode>,
    /// Flow et version figés à la création de la tâche (aucun si pas de flow par défaut).
    pub flow_id: Option<String>,
    pub flow_version: Option<i64>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JobEvent {
    pub event_type: JobEventType,
    pub job_id: Uuid,
    pub status: JobStatus,
    pub timestamp: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<JobErrorCode>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NewJob {
    pub source_name: String,
    pub target_worker_id: Option<Uuid>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MediaFile {
    pub name: String,
    pub bytes: u64,
}

impl Job {
    /// Relançable à la main : échec par perte de bail, sous la limite de tentatives (ADR 0012).
    pub fn is_retryable(&self) -> bool {
        self.status == JobStatus::Failed
            && self.error_code == Some(JobErrorCode::LeaseExpired)
            && self.attempts < MAX_ATTEMPTS
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job(status: JobStatus, error_code: Option<JobErrorCode>, attempts: i32) -> Job {
        let now = DateTime::<Utc>::UNIX_EPOCH;
        Job {
            id: Uuid::nil(),
            source_name: "film.mkv".into(),
            source_sha256: None,
            source_bytes: None,
            output_sha256: None,
            output_bytes: None,
            warning: false,
            target_worker_id: None,
            worker_id: None,
            status,
            attempts,
            lease_token: None,
            leased_until: None,
            output_name: None,
            error_code,
            flow_id: None,
            flow_version: None,
            created_at: now,
            updated_at: now,
        }
    }

    #[test]
    fn only_a_lost_lease_under_the_attempt_limit_is_retryable() {
        let lost = Some(JobErrorCode::LeaseExpired);
        assert!(job(JobStatus::Failed, lost, 1).is_retryable());
        assert!(!job(JobStatus::Failed, lost, MAX_ATTEMPTS).is_retryable());
        assert!(!job(JobStatus::Failed, Some(JobErrorCode::RemuxFailed), 1).is_retryable());
        assert!(!job(JobStatus::Running, None, 1).is_retryable());
    }

    #[test]
    fn a_job_read_from_the_api_has_no_lease_token() -> Result<(), serde_json::Error> {
        let json = serde_json::to_value(job(JobStatus::Queued, None, 0))?;
        assert!(json.get("lease_token").is_none());
        let back: Job = serde_json::from_value(json)?;
        assert_eq!(back.lease_token, None);
        Ok(())
    }
}
