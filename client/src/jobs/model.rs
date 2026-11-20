//! Règles d'affichage des tâches, sans DOM : libellés et mise à jour en direct.

use forgearr_shared::jobs::{Job, JobErrorCode, JobEvent, JobEventType, JobStatus, JobStepStatus};
use std::collections::HashMap;
use uuid::Uuid;

const DIGEST_PREVIEW_CHARS: usize = 12;
const SHORT_UUID_CHARS: usize = 8;

pub fn step_status_label(status: JobStepStatus) -> &'static str {
    match status {
        JobStepStatus::Succeeded => "Réussie",
        JobStepStatus::Failed => "Échouée",
        JobStepStatus::Skipped => "Ignorée",
    }
}

pub fn step_status_tone(status: JobStepStatus) -> &'static str {
    match status {
        JobStepStatus::Succeeded => "success",
        JobStepStatus::Failed => "danger",
        JobStepStatus::Skipped => "warning",
    }
}

///Variation relative entre taille source et taille produite (ex. `-42 %`).
pub fn format_size_delta(source_bytes: Option<i64>, output_bytes: Option<i64>) -> Option<String> {
    let src = source_bytes.filter(|&v| v > 0)?;
    let out = output_bytes.filter(|&v| v > 0)?;
    let pct = ((out as f64 - src as f64) / src as f64) * 100.0;
    let rounded = pct.round() as i64;
    if rounded > 0 {
        Some(format!("+{rounded} %"))
    } else {
        Some(format!("{rounded} %"))
    }
}

/// Raccourcit une empreinte SHA-256 pour l'affichage tabulaire.
pub fn short_digest(digest: Option<&str>) -> String {
    match digest.map(str::trim).filter(|s| !s.is_empty()) {
        Some(value) if value.len() > DIGEST_PREVIEW_CHARS => {
            let prefix: String = value.chars().take(DIGEST_PREVIEW_CHARS).collect();
            format!("{prefix}…")
        }
        Some(value) => value.to_owned(),
        None => "—".to_owned(),
    }
}

/// Résout le nom d'une bibliothèque liée, avec repli sur l'identifiant court.
pub fn resolve_library_label(
    library_id: Option<Uuid>,
    libraries: &HashMap<Uuid, String>,
) -> Option<String> {
    let id = library_id?;
    Some(
        libraries
            .get(&id)
            .cloned()
            .unwrap_or_else(|| id.to_string().chars().take(SHORT_UUID_CHARS).collect()),
    )
}

pub fn status_label(status: JobStatus) -> &'static str {
    match status {
        JobStatus::Queued => "En attente",
        JobStatus::Running => "En cours",
        JobStatus::Succeeded => "Réussie",
        JobStatus::Failed => "Échouée",
        JobStatus::Canceled => "Annulée",
    }
}

/// Classe de pastille du thème pour un statut.
pub fn status_tone(status: JobStatus) -> &'static str {
    match status {
        JobStatus::Queued => "info",
        JobStatus::Running => "warning",
        JobStatus::Succeeded => "success",
        JobStatus::Failed => "danger",
        JobStatus::Canceled => "neutral",
    }
}

/// Libellé lisible d'une priorité de tâche.
pub fn priority_label(priority: i32) -> String {
    if priority >= 10 {
        format!("Haute ({priority})")
    } else if priority > 0 {
        format!("+{priority}")
    } else if priority < 0 {
        format!("Basse ({priority})")
    } else {
        "Normale".to_owned()
    }
}

pub fn error_label(code: JobErrorCode) -> &'static str {
    match code {
        JobErrorCode::OperationFailed => "L'opération du flux a échoué",
        JobErrorCode::WebhookFailed => "L'appel webhook du flux a échoué",
        JobErrorCode::IntegrationFailed => "L'intégration externe a échoué",
        JobErrorCode::SourceIntegrity => "Source altérée pendant le transfert",
        JobErrorCode::SourceChanged => "La source a changé depuis la découverte",
        JobErrorCode::SourcePublicationUncertain => {
            "Résultat du remplacement incertain : vérifier la source et le résultat conservé"
        }
        JobErrorCode::TransferFailed => "Transfert interrompu",
        JobErrorCode::ProbeUnavailable => "Sonde requise absente sur le worker",
        JobErrorCode::LeaseExpired => "Serveur déconnecté en cours de traitement",
        JobErrorCode::MissingSourceFingerprint => "Empreinte de la source manquante",
        JobErrorCode::NoCapableWorker => "Aucun worker capable pour le relais",
    }
}

