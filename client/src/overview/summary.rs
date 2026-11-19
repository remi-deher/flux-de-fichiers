//! Chiffres et alertes de la vue d'ensemble, calculés sans DOM.

use chrono::{DateTime, Utc};
use forgearr_shared::jobs::Job;
use forgearr_shared::libraries::Library;
use forgearr_shared::workers::Worker;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Summary {
    pub retryable: usize,
    pub workers_online: usize,
    pub workers_total: usize,
    pub libraries_ready: usize,
    pub libraries_total: usize,
}

impl Summary {
    pub fn new(
        jobs: &[Job],
        workers: &[Worker],
        libraries: &[Library],
        now: DateTime<Utc>,
    ) -> Self {
        Self {
            retryable: jobs.iter().filter(|job| job.is_retryable()).count(),
            workers_online: workers
                .iter()
                .filter(|worker| worker.is_online(now))
                .count(),
            workers_total: workers.len(),
            libraries_ready: libraries
                .iter()
                .filter(|library| library.flow_id.is_some() && !library.workers.is_empty())
                .count(),
            libraries_total: libraries.len(),
        }
    }
}

/// Points d'attention, du plus urgent au plus informatif.
pub fn alerts(
    summary: &Summary,
    workers: &[Worker],
    now: DateTime<Utc>,
    plain_http: bool,
) -> Vec<String> {
    let mut list = Vec::new();
    if plain_http {
        list.push("Accès en HTTP simple : les mots de passe circulent en clair. Placez le central derrière un proxy TLS.".into());
    }
    let offline: Vec<&str> = workers
        .iter()
        .filter(|worker| !worker.is_integrated() && !worker.is_online(now))
        .map(|worker| worker.name.as_str())
        .collect();
    if !offline.is_empty() {
        list.push(format!("Worker hors ligne : {}.", offline.join(", ")));
    }
    if summary.retryable > 0 {
        list.push(format!(
            "{} tâche(s) perdue(s) avec leur worker, relançable(s) depuis Tâches.",
            summary.retryable
        ));
    }
    if summary.libraries_ready < summary.libraries_total {
        list.push(format!(
            "{} bibliothèque(s) sans flux ou sans worker : elles ne sont pas traitées.",
            summary.libraries_total - summary.libraries_ready
        ));
    }
    list
}

/// Pourcentage entier borné `[0, 100]` d'une sous-partie sur un total.
pub fn ratio_percent(part: usize, total: usize) -> u8 {
    if total == 0 {
        return 0;
    }
    ((part.saturating_mul(100)) / total).min(100) as u8
}

#[cfg(test)]
mod tests {
    use super::*;
    use forgearr_shared::workers::INTEGRATED_WORKER_NAME;
    use uuid::Uuid;

    fn worker(name: &str, seconds_ago: i64, now: DateTime<Utc>) -> Worker {
        Worker {
            id: Uuid::nil(),
            name: name.into(),
            last_seen: now - chrono::Duration::seconds(seconds_ago),
            version: None,
            remote_addr: None,
            telemetry: None,
            max_concurrency: 1,
            schedule: None,
        }
    }

    #[test]
    fn offline_remote_workers_and_unready_libraries_raise_alerts() {
        let now = DateTime::<Utc>::UNIX_EPOCH + chrono::Duration::days(1);
        let workers = [
            worker(INTEGRATED_WORKER_NAME, 3600, now),
            worker("nas", 3600, now),
            worker("pc", 1, now),
        ];
        let library = Library {
            id: Uuid::nil(),
            name: "Films".into(),
            flow_id: None,
            workers: Vec::new(),
            transfer_workers: Vec::new(),
            scan_interval_minutes: 0,
            last_scanned_at: None,
            include_extensions: Vec::new(),
            exclude_patterns: Vec::new(),
            min_file_bytes: 0,
            output_policy: forgearr_shared::libraries::LibraryOutputPolicy::OutputDir,
            created_at: now,
            updated_at: now,
        };
        let summary = Summary::new(&[], &workers, &[library], now);
        assert_eq!(summary.workers_online, 1);
        assert_eq!(summary.libraries_ready, 0);
        let list = alerts(&summary, &workers, now, true);
        assert_eq!(list.len(), 3);
        assert!(list[1].contains("nas") && !list[1].contains(INTEGRATED_WORKER_NAME));
        assert!(alerts(&Summary::default(), &[], now, false).is_empty());
    }

    #[test]
    fn ratio_percent_is_zero_safe_and_clamped_to_hundred() {
        assert_eq!(ratio_percent(0, 0), 0);
        assert_eq!(ratio_percent(1, 4), 25);
        assert_eq!(ratio_percent(5, 4), 100);
    }
}
