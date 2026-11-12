use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const LEASE_SECONDS: i64 = 120;
pub const MAX_ATTEMPTS: i32 = 3;

/// Statut d'avancement d'un traitement de remuxage.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Pending,
    Running,
    Succeeded,
    Failed,
    Canceled,
}

impl JobStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Canceled => "canceled",
        }
    }
}

impl std::fmt::Display for JobStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
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
}

impl JobEventType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::JobCreated => "job_created",
            Self::JobClaimed => "job_claimed",
            Self::JobHeartbeat => "job_heartbeat",
            Self::JobProgress => "job_progress",
            Self::JobSucceeded => "job_succeeded",
            Self::JobFailed => "job_failed",
            Self::JobCanceled => "job_canceled",
        }
    }
}

/// Codes d'erreur stricts autorisés lors de l'échec d'un bail de traitement.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobErrorCode {
    RemuxFailed,
    SourceIntegrity,
    TransferFailed,
}

impl JobErrorCode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::RemuxFailed => "remux_failed",
            Self::SourceIntegrity => "source_integrity",
            Self::TransferFailed => "transfer_failed",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "remux_failed" => Some(Self::RemuxFailed),
            "source_integrity" => Some(Self::SourceIntegrity),
            "transfer_failed" => Some(Self::TransferFailed),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct Worker {
    pub id: Uuid,
    pub name: String,
    pub last_seen: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize, sqlx::FromRow)]
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
    pub status: String,
    pub attempts: i32,
    #[serde(skip_serializing)]
    pub lease_token: Option<Uuid>,
    pub leased_until: Option<DateTime<Utc>>,
    pub output_name: Option<String>,
    pub error_code: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JobEvent {
    pub event_type: String,
    pub job_id: Uuid,
    pub status: String,
    pub timestamp: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Claim {
    pub job_id: Uuid,
    pub lease_token: Uuid,
    pub source_name: String,
    pub source_sha256: String,
    pub source_bytes: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Register {
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ClaimRequest {
    pub worker_id: Uuid,
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
