use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub use flux_shared::jobs::{
    Job, JobErrorCode, JobEvent, JobEventType, JobStatus, MAX_ATTEMPTS, MediaFile, NewJob,
};

pub const LEASE_SECONDS: i64 = 120;

#[derive(Clone, Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct Worker {
    pub id: Uuid,
    pub name: String,
    pub last_seen: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Claim {
    pub job_id: Uuid,
    pub lease_token: Uuid,
    pub source_name: String,
    pub source_sha256: String,
    pub source_bytes: i64,
    /// Sonde exigée par le catalogue au moment du bail. Absente si aucun outil
    /// `probe` n'est le choix actuel.
    #[serde(default)]
    pub probe_tool_id: Option<String>,
    #[serde(default)]
    pub probe_version: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Register {
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ClaimRequest {
    pub worker_id: Uuid,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LeaseRequest {
    pub worker_id: Uuid,
    pub lease_token: Uuid,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub telemetry: Option<crate::telemetry::TelemetrySnapshot>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Failure {
    pub worker_id: Uuid,
    pub lease_token: Uuid,
    pub error_code: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct FlowRecord {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub file_mask: String,
    pub is_default: bool,
    pub definition: String,
    /// Dernière version enregistrée ; les versions passées sont immuables (`flow_versions`).
    pub version: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateFlowRequest {
    pub name: String,
    pub description: Option<String>,
    pub file_mask: String,
    pub is_default: bool,
    pub definition: String,
}

pub const LIBRARY_NAME_MAX: usize = 80;
pub const LIBRARY_PATH_MAX: usize = 512;

/// Accès d'un worker au dossier de la bibliothèque.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(rename_all = "snake_case")]
pub enum LibraryAccess {
    Read,
    ReadWrite,
}

/// Chemin local d'une bibliothèque sur un worker.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LibraryWorkerPath {
    pub worker_id: Uuid,
    pub local_path: String,
    pub access: LibraryAccess,
}

/// Bibliothèque déclarée : un nom, un flux, des workers.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Library {
    pub id: Uuid,
    pub name: String,
    pub flow_id: Option<String>,
    pub workers: Vec<LibraryWorkerPath>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Création ou remplacement d'une bibliothèque.
#[derive(Debug, Deserialize)]
pub struct LibraryWrite {
    pub name: String,
    #[serde(default)]
    pub flow_id: Option<String>,
    #[serde(default)]
    pub workers: Vec<LibraryWorkerPath>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn job_status_matches_check_constraint() -> Result<(), serde_json::Error> {
        let cases = [
            (JobStatus::Queued, "queued"),
            (JobStatus::Running, "running"),
            (JobStatus::Succeeded, "succeeded"),
            (JobStatus::Failed, "failed"),
        ];
        for (status, label) in cases {
            assert_eq!(status.as_str(), label);
            assert_eq!(status.to_string(), label);
            assert_eq!(label.parse::<JobStatus>(), Ok(status));
            let json = serde_json::to_string(&status)?;
            assert_eq!(json, format!("\"{label}\""));
            assert_eq!(serde_json::from_str::<JobStatus>(&json)?, status);
        }
        assert!("pending".parse::<JobStatus>().is_err());
        assert!("canceled".parse::<JobStatus>().is_err());
        Ok(())
    }

    #[test]
    fn job_event_type_wire_labels() -> Result<(), serde_json::Error> {
        let cases = [
            (JobEventType::JobCreated, "job_created"),
            (JobEventType::JobClaimed, "job_claimed"),
            (JobEventType::JobHeartbeat, "job_heartbeat"),
            (JobEventType::JobProgress, "job_progress"),
            (JobEventType::JobSucceeded, "job_succeeded"),
            (JobEventType::JobFailed, "job_failed"),
            (JobEventType::JobCanceled, "job_canceled"),
        ];
        for (event_type, label) in cases {
            assert_eq!(event_type.as_str(), label);
            assert_eq!(
                serde_json::from_str::<JobEventType>(&serde_json::to_string(&event_type)?)?,
                event_type
            );
        }
        Ok(())
    }

    #[test]
    fn job_error_code_roundtrip_and_worker_policy() -> Result<(), serde_json::Error> {
        let cases = [
            (JobErrorCode::RemuxFailed, "remux_failed", true),
            (JobErrorCode::SourceIntegrity, "source_integrity", true),
            (JobErrorCode::TransferFailed, "transfer_failed", true),
            (JobErrorCode::ProbeUnavailable, "probe_unavailable", true),
            (JobErrorCode::LeaseExpired, "lease_expired", false),
            (
                JobErrorCode::MissingSourceFingerprint,
                "missing_source_fingerprint",
                false,
            ),
        ];
        for (code, label, reportable) in cases {
            assert_eq!(code.as_str(), label);
            assert_eq!(code.to_string(), label);
            assert_eq!(label.parse::<JobErrorCode>(), Ok(code));
            assert_eq!(code.is_worker_reportable(), reportable);
            assert_eq!(
                serde_json::from_str::<JobErrorCode>(&serde_json::to_string(&code)?)?,
                code
            );
        }
        assert!("nope".parse::<JobErrorCode>().is_err());
        assert_eq!(
            JobErrorCode::from_worker_label("source_integrity"),
            JobErrorCode::SourceIntegrity
        );
        assert_eq!(
            JobErrorCode::from_worker_label("remux_failed"),
            JobErrorCode::RemuxFailed
        );
        assert_eq!(
            JobErrorCode::from_worker_label("anything-else"),
            JobErrorCode::TransferFailed
        );
        Ok(())
    }

    #[test]
    fn library_access_serializes_snake_case() -> Result<(), serde_json::Error> {
        assert_eq!(serde_json::to_string(&LibraryAccess::Read)?, "\"read\"");
        assert_eq!(
            serde_json::to_string(&LibraryAccess::ReadWrite)?,
            "\"read_write\""
        );
        assert_eq!(
            serde_json::from_str::<LibraryAccess>("\"read_write\"")?,
            LibraryAccess::ReadWrite
        );
        Ok(())
    }
}