/// Libellé de la marque « relayée » d'une tâche : où elle attend, ou d'où elle vient.
pub fn relay_label(
    info: &forgearr_shared::relay::JobRelayInfo,
    status: forgearr_shared::jobs::JobStatus,
    workers: &std::collections::HashMap<uuid::Uuid, String>,
) -> String {
    let name = |id: &uuid::Uuid| {
        workers
            .get(id)
            .cloned()
            .unwrap_or_else(|| id.to_string().chars().take(8).collect())
    };
    if status != forgearr_shared::jobs::JobStatus::Queued {
        return format!("Relayée depuis {}", name(&info.from_worker_id));
    }
    match (&info.target_worker_id, &info.requirement) {
        (Some(target), _) => format!("Relayée vers {}", name(target)),
        (None, Some(requirement)) => {
            format!("Relayée · attend un worker {}", requirement.codec.label())
        }
        (None, None) => "Relayée".to_owned(),
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
pub const EVENT_NAMES: [JobEventType; 9] = [
    JobEventType::JobCreated,
    JobEventType::JobClaimed,
    JobEventType::JobHeartbeat,
    JobEventType::JobProgress,
    JobEventType::JobSucceeded,
    JobEventType::JobFailed,
    JobEventType::JobCanceled,
    JobEventType::JobRetried,
    JobEventType::JobRelayed,
];

/// Tâches lues par page : le maximum que le central sert.
pub const JOBS_PAGE_SIZE: usize = 200;

/// Première page suivie de pages plus anciennes ; une tâche déjà vue n'apparaît qu'une fois.
pub fn merge_pages(first: Vec<Job>, older: &[Job]) -> Vec<Job> {
    let known: std::collections::HashSet<_> = first.iter().map(|job| job.id).collect();
    let mut merged = first;
    merged.extend(older.iter().filter(|job| !known.contains(&job.id)).cloned());
    merged
}

/// Y a-t-il peut-être d'autres tâches après cette page ? Une page pleine le laisse penser.
pub fn may_have_more(page_len: usize) -> bool {
    page_len >= JOBS_PAGE_SIZE
}

/// Synthèse `(en_cours, en_attente, terminées, échouées)` des tâches chargées.
pub fn summarize_jobs(jobs: &[Job]) -> (usize, usize, usize, usize) {
    let mut running = 0_usize;
    let mut queued = 0_usize;
    let mut completed = 0_usize;
    let mut failed = 0_usize;
    for job in jobs {
        match job.status {
            JobStatus::Running => running += 1,
            JobStatus::Queued => queued += 1,
            JobStatus::Succeeded | JobStatus::Canceled => completed += 1,
            JobStatus::Failed => failed += 1,
        }
    }
    (running, queued, completed, failed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job(id: u128) -> Result<Job, serde_json::Error> {
        serde_json::from_value(serde_json::json!({
            "id": uuid::Uuid::from_u128(id), "source_name": "a.mkv", "source_sha256": null,
            "source_bytes": null, "output_sha256": null, "output_bytes": null, "warning": false,
            "target_worker_id": null, "worker_id": null, "status": "queued", "attempts": 0,
            "leased_until": null, "output_name": null, "error_code": null, "flow_id": null,
            "flow_version": null, "created_at": "2026-11-14T10:00:00Z", "updated_at": "2026-11-14T10:00:00Z"
        }))
    }

    #[test]
    fn older_pages_follow_the_first_one_without_duplicates() -> Result<(), serde_json::Error> {
        let merged = merge_pages(vec![job(1)?, job(2)?], &[job(2)?, job(3)?]);
        let ids: Vec<_> = merged.iter().map(|job| job.id.as_u128()).collect();
        assert_eq!(ids, [1, 2, 3]);
        Ok(())
    }

    #[test]
    fn only_a_full_page_suggests_more() {
        assert!(may_have_more(JOBS_PAGE_SIZE));
        assert!(!may_have_more(JOBS_PAGE_SIZE - 1));
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
    fn relay_label_tells_where_the_job_waits_or_came_from() {
        use forgearr_shared::capabilities::{CapabilityRequirement, VideoCodec};
        use forgearr_shared::relay::JobRelayInfo;
        let origin = uuid::Uuid::from_u128(1);
        let target = uuid::Uuid::from_u128(2);
        let workers = std::collections::HashMap::from([(origin, "sans-gpu".to_owned())]);
        let open = JobRelayInfo {
            job_id: uuid::Uuid::from_u128(9),
            from_worker_id: origin,
            relay_count: 1,
            target_worker_id: None,
            requirement: Some(CapabilityRequirement {
                codec: VideoCodec::Hevc,
                acceleration: None,
            }),
        };
        assert_eq!(
            relay_label(&open, JobStatus::Queued, &workers),
            "Relayée · attend un worker H.265"
        );
        assert_eq!(
            relay_label(&open, JobStatus::Running, &workers),
            "Relayée depuis sans-gpu"
        );
        let pinned = JobRelayInfo {
            target_worker_id: Some(target),
            requirement: None,
            ..open
        };
        assert_eq!(
            relay_label(&pinned, JobStatus::Queued, &workers),
            "Relayée vers 00000000"
        );
    }

    #[test]
    fn every_error_code_has_a_label() {
        for code in [
            JobErrorCode::OperationFailed,
            JobErrorCode::WebhookFailed,
            JobErrorCode::IntegrationFailed,
            JobErrorCode::SourceIntegrity,
            JobErrorCode::SourceChanged,
            JobErrorCode::SourcePublicationUncertain,
            JobErrorCode::TransferFailed,
            JobErrorCode::ProbeUnavailable,
            JobErrorCode::LeaseExpired,
            JobErrorCode::MissingSourceFingerprint,
            JobErrorCode::NoCapableWorker,
        ] {
            assert!(!error_label(code).is_empty());
        }
    }

    #[test]
    fn summarize_jobs_counts_running_queued_completed_and_failed() -> Result<(), serde_json::Error>
    {
        let queued = job(1)?;
        let running = Job {
            status: JobStatus::Running,
            ..job(2)?
        };
        let succeeded = Job {
            status: JobStatus::Succeeded,
            ..job(3)?
        };
        let failed = Job {
            status: JobStatus::Failed,
            ..job(4)?
        };
        assert_eq!(
            summarize_jobs(&[queued, running, succeeded, failed]),
            (1, 1, 1, 1)
        );
        assert_eq!(summarize_jobs(&[]), (0, 0, 0, 0));
        Ok(())
    }

    #[test]
    fn size_delta_and_digest_and_library_helpers_format_cleanly() {
        assert_eq!(
            format_size_delta(Some(1000), Some(600)).as_deref(),
            Some("-40 %")
        );
        assert_eq!(
            format_size_delta(Some(1000), Some(1150)).as_deref(),
            Some("+15 %")
        );
        assert_eq!(format_size_delta(Some(0), Some(500)), None);
        assert_eq!(format_size_delta(None, Some(500)), None);

        assert_eq!(
            short_digest(Some("0123456789abcdef0123456789")),
            "0123456789ab…"
        );
        assert_eq!(short_digest(Some("")), "—");
        assert_eq!(short_digest(None), "—");

        let lib_id = Uuid::from_u128(0xabcd_ef01);
        let mut libs = HashMap::new();
        assert_eq!(
            resolve_library_label(Some(lib_id), &libs).as_deref(),
            Some("00000000")
        );
        libs.insert(lib_id, "Séries UHD".into());
        assert_eq!(
            resolve_library_label(Some(lib_id), &libs).as_deref(),
            Some("Séries UHD")
        );
        assert_eq!(resolve_library_label(None, &libs), None);
        assert_eq!(step_status_tone(JobStepStatus::Skipped), "warning");
    }
}
