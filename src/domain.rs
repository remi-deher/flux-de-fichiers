use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub use forgearr_shared::flows::{FlowRecord, FlowVersionSummary, UpdateFlowRequest};
pub use forgearr_shared::jobs::{
    JOB_PRIORITY_MAX, JOB_PRIORITY_MIN, Job, JobBatchAction, JobBatchRequest, JobBatchResult,
    JobErrorCode, JobEvent, JobEventType, JobStatus, JobStepLog, JobStepStatus, JobStorageStats,
    LibraryJobStats, MAX_ATTEMPTS, MediaFile, NewJob, UpdateJobPriorityRequest,
};

pub use forgearr_shared::libraries::{
    DirectoryEntry, DirectoryListing, LIBRARY_NAME_MAX, LIBRARY_PATH_MAX, Library, LibraryAccess,
    LibraryOutputPolicy, LibraryWorkerPath, LibraryWrite, MAX_SCAN_INTERVAL_MINUTES,
    MediaFileEntry, RootShortcut, WorkerBrowseReport, WorkerBrowseTask, library_accepts_file,
};
pub use forgearr_shared::workers::{
    SystemConfig, UpdateWorkerSettingsRequest, WORKER_MAX_CONCURRENCY, WORKER_MIN_CONCURRENCY,
    Worker, WorkerSchedule,
};

pub const LEASE_SECONDS: i64 = 120;

/// Outil et version attendue pour exécuter une tâche (dérivés du flux figé).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolRequirement {
    pub tool_id: String,
    pub version: String,
}

/// Étape exécutée d'un flux, remontée par un worker ou le runner local.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobStepEntry {
    pub step_index: i32,
    pub node_id: String,
    pub block_kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation_id: Option<String>,
    pub status: JobStepStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_excerpt: Option<String>,
    /// Variables personnalisées (`vars.*`) figées pour une notification différée après publication.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variables: Option<serde_json::Map<String, serde_json::Value>>,
}

/// Rapport d'étapes transmis par un worker distant au central.
#[derive(Debug, Serialize, Deserialize)]
pub struct StepsReport {
    pub worker_id: Uuid,
    pub lease_token: Uuid,
    #[serde(default)]
    pub lease_epoch: i64,
    pub steps: Vec<JobStepEntry>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Claim {
    pub job_id: Uuid,
    pub lease_token: Uuid,
    #[serde(default)]
    pub lease_epoch: i64,
    pub source_name: String,
    pub source_sha256: String,
    pub source_bytes: i64,
    /// Sonde exigée par le catalogue au moment du bail. Absente si aucun outil
    /// `probe` n'est le choix actuel.
    #[serde(default)]
    pub probe_tool_id: Option<String>,
    #[serde(default)]
    pub probe_version: Option<String>,
    /// Outils exigés par le flux figé (et leurs versions du catalogue).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_tools: Vec<ToolRequirement>,
    /// Bibliothèque associée à la tâche, si renseignée.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub library_id: Option<Uuid>,
    /// Version de flux figée à la création de la tâche ; voyage avec le bail pour
    /// que workers distants et intégré exécutent la même définition.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frozen_flow: Option<FlowRecord>,
    /// Contrats d'opérations JSON nécessaires au worker pour exécuter le flux sans logique figée.
    #[serde(default, skip_serializing_if = "std::collections::HashMap::is_empty")]
    pub operations: std::collections::HashMap<String, serde_json::Value>,
    /// Table de correspondance bloc / alias -> `operationId` pour ce bail.
    #[serde(default, skip_serializing_if = "std::collections::HashMap::is_empty")]
    pub block_operations: std::collections::HashMap<String, String>,
    /// Réglages de plugins déchiffrés pour les seules opérations HTTP du flux figé.
    #[serde(default, skip_serializing_if = "std::collections::HashMap::is_empty")]
    pub plugin_configs: std::collections::HashMap<String, serde_json::Value>,
    /// Secrets du bail (réglages de plugins et en-têtes webhook) chiffrés pour le worker détenteur.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sealed_secrets: Option<String>,
    /// Politique réseau sortante pour les intégrations / webhooks exécutées par le worker.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allow_private_integrations: Option<bool>,
    /// Dossier source tel que vu par le central (pour les traductions de chemins d'intégration).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media_root: Option<std::path::PathBuf>,
    /// Racine locale à ce worker, fournie seulement pour les tâches épinglées à son montage.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_source_root: Option<String>,
    /// Nœud du flux où reprendre une tâche relayée par un autre worker (ADR 0018).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resume_node_id: Option<String>,
    /// Copie de travail reçue avant ce bail, si celui-ci suit une remise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handoff_input: Option<forgearr_shared::relay::HandoffInput>,
    /// Avertissement accumulé avant un éventuel transfert.
    #[serde(default)]
    pub prior_warning: bool,
    /// Billet pour lire la source chez le worker qui la détient (ADR 0019).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_ticket: Option<forgearr_shared::transfer::SourceTicket>,
    /// Nom de la bibliothèque de la tâche (variable de flux `library.name`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub library_name: Option<String>,
    /// Nom du worker détenteur du bail (variable de flux `worker.name`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worker_name: Option<String>,
    /// Variables personnalisées (`vars.*`) transmises par le worker qui a relayé la tâche.
    #[serde(default, skip_serializing_if = "serde_json::Map::is_empty")]
    pub resume_variables: serde_json::Map<String, serde_json::Value>,
    /// Versions figées des sous-flux et du flux d'échec de la tâche (ADR 0022).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub linked_flows: Vec<forgearr_shared::flows::LinkedFlow>,
}

impl Claim {
    /// Flux figé principal puis flux liés : tous ceux que le worker peut exécuter pour ce bail.
    pub fn executable_flows(&self) -> impl Iterator<Item = &FlowRecord> {
        self.frozen_flow
            .iter()
            .chain(self.linked_flows.iter().map(|linked| &linked.flow))
    }

