//! Relais d'une tâche vers la file d'un autre worker (ADR 0018).

use crate::{
    domain::{JobErrorCode, JobStatus, LeaseRequest, Worker},
    error::{AppError, AppResult},
};
use forgearr_shared::{
    capabilities::CapabilityRequirement,
    relay::{JobRelayInfo, MAX_RELAYS_PER_JOB, RelayRequest, RelayTarget},
    workers::WORKER_ONLINE_SECONDS,
};
use sqlx::SqlitePool;
use std::collections::HashSet;
use uuid::Uuid;

/// Issue d'une demande de relais côté central.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RelayOutcome {
    /// La tâche attend : dans la file du worker précis, ou offerte à tous les workers capables
    /// (`target_worker_id` absent).
    Relayed { target_worker_id: Option<Uuid> },
    /// Aucune cible éligible (ou plafond de relais atteint) : la tâche a échoué.
    Failed,
}

/// Où attend la tâche relayée.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Placement {
    /// Dans la file d'un worker désigné : elle l'attend, même hors ligne.
    Pinned(Uuid),
    /// Offerte à tout worker satisfaisant l'exigence : le premier qui la réclame la prend.
    Open(CapabilityRequirement),
}

/// Décide où placer la tâche. `None` si aucun worker éligible (différent de l'origine, avec accès
/// à la source, et capable dans le cas d'une exigence).
fn decide_placement(
    workers: &[Worker],
    from: Uuid,
    target: &RelayTarget,
    with_source_access: &HashSet<Uuid>,
) -> Option<Placement> {
    let eligible = |worker: &&Worker| worker.id != from && with_source_access.contains(&worker.id);
    match target {
        RelayTarget::Worker { worker_id } => workers
            .iter()
            .filter(eligible)
            .any(|worker| worker.id == *worker_id)
            .then_some(Placement::Pinned(*worker_id)),
        RelayTarget::Capable { requirement } => workers
            .iter()
            .filter(eligible)
            .any(|worker| {
                worker
                    .telemetry
                    .as_ref()
                    .is_some_and(|telemetry| telemetry.capabilities.satisfies(requirement))
            })
            .then_some(Placement::Open(*requirement)),
    }
}

/// Le worker peut-il lire la source du job (central ou montage partagé vérifié) ?
async fn worker_can_read_source(pool: &SqlitePool, job: Uuid, worker: Uuid) -> AppResult<bool> {
    let accessible: Option<bool> = sqlx::query_scalar(
        "SELECT (source_key IS NULL AND (source_worker_id IS NULL OR source_worker_id=?2)) \
             OR EXISTS (SELECT 1 FROM verified_job_source_mounts m WHERE m.job_id=jobs.id \
                        AND (m.worker_id=?2 OR m.worker_id=zeroblob(16))) \
             OR EXISTS (SELECT 1 FROM workers o WHERE o.id=jobs.source_worker_id \
                        AND o.last_seen > datetime('now', '-' || ?3 || ' seconds')) \
         FROM jobs WHERE id=?1",
    )
    .bind(job)
    .bind(worker)
    .bind(WORKER_ONLINE_SECONDS)
    .fetch_optional(pool)
    .await?;
    Ok(accessible.unwrap_or(false))
}

