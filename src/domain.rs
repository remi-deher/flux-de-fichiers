use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub use flux_shared::flows::{FlowRecord, UpdateFlowRequest};
pub use flux_shared::jobs::{
    Job, JobErrorCode, JobEvent, JobEventType, JobStatus, MAX_ATTEMPTS, MediaFile, NewJob,
};

pub use flux_shared::libraries::{
    LIBRARY_NAME_MAX, LIBRARY_PATH_MAX, Library, LibraryAccess, LibraryWorkerPath, LibraryWrite,
};
pub use flux_shared::workers::{SystemConfig, Worker};

pub const LEASE_SECONDS: i64 = 120;

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
