//! Relais d'une tâche vers la file d'un autre worker (ADR 0018).

use crate::{
    domain::{JobErrorCode, JobStatus, LeaseRequest, Worker},
    error::{AppError, AppResult},
};
use chrono::{DateTime, Utc};
use forgearr_shared::relay::{MAX_RELAYS_PER_JOB, RelayRequest, RelayTarget};
use sqlx::SqlitePool;
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

/// Issue d'une demande de relais côté central.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RelayOutcome {
    /// La tâche attend dans la file de ce worker, marquée « relayée ».
    Relayed { target_worker_id: Uuid },
    /// Aucune cible éligible (ou plafond de relais atteint) : la tâche a échoué.
    Failed,
}

/// Choisit le worker cible : éligible, différent de l'origine, avec accès à la source ;
/// de préférence en ligne, puis le moins chargé, puis par nom.
pub fn choose_target(
    workers: &[Worker],
    from: Uuid,
    target: &RelayTarget,
    loads: &HashMap<Uuid, i64>,
    with_source_access: &HashSet<Uuid>,
    now: DateTime<Utc>,
) -> Option<Uuid> {
    workers
        .iter()
        .filter(|worker| worker.id != from && with_source_access.contains(&worker.id))
        .filter(|worker| match target {
            RelayTarget::Worker { worker_id } => worker.id == *worker_id,
            RelayTarget::Capable { requirement } => worker
                .telemetry
                .as_ref()
                .is_some_and(|telemetry| telemetry.capabilities.satisfies(requirement)),
        })
        .min_by_key(|worker| {
            (
                !worker.is_online(now),
                loads.get(&worker.id).copied().unwrap_or(0),
                worker.name.clone(),
            )
        })
        .map(|worker| worker.id)
}

/// Le worker peut-il lire la source du job (central ou montage partagé vérifié) ?
async fn worker_can_read_source(pool: &SqlitePool, job: Uuid, worker: Uuid) -> AppResult<bool> {
    let accessible: Option<bool> = sqlx::query_scalar(
        "SELECT (source_key IS NULL AND (source_worker_id IS NULL OR source_worker_id=?2)) \
             OR EXISTS (SELECT 1 FROM verified_job_source_mounts m WHERE m.job_id=jobs.id \
                        AND (m.worker_id=?2 OR m.worker_id=zeroblob(16))) \
         FROM jobs WHERE id=?1",
    )
    .bind(job)
    .bind(worker)
    .fetch_optional(pool)
    .await?;
    Ok(accessible.unwrap_or(false))
}

/// Charge de chaque worker : tâches en cours chez lui plus tâches en file qui le ciblent.
async fn worker_loads(pool: &SqlitePool) -> AppResult<HashMap<Uuid, i64>> {
    let rows: Vec<(Uuid, i64)> = sqlx::query_as(
        "SELECT w, COUNT(*) FROM (\
             SELECT CASE WHEN status='running' THEN worker_id ELSE target_worker_id END AS w \
             FROM jobs WHERE status IN ('queued', 'running')) \
         WHERE w IS NOT NULL GROUP BY w",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().collect())
}

/// Relaie une tâche : choisit la cible, libère le bail et remet la tâche dans la file de la cible.
///
/// Sans cible éligible ou au-delà de `MAX_RELAYS_PER_JOB`, la tâche échoue (`no_capable_worker`).
pub async fn relay_job(
    pool: &SqlitePool,
    id: Uuid,
    lease: &LeaseRequest,
    request: &RelayRequest,
) -> AppResult<RelayOutcome> {
    let relay_count: i64 = sqlx::query_scalar(
        "SELECT COALESCE((SELECT relay_count FROM job_relays WHERE job_id=?1), 0)",
    )
    .bind(id)
    .fetch_one(pool)
    .await?;
    let target = if relay_count >= MAX_RELAYS_PER_JOB {
        None
    } else {
        let workers = crate::repository::list_workers(pool).await?;
        let mut with_access = HashSet::new();
        for worker in workers.iter().filter(|worker| worker.id != lease.worker_id) {
            if worker_can_read_source(pool, id, worker.id).await? {
                with_access.insert(worker.id);
            }
        }
        let loads = worker_loads(pool).await?;
        choose_target(
            &workers,
            lease.worker_id,
            &request.target,
            &loads,
            &with_access,
            Utc::now(),
        )
    };

    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let outcome = match target {
        Some(target_worker_id) => {
            let updated = sqlx::query(
                "UPDATE jobs SET status=?5, worker_id=NULL, lease_token=NULL, \
                     lease_epoch=lease_epoch+1, leased_until=NULL, target_worker_id=?6, \
                     attempts=MAX(attempts-1, 0), updated_at=datetime('now') \
                 WHERE id=?1 AND worker_id=?2 AND lease_token=?3 AND lease_epoch=?4 \
                   AND status=?7 AND leased_until > datetime('now')",
            )
            .bind(id)
            .bind(lease.worker_id)
            .bind(lease.lease_token)
            .bind(lease.lease_epoch)
            .bind(JobStatus::Queued)
            .bind(target_worker_id)
            .bind(JobStatus::Running)
            .execute(&mut *tx)
            .await?
            .rows_affected();
            if updated != 1 {
                return Err(AppError::Conflict);
            }
            sqlx::query(
                "INSERT INTO job_relays(job_id, from_worker_id, relay_count, resume_node_id) \
                 VALUES(?1, ?2, 1, ?3) \
                 ON CONFLICT(job_id) DO UPDATE SET from_worker_id=excluded.from_worker_id, \
                     relay_count=relay_count+1, resume_node_id=excluded.resume_node_id, \
                     relayed_at=datetime('now')",
            )
            .bind(id)
            .bind(lease.worker_id)
            .bind(&request.resume_node_id)
            .execute(&mut *tx)
            .await?;
            RelayOutcome::Relayed { target_worker_id }
        }
        None => {
            let updated = sqlx::query(
                "UPDATE jobs SET status=?5, error_code=?6, lease_token=NULL, \
                     lease_epoch=lease_epoch+1, leased_until=NULL, updated_at=datetime('now') \
                 WHERE id=?1 AND worker_id=?2 AND lease_token=?3 AND lease_epoch=?4 \
                   AND status=?7 AND leased_until > datetime('now')",
            )
            .bind(id)
            .bind(lease.worker_id)
            .bind(lease.lease_token)
            .bind(lease.lease_epoch)
            .bind(JobStatus::Failed)
            .bind(JobErrorCode::NoCapableWorker)
            .bind(JobStatus::Running)
            .execute(&mut *tx)
            .await?
            .rows_affected();
            if updated != 1 {
                return Err(AppError::Conflict);
            }
            RelayOutcome::Failed
        }
    };
    tx.commit().await?;
    Ok(outcome)
}