/// Relaie une tâche : décide du placement, libère le bail et remet la tâche en file.
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
    let placement = if relay_count >= MAX_RELAYS_PER_JOB {
        None
    } else {
        let workers = crate::repository::list_workers(pool).await?;
        let mut with_access = HashSet::new();
        for worker in workers.iter().filter(|worker| worker.id != lease.worker_id) {
            if worker_can_read_source(pool, id, worker.id).await? {
                with_access.insert(worker.id);
            }
        }
        decide_placement(&workers, lease.worker_id, &request.target, &with_access)
    };

    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let outcome = match placement {
        Some(placement) => {
            let (pinned, requirement_json) = match placement {
                Placement::Pinned(worker) => (Some(worker), None),
                Placement::Open(requirement) => (
                    None,
                    Some(serde_json::to_string(&requirement).map_err(|_| AppError::Internal)?),
                ),
            };
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
            .bind(pinned)
            .bind(JobStatus::Running)
            .execute(&mut *tx)
            .await?
            .rows_affected();
            if updated != 1 {
                return Err(AppError::Conflict);
            }
            sqlx::query(
                "INSERT INTO job_relays(job_id, from_worker_id, relay_count, resume_node_id, requirement_json) \
                 VALUES(?1, ?2, 1, ?3, ?4) \
                 ON CONFLICT(job_id) DO UPDATE SET from_worker_id=excluded.from_worker_id, \
                     relay_count=relay_count+1, resume_node_id=excluded.resume_node_id, \
                     requirement_json=excluded.requirement_json, relayed_at=datetime('now')",
            )
            .bind(id)
            .bind(lease.worker_id)
            .bind(&request.resume_node_id)
            .bind(requirement_json)
            .execute(&mut *tx)
            .await?;
            RelayOutcome::Relayed {
                target_worker_id: pinned,
            }
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

/// Marques « relayée » des tâches données (celles qui n'ont pas été relayées sont absentes).
pub async fn list_relays(pool: &SqlitePool, ids: &[Uuid]) -> AppResult<Vec<JobRelayInfo>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let placeholders = vec!["?"; ids.len()].join(",");
    let sql = format!(
        "SELECT r.job_id, r.from_worker_id, r.relay_count, j.target_worker_id, r.requirement_json \
         FROM job_relays r JOIN jobs j ON j.id = r.job_id WHERE r.job_id IN ({placeholders})"
    );
    let mut query = sqlx::query_as::<_, (Uuid, Uuid, i64, Option<Uuid>, Option<String>)>(&sql);
    for id in ids {
        query = query.bind(id);
    }
    let rows = query.fetch_all(pool).await?;
    Ok(rows
        .into_iter()
        .map(
            |(job_id, from_worker_id, relay_count, target_worker_id, requirement)| JobRelayInfo {
                job_id,
                from_worker_id,
                relay_count,
                target_worker_id,
                requirement: requirement.and_then(|raw| serde_json::from_str(&raw).ok()),
            },
        )
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};
    use forgearr_shared::{
        capabilities::{EncodeAcceleration, HardwareEncoder, VideoCodec, WorkerCapabilities},
        workers::WorkerTelemetry,
    };

    fn worker(name: &str, hevc: bool) -> Worker {
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
            last_seen: Utc::now() - Duration::seconds(0),
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

    fn hevc_requirement() -> CapabilityRequirement {
        CapabilityRequirement {
            codec: VideoCodec::Hevc,
            acceleration: None,
        }
    }

    fn all_access(workers: &[Worker]) -> HashSet<Uuid> {
        workers.iter().map(|worker| worker.id).collect()
    }

    #[test]
    fn a_capable_target_is_offered_to_every_capable_worker() {
        let origin = worker("origine", false);
        let capable = worker("capable", true);
        let workers = vec![origin.clone(), capable];
        let target = RelayTarget::Capable {
            requirement: hevc_requirement(),
        };
        assert_eq!(
            decide_placement(&workers, origin.id, &target, &all_access(&workers)),
            Some(Placement::Open(hevc_requirement()))
        );
    }

    #[test]
    fn no_capable_worker_means_no_placement() {
        let origin = worker("origine", true);
        let blind = worker("sans-gpu", false);
        let workers = vec![origin.clone(), blind];
        let target = RelayTarget::Capable {
            requirement: hevc_requirement(),
        };
        assert_eq!(
            decide_placement(&workers, origin.id, &target, &all_access(&workers)),
            None,
            "l'origine est exclue même si elle est capable"
        );
    }

    #[test]
    fn a_capable_worker_without_source_access_does_not_count() {
        let origin = worker("origine", false);
        let capable = worker("capable", true);
        let workers = vec![origin.clone(), capable];
        let target = RelayTarget::Capable {
            requirement: hevc_requirement(),
        };
        let access = HashSet::from([origin.id]);
        assert_eq!(
            decide_placement(&workers, origin.id, &target, &access),
            None
        );
    }

    #[test]
    fn a_precise_worker_is_pinned_even_when_offline() {
        let origin = worker("origine", false);
        let mut precise = worker("precis", false);
        precise.last_seen = Utc::now() - Duration::seconds(100_000);
        let workers = vec![origin.clone(), precise.clone()];
        let target = RelayTarget::Worker {
            worker_id: precise.id,
        };
        assert_eq!(
            decide_placement(&workers, origin.id, &target, &all_access(&workers)),
            Some(Placement::Pinned(precise.id))
        );
        let to_self = RelayTarget::Worker {
            worker_id: origin.id,
        };
        assert_eq!(
            decide_placement(&workers, origin.id, &to_self, &all_access(&workers)),
            None
        );
    }
}
