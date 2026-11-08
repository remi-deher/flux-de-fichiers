use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const LEASE_SECONDS: i64 = 120;
pub const MAX_ATTEMPTS: i32 = 3;

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

#[derive(Debug, Serialize, Deserialize)]
pub struct LeaseRequest {
    pub worker_id: Uuid,
    pub lease_token: Uuid,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Failure {
    pub worker_id: Uuid,
    pub lease_token: Uuid,
    pub error_code: String,
}