/// Nœud de reprise d'une tâche relayée, lu au moment du bail.
pub(super) async fn resume_node(pool: &SqlitePool, job: Uuid) -> AppResult<Option<String>> {
    Ok(
        sqlx::query_scalar("SELECT resume_node_id FROM job_relays WHERE job_id=?1")
            .bind(job)
            .fetch_optional(pool)
            .await?,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use forgearr_shared::{
        capabilities::{
            CapabilityRequirement, EncodeAcceleration, HardwareEncoder, VideoCodec,
            WorkerCapabilities,
        },
        workers::WorkerTelemetry,
    };

    fn worker(name: &str, seen_ago: i64, hevc: bool) -> Worker {
        let capabilities = WorkerCapabilities {
            gpu_models: Vec::new(),
            hardware_encoders: if hevc {
                vec![HardwareEncoder {
                    codec: VideoCodec::Hevc,
                    acceleration: EncodeAcceleration::Nvenc,
                }]
            } else {
                Vec::new()
            },
        };
        Worker {
            id: Uuid::new_v4(),
            name: name.to_owned(),
            last_seen: Utc::now() - Duration::seconds(seen_ago),
            version: None,
            remote_addr: None,
            telemetry: Some(WorkerTelemetry {
                timestamp: Utc::now(),
                logical_cores: 4,
                cpu_usage_percent: 0.0,
                ram_total_bytes: 0,
                ram_used_bytes: 0,
                storage_total_bytes: 0,
                storage_available_bytes: 0,
                gpu_devices: Vec::new(),
                capabilities,
            }),
            max_concurrency: 1,
            schedule: None,
        }
    }

    fn hevc_target() -> RelayTarget {
        RelayTarget::Capable {
            requirement: CapabilityRequirement {
                codec: VideoCodec::Hevc,
                acceleration: None,
            },
        }
    }

    fn all_access(workers: &[Worker]) -> HashSet<Uuid> {
        workers.iter().map(|worker| worker.id).collect()
    }

    #[test]
    fn picks_the_least_loaded_online_capable_worker() {
        let origin = worker("a-origine", 0, false);
        let busy = worker("b-occupe", 0, true);
        let idle = worker("c-libre", 0, true);
        let incapable = worker("d-sans-gpu", 0, false);
        let workers = vec![origin.clone(), busy.clone(), idle.clone(), incapable];
        let loads = HashMap::from([(busy.id, 3), (idle.id, 1)]);
        let chosen = choose_target(
            &workers,
            origin.id,
            &hevc_target(),
            &loads,
            &all_access(&workers),
            Utc::now(),
        );
        assert_eq!(chosen, Some(idle.id));
    }

    #[test]
    fn prefers_an_online_worker_over_an_offline_one() {
        let origin = worker("a", 0, false);
        let offline = worker("b", 100_000, true);
        let online = worker("c", 0, true);
        let workers = vec![origin.clone(), offline, online.clone()];
        let chosen = choose_target(
            &workers,
            origin.id,
            &hevc_target(),
            &HashMap::new(),
            &all_access(&workers),
            Utc::now(),
        );
        assert_eq!(chosen, Some(online.id));
    }

    #[test]
    fn excludes_the_origin_and_workers_without_source_access() {
        let origin = worker("a", 0, true);
        let blind = worker("b", 0, true);
        let workers = vec![origin.clone(), blind.clone()];
        let access = HashSet::from([origin.id]);
        assert_eq!(
            choose_target(
                &workers,
                origin.id,
                &hevc_target(),
                &HashMap::new(),
                &access,
                Utc::now()
            ),
            None
        );
    }

    #[test]
    fn a_precise_worker_is_chosen_only_if_it_has_source_access() {
        let origin = worker("a", 0, false);
        let precise = worker("b", 0, false);
        let workers = vec![origin.clone(), precise.clone()];
        let target = RelayTarget::Worker {
            worker_id: precise.id,
        };
        let with = all_access(&workers);
        assert_eq!(
            choose_target(
                &workers,
                origin.id,
                &target,
                &HashMap::new(),
                &with,
                Utc::now()
            ),
            Some(precise.id)
        );
        let target_self = RelayTarget::Worker {
            worker_id: origin.id,
        };
        assert_eq!(
            choose_target(
                &workers,
                origin.id,
                &target_self,
                &HashMap::new(),
                &with,
                Utc::now()
            ),
            None
        );
    }
}
