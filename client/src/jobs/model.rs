//! Règles d'affichage des tâches, sans DOM : libellés, filtres et mise à jour en direct.

use flux_shared::jobs::{Job, JobErrorCode, JobEvent, JobEventType, JobStatus};

/// Onglet de l'écran Activité : la file en direct, l'historique ou les échecs à diagnostiquer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JobFilter {
    Live,
    History,
    Diagnostic,
}

impl JobFilter {
    pub const TABS: [Self; 3] = [Self::Live, Self::History, Self::Diagnostic];

    pub fn tab(self) -> crate::ui::SubTab {
        match self {
            Self::Live => crate::ui::SubTab::new("File en direct", "⚡"),
            Self::History => crate::ui::SubTab::new("Historique", "📜"),
            Self::Diagnostic => crate::ui::SubTab::new("Diagnostic", "🔬"),
        }
    }

    pub fn accepts(self, job: &Job) -> bool {
        match self {
            Self::Live => matches!(job.status, JobStatus::Queued | JobStatus::Running),
            Self::History => matches!(job.status, JobStatus::Succeeded | JobStatus::Failed),
            Self::Diagnostic => job.status == JobStatus::Failed,
        }
    }
}

pub fn status_label(status: JobStatus) -> &'static str {
    match status {
        JobStatus::Queued => "En attente",
        JobStatus::Running => "En cours",
        JobStatus::Succeeded => "Réussie",
        JobStatus::Failed => "Échouée",
    }
}

/// Classe de pastille du thème pour un statut.
pub fn status_tone(status: JobStatus) -> &'static str {
    match status {
        JobStatus::Queued => "info",
        JobStatus::Running => "warning",
        JobStatus::Succeeded => "success",
        JobStatus::Failed => "danger",
    }
}

pub fn error_label(code: JobErrorCode) -> &'static str {
    match code {
        JobErrorCode::RemuxFailed => "Le traitement du média a échoué",
        JobErrorCode::SourceIntegrity => "Source altérée pendant le transfert",
        JobErrorCode::TransferFailed => "Transfert interrompu",
        JobErrorCode::ProbeUnavailable => "Sonde média absente sur le worker",
        JobErrorCode::LeaseExpired => "Worker perdu (bail expiré)",
        JobErrorCode::MissingSourceFingerprint => "Empreinte de la source manquante",
    }
}

/// Un événement du flux SSE impose-t-il de relire la liste ?
pub fn needs_reload(event: JobEventType) -> bool {
    !matches!(
        event,
        JobEventType::JobProgress | JobEventType::JobHeartbeat
    )
}

/// Progression en pourcentage portée par un événement `job_progress`.
pub fn progress_of(event: &JobEvent) -> Option<(uuid::Uuid, u32)> {
    match (event.event_type, event.progress) {
        (JobEventType::JobProgress, Some(progress)) => Some((event.job_id, progress)),
        _ => None,
    }
}

/// Noms d'événements du flux `/api/v1/jobs/events`, un par type.
pub const EVENT_NAMES: [JobEventType; 8] = [
    JobEventType::JobCreated,
    JobEventType::JobClaimed,
    JobEventType::JobHeartbeat,
    JobEventType::JobProgress,
    JobEventType::JobSucceeded,
    JobEventType::JobFailed,
    JobEventType::JobCanceled,
    JobEventType::JobRetried,
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_status_appears_in_the_live_queue_or_the_history() -> Result<(), serde_json::Error> {
        use flux_shared::jobs::{Job, MAX_ATTEMPTS};
        let job: Job = serde_json::from_value(serde_json::json!({
            "id": "00000000-0000-0000-0000-000000000001", "source_name": "a.mkv", "source_sha256": null,
            "source_bytes": null, "output_sha256": null, "output_bytes": null, "warning": false,
            "target_worker_id": null, "worker_id": null, "status": "failed", "attempts": MAX_ATTEMPTS,
            "leased_until": null, "output_name": null, "error_code": null, "flow_id": null,
            "flow_version": null, "created_at": "2026-11-14T10:00:00Z", "updated_at": "2026-11-14T10:00:00Z"
        }))?;
        assert!(JobFilter::History.accepts(&job) && JobFilter::Diagnostic.accepts(&job));
        assert!(!JobFilter::Live.accepts(&job));
        assert_eq!(JobFilter::TABS.len(), 3);
        Ok(())
    }

    #[test]
    fn only_state_changes_reload_the_list() {
        assert!(needs_reload(JobEventType::JobRetried));
        assert!(needs_reload(JobEventType::JobFailed));
        assert!(!needs_reload(JobEventType::JobProgress));
        assert!(!needs_reload(JobEventType::JobHeartbeat));
    }

    #[test]
    fn every_event_type_is_listened_to() {
        let mut names: Vec<_> = EVENT_NAMES.iter().map(|event| event.as_str()).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), EVENT_NAMES.len());
    }

    #[test]
    fn progress_is_read_from_progress_events_only() -> Result<(), serde_json::Error> {
        let event: JobEvent = serde_json::from_value(serde_json::json!({
            "event_type": "job_progress",
            "job_id": "00000000-0000-0000-0000-000000000001",
            "status": "running",
            "timestamp": "2026-11-14T10:00:00Z",
            "progress": 42
        }))?;
        assert_eq!(progress_of(&event).map(|(_, pct)| pct), Some(42));
        let done = JobEvent {
            event_type: JobEventType::JobSucceeded,
            ..event
        };
        assert_eq!(progress_of(&done), None);
        Ok(())
    }

    #[test]
    fn every_error_code_has_a_label() {
        for code in [
            JobErrorCode::RemuxFailed,
            JobErrorCode::SourceIntegrity,
            JobErrorCode::TransferFailed,
            JobErrorCode::ProbeUnavailable,
            JobErrorCode::LeaseExpired,
            JobErrorCode::MissingSourceFingerprint,
        ] {
            assert!(!error_label(code).is_empty());
        }
    }
}
