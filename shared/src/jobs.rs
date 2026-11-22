//! Contrat des tâches : partagé par l'API du central et le client web (ADR 0014).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::Path;
use std::str::FromStr;
use uuid::Uuid;

mod lease;
mod resource;

pub use lease::{
    ACTION_NODE_ID_MAX_CHARS, DEFAULT_UNREACHABLE_TIMEOUT_SECONDS, IrreversibleAction,
    JobLeaseInfo, LeaseProtocol, LeaseReachability, MAX_UNREACHABLE_TIMEOUT_SECONDS,
    MIN_UNREACHABLE_TIMEOUT_SECONDS, UNREACHABLE_AFTER_SECONDS, UNREACHABLE_TIMEOUT_HEADER,
    valid_unreachable_timeout,
};
pub use resource::{JobResourceClass, JobResourceInfo};

/// Nombre maximal de tentatives enchaînées ; une relance manuelle d'une tâche qui les a épuisées
/// remet le compte à zéro.
pub const MAX_ATTEMPTS: i32 = 3;
const MAX_WORKING_EXT_LEN: usize = 16;

/// Retourne un nom de fichier de travail sûr conservant l'extension de la source (`source.<ext>`).
pub fn source_working_filename(source_name: &str) -> String {
    match Path::new(source_name)
        .extension()
        .and_then(|ext| ext.to_str())
        .filter(|ext| {
            !ext.is_empty()
                && ext.len() <= MAX_WORKING_EXT_LEN
                && ext.chars().all(|c| c.is_ascii_alphanumeric())
        }) {
        Some(ext) => format!("source.{ext}"),
        None => "source".to_owned(),
    }
}

/// Statuts persistés par `jobs.status` (CHECK SQL : queued, running, succeeded, failed, canceled).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(sqlx::Type), sqlx(rename_all = "snake_case"))]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
    Canceled,
}

impl JobStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Canceled => "canceled",
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
            "canceled" => Ok(Self::Canceled),
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
    JobRelayed,
    /// Titulaire silencieux : la tâche reste la sienne (ADR 0027).
    JobUnreachable,
    /// Le titulaire est revenu avant l'échéance.
    JobRestored,
    /// Bail perdu : la tâche est de nouveau en file.
    JobRequeued,
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
            Self::JobRelayed => "job_relayed",
            Self::JobUnreachable => "job_unreachable",
            Self::JobRestored => "job_restored",
            Self::JobRequeued => "job_requeued",
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
/// Les codes liés au traitement ou transfert peuvent être déclarés par un worker ;
/// `lease_expired` et `missing_source_fingerprint` sont écrits par le central.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(sqlx::Type), sqlx(rename_all = "snake_case"))]
#[serde(rename_all = "snake_case")]
pub enum JobErrorCode {
    #[serde(alias = "remux_failed", alias = "encode_failed")]
    OperationFailed,
    WebhookFailed,
    IntegrationFailed,
    SourceIntegrity,
    SourceChanged,
    SourcePublicationUncertain,
    TransferFailed,
    ProbeUnavailable,
    LeaseExpired,
    MissingSourceFingerprint,
    NoCapableWorker,
}