    /// Version mutable de [`Claim::executable_flows`] (chiffrement des en-têtes).
    pub fn executable_flows_mut(&mut self) -> impl Iterator<Item = &mut FlowRecord> {
        self.frozen_flow
            .iter_mut()
            .chain(self.linked_flows.iter_mut().map(|linked| &mut linked.flow))
    }

    /// Le fichier à sonder peut être le résultat intermédiaire d'un worker précédent.
    pub fn working_source_name(&self) -> &str {
        self.handoff_input
            .as_ref()
            .map_or(self.source_name.as_str(), |input| {
                input.artifact.name.as_str()
            })
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Register {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub telemetry: Option<crate::telemetry::TelemetrySnapshot>,
    /// Adresse du serveur de transfert de ce worker, si elle est annoncée (ADR 0019).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transfer_url: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ClaimRequest {
    pub worker_id: Uuid,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub telemetry: Option<crate::telemetry::TelemetrySnapshot>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LeaseRequest {
    pub worker_id: Uuid,
    pub lease_token: Uuid,
    #[serde(default)]
    pub lease_epoch: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub telemetry: Option<crate::telemetry::TelemetrySnapshot>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Failure {
    pub worker_id: Uuid,
    pub lease_token: Uuid,
    #[serde(default)]
    pub lease_epoch: i64,
    pub error_code: String,
}

/// Progression remontée par un worker distant pendant l'exécution (remux / flux).
#[derive(Debug, Serialize, Deserialize)]
pub struct ProgressReport {
    pub worker_id: Uuid,
    pub lease_token: Uuid,
    #[serde(default)]
    pub lease_epoch: i64,
    pub progress: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
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
            (JobStatus::Canceled, "canceled"),
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
        assert!("cancelled".parse::<JobStatus>().is_err());
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
            (JobErrorCode::OperationFailed, "operation_failed", true),
            (JobErrorCode::WebhookFailed, "webhook_failed", true),
            (JobErrorCode::IntegrationFailed, "integration_failed", true),
            (JobErrorCode::SourceIntegrity, "source_integrity", true),
            (JobErrorCode::SourceChanged, "source_changed", true),
            (
                JobErrorCode::SourcePublicationUncertain,
                "source_publication_uncertain",
                false,
            ),
            (JobErrorCode::TransferFailed, "transfer_failed", true),
            (JobErrorCode::ProbeUnavailable, "probe_unavailable", true),
            (JobErrorCode::LeaseExpired, "lease_expired", false),
            (
                JobErrorCode::MissingSourceFingerprint,
                "missing_source_fingerprint",
                false,
            ),
            (JobErrorCode::NoCapableWorker, "no_capable_worker", false),
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
            JobErrorCode::from_worker_label("operation_failed"),
            JobErrorCode::OperationFailed
        );
        assert_eq!(
            JobErrorCode::from_worker_label("remux_failed"),
            JobErrorCode::OperationFailed
        );
        assert_eq!(
            JobErrorCode::from_worker_label("anything-else"),
            JobErrorCode::TransferFailed
        );
        Ok(())
    }

    #[test]
    fn claim_carries_optional_frozen_flow_on_the_wire() -> Result<(), serde_json::Error> {
        let bare: Claim = serde_json::from_str(
            r#"{"job_id":"00000000-0000-0000-0000-000000000001","lease_token":"00000000-0000-0000-0000-000000000002","source_name":"a.mkv","source_sha256":"ab","source_bytes":1}"#,
        )?;
        assert!(bare.frozen_flow.is_none());
        assert!(bare.operations.is_empty());
        assert!(bare.plugin_configs.is_empty());
        let with_flow = Claim {
            job_id: Uuid::nil(),
            lease_token: Uuid::nil(),
            lease_epoch: 1,
            source_name: "a.mkv".into(),
            source_sha256: "ab".into(),
            source_bytes: 1,
            probe_tool_id: None,
            probe_version: None,
            required_tools: Vec::new(),
            library_id: None,
            frozen_flow: Some(FlowRecord {
                id: "flow.x".into(),
                name: "X".into(),
                description: None,
                file_mask: "*.mkv".into(),
                is_default: true,
                definition: "{}".into(),
                version: 3,
                created_at: chrono::DateTime::UNIX_EPOCH,
                updated_at: chrono::DateTime::UNIX_EPOCH,
            }),
            operations: std::collections::HashMap::new(),
            block_operations: std::collections::HashMap::new(),
            plugin_configs: std::collections::HashMap::new(),
            sealed_secrets: None,
            allow_private_integrations: None,
            media_root: None,
            local_source_root: None,
            resume_node_id: None,
            handoff_input: None,
            prior_warning: false,
            source_ticket: None,
            library_name: None,
            worker_name: None,
            resume_variables: serde_json::Map::new(),
            linked_flows: Vec::new(),
        };
        let encoded = serde_json::to_value(&with_flow)?;
        assert_eq!(encoded["frozen_flow"]["id"], "flow.x");
        assert_eq!(encoded["frozen_flow"]["version"], 3);
        assert!(encoded.get("operations").is_none());
        assert!(encoded.get("plugin_configs").is_none());
        assert!(encoded.get("resume_variables").is_none());
        Ok(())
    }

    #[test]
    fn claim_without_flow_variables_still_deserializes() -> Result<(), serde_json::Error> {
        let claim: Claim = serde_json::from_value(serde_json::json!({
            "job_id": Uuid::nil(),
            "lease_token": Uuid::nil(),
            "source_name": "a.mkv",
            "source_sha256": "ab",
            "source_bytes": 1
        }))?;
        assert!(claim.library_name.is_none());
        assert!(claim.worker_name.is_none());
        assert!(claim.resume_variables.is_empty());
        Ok(())
    }

    #[test]
    fn progress_report_roundtrip() -> Result<(), serde_json::Error> {
        let report = ProgressReport {
            worker_id: Uuid::nil(),
            lease_token: Uuid::from_u128(1),
            lease_epoch: 2,
            progress: 42,
            message: Some("Progress: 42%".into()),
        };
        let encoded = serde_json::to_value(&report)?;
        assert_eq!(encoded["progress"], 42);
        assert_eq!(encoded["message"], "Progress: 42%");
        let bare: ProgressReport = serde_json::from_str(
            r#"{"worker_id":"00000000-0000-0000-0000-000000000000","lease_token":"00000000-0000-0000-0000-000000000001","progress":7}"#,
        )?;
        assert_eq!(bare.progress, 7);
        assert!(bare.message.is_none());
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