impl JobErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::OperationFailed => "operation_failed",
            Self::WebhookFailed => "webhook_failed",
            Self::IntegrationFailed => "integration_failed",
            Self::SourceIntegrity => "source_integrity",
            Self::SourceChanged => "source_changed",
            Self::SourcePublicationUncertain => "source_publication_uncertain",
            Self::TransferFailed => "transfer_failed",
            Self::ProbeUnavailable => "probe_unavailable",
            Self::LeaseExpired => "lease_expired",
            Self::MissingSourceFingerprint => "missing_source_fingerprint",
            Self::NoCapableWorker => "no_capable_worker",
        }
    }

    pub fn is_worker_reportable(self) -> bool {
        matches!(
            self,
            Self::OperationFailed
                | Self::WebhookFailed
                | Self::IntegrationFailed
                | Self::SourceIntegrity
                | Self::SourceChanged
                | Self::TransferFailed
                | Self::ProbeUnavailable
        )
    }

    /// Traduit un libellé interne de traitement en code déclarable par un worker.
    pub fn from_worker_label(label: &str) -> Self {
        match label {
            "source_integrity" => Self::SourceIntegrity,
            "source_changed" => Self::SourceChanged,
            "operation_failed" | "remux_failed" | "encode_failed" => Self::OperationFailed,
            "webhook_failed" => Self::WebhookFailed,
            "integration_failed" => Self::IntegrationFailed,
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
            "operation_failed" | "remux_failed" | "encode_failed" => Ok(Self::OperationFailed),
            "webhook_failed" => Ok(Self::WebhookFailed),
            "integration_failed" => Ok(Self::IntegrationFailed),
            "source_integrity" => Ok(Self::SourceIntegrity),
            "source_changed" => Ok(Self::SourceChanged),
            "source_publication_uncertain" => Ok(Self::SourcePublicationUncertain),
            "transfer_failed" => Ok(Self::TransferFailed),
            "probe_unavailable" => Ok(Self::ProbeUnavailable),
            "lease_expired" => Ok(Self::LeaseExpired),
            "missing_source_fingerprint" => Ok(Self::MissingSourceFingerprint),
            "no_capable_worker" => Ok(Self::NoCapableWorker),
            _ => Err(()),
        }
    }
}

/// Statut d'exécution d'un bloc dans un flux.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(sqlx::Type), sqlx(rename_all = "snake_case"))]
#[serde(rename_all = "snake_case")]
pub enum JobStepStatus {
    Succeeded,
    Failed,
    Skipped,
}

impl JobStepStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Skipped => "skipped",
        }
    }
}

/// Entrée de journal d'exécution d'une étape de flux (persistée et consultable via API).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(sqlx::FromRow))]
pub struct JobStepLog {
    pub id: i64,
    pub job_id: Uuid,
    pub step_index: i32,
    pub node_id: String,
    pub block_kind: String,
    pub operation_id: Option<String>,
    pub status: JobStepStatus,
    pub message: Option<String>,
    pub output_excerpt: Option<String>,
    pub created_at: DateTime<Utc>,
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
    #[serde(default)]
    pub lease_epoch: i64,
    #[serde(skip_serializing)]
    pub lease_token: Option<Uuid>,
    pub leased_until: Option<DateTime<Utc>>,
    pub output_name: Option<String>,
    pub error_code: Option<JobErrorCode>,
    /// Bibliothèque associée à la création de la tâche, si renseignée.
    #[serde(default)]
    pub library_id: Option<Uuid>,
    /// Flow et version figés à la création de la tâche (aucun si pas de flow par défaut).
    pub flow_id: Option<String>,
    pub flow_version: Option<i64>,
    #[serde(default)]
    pub priority: i32,
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

/// Priorité minimale et maximale admise sur une tâche.
pub const JOB_PRIORITY_MIN: i32 = -100;
pub const JOB_PRIORITY_MAX: i32 = 100;

/// Priorité d'une bibliothèque qui n'en déclare pas, et donc des tâches créées sans priorité.
pub const LIBRARY_DEFAULT_JOB_PRIORITY: i32 = 0;

pub const fn default_library_job_priority() -> i32 {
    LIBRARY_DEFAULT_JOB_PRIORITY
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct NewJob {
    pub source_name: String,
    pub target_worker_id: Option<Uuid>,
    #[serde(default)]
    pub library_id: Option<Uuid>,
    #[serde(default)]
    pub flow_id: Option<String>,
    /// Absente : priorité par défaut de la bibliothèque, sinon `LIBRARY_DEFAULT_JOB_PRIORITY`.
    #[serde(default)]
    pub priority: Option<i32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobBatchAction {
    Retry,
    Cancel,
    Delete,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobBatchRequest {
    pub ids: Vec<Uuid>,
    pub action: JobBatchAction,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobBatchResult {
    pub updated: usize,
    pub skipped: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateJobPriorityRequest {
    pub priority: i32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryJobStats {
    pub library_id: Option<Uuid>,
    pub library_name: String,
    pub succeeded_count: i64,
    pub failed_count: i64,
    pub source_bytes: i64,
    pub output_bytes: i64,
    pub saved_bytes: i64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobStorageStats {
    pub total_jobs: i64,
    pub succeeded_jobs: i64,
    pub failed_jobs: i64,
    pub canceled_jobs: i64,
    pub total_source_bytes: i64,
    pub total_output_bytes: i64,
    pub saved_bytes: i64,
    pub by_library: Vec<LibraryJobStats>,
    #[serde(default)]
    pub by_volume: Vec<VolumeJobStats>,
    #[serde(default)]
    pub active_jobs: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MediaFile {
    pub name: String,
    pub bytes: u64,
}

impl Job {
    /// Relançable à la main : échec par perte de bail, tentatives épuisées comprises (ADR 0027).
    pub fn is_retryable(&self) -> bool {
        self.status == JobStatus::Failed
            && self.error_code == Some(JobErrorCode::LeaseExpired)
            && self.output_name.is_none()
    }

    /// Annulable à la main : tâche en attente ou en cours d'exécution.
    pub fn is_cancelable(&self) -> bool {
        matches!(self.status, JobStatus::Queued | JobStatus::Running)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_job_priority_is_optional_and_keeps_explicit_values() -> Result<(), serde_json::Error> {
        let inherited: NewJob =
            serde_json::from_str(r#"{"source_name":"a.mkv","target_worker_id":null}"#)?;
        assert_eq!(inherited.priority, None);
        let explicit: NewJob = serde_json::from_str(
            r#"{"source_name":"a.mkv","target_worker_id":null,"priority":5}"#,
        )?;
        assert_eq!(explicit.priority, Some(5));
        assert_eq!(default_library_job_priority(), LIBRARY_DEFAULT_JOB_PRIORITY);
        Ok(())
    }

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
            lease_epoch: 0,
            lease_token: None,
            leased_until: None,
            output_name: None,
            error_code,
            library_id: None,
            flow_id: None,
            flow_version: None,
            priority: 0,
            created_at: now,
            updated_at: now,
        }
    }

    #[test]
    fn only_a_lost_lease_is_retryable_even_without_attempts_left() {
        let lost = Some(JobErrorCode::LeaseExpired);
        assert!(job(JobStatus::Failed, lost, 1).is_retryable());
        assert!(job(JobStatus::Failed, lost, MAX_ATTEMPTS).is_retryable());
        assert!(!job(JobStatus::Failed, Some(JobErrorCode::OperationFailed), 1).is_retryable());
        assert!(!job(JobStatus::Running, None, 1).is_retryable());
        let mut pending_result = job(JobStatus::Failed, lost, 1);
        pending_result.output_name = Some("result.mkv".into());
        assert!(!pending_result.is_retryable());
    }

    #[test]
    fn source_working_filename_preserves_safe_extension_and_rejects_traversal() {
        assert_eq!(source_working_filename("mon_livre.epub"), "source.epub");
        assert_eq!(source_working_filename("Film.MKV"), "source.MKV");
        assert_eq!(source_working_filename("../weird/file"), "source");
        assert_eq!(source_working_filename("bad.tar..gz"), "source.gz");
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

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct JobListFilter {
    pub state: Option<JobListState>,
    pub library: Option<Uuid>,
    pub volume: Option<String>,
    pub error: Option<JobErrorCode>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum JobListState {
    Active,
    Completed,
    Succeeded,
    Failed,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct JobFailureDetail {
    pub job_id: Uuid,
    pub step_index: i32,
    pub block_kind: String,
    pub message: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct FailureReplay {
    pub ids: Vec<Uuid>,
    pub version: Option<i64>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VolumeJobStats {
    pub library_id: Option<Uuid>,
    pub volume: String,
    pub key: String,
    pub saved_bytes: i64,
    pub grown_bytes: i64,
    pub grown_count: i64,
}
