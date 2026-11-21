//! Relais d'une tâche vers la file d'un autre worker (ADR 0018).

use crate::{
    domain::{JobErrorCode, JobStatus, LeaseRequest, Worker},
    error::{AppError, AppResult},
};
use forgearr_shared::{
    capabilities::CapabilityRequirement,
    flows::FlowVariables,
    relay::{
        HandoffArtifact, HandoffInput, JobRelayInfo, MAX_RELAYED_OUTPUTS_BYTES, MAX_RELAYS_PER_JOB,
        RelayRequest, RelayTarget, RelayedNodeOutput,
    },
    transfer::ArtifactKind,
    workers::WORKER_ONLINE_SECONDS,
};
use serde_json::{Map, Value};
use sqlx::SqlitePool;
use std::collections::{HashMap, HashSet};
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
pub(super) enum Placement {
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

/// Le worker peut-il traiter la tâche (règle de la bibliothèque, ADR 0021) ? Avec `needs_source`, sa
/// source doit aussi être lisible (central, montage propre, ou détenteur en ligne avec adresse de
/// transfert) ; un relais par artefact lit le fichier de travail chez son détenteur et s'en passe.
async fn worker_can_process(
    pool: &SqlitePool,
    job: Uuid,
    worker: Uuid,
    needs_source: bool,
) -> AppResult<bool> {
    let access = super::library_access::job_access_sql("?2");
    let sql = if needs_source {
        let readable = super::library_access::source_readable_sql("?2", "?3");
        format!("SELECT {access} AND {readable} FROM jobs WHERE id=?1")
    } else {
        format!("SELECT {access} FROM jobs WHERE id=?1")
    };
    let mut query = sqlx::query_scalar::<_, bool>(&sql).bind(job).bind(worker);
    if needs_source {
        query = query.bind(WORKER_ONLINE_SECONDS);
    }
    let accessible: Option<bool> = query.fetch_optional(pool).await?;
    Ok(accessible.unwrap_or(false))
}

pub(super) async fn choose_placement(
    pool: &SqlitePool,
    job: Uuid,
    from: Uuid,
    target: &RelayTarget,
    needs_source: bool,
) -> AppResult<Option<Placement>> {
    let workers = crate::repository::list_workers(pool).await?;
    let mut with_access = HashSet::new();
    for worker in workers.iter().filter(|worker| worker.id != from) {
        if worker_can_process(pool, job, worker.id, needs_source).await? {
            with_access.insert(worker.id);
        }
    }
    Ok(decide_placement(&workers, from, target, &with_access))
}

/// Relaie une tâche : décide du placement, libère le bail et remet la tâche en file.
///
/// Sans cible éligible ou au-delà de `MAX_RELAYS_PER_JOB`, la tâche échoue (`no_capable_worker`).
pub async fn relay_job(
    pool: &SqlitePool,
    id: Uuid,
    lease: &LeaseRequest,
    request: &RelayRequest,
    warning: bool,
) -> AppResult<RelayOutcome> {
    let relay_count: i64 = sqlx::query_scalar(
        "SELECT COALESCE((SELECT relay_count FROM job_relays WHERE job_id=?1), 0)",
    )
    .bind(id)
    .fetch_one(pool)
    .await?;
    // Un fichier de travail déclaré par ce bail fait un relais par artefact (ADR 0022).
    let kind = ArtifactKind::Working;
    let epoch = lease.lease_epoch;
    let artifact_relay = super::artifact_retained(pool, id, kind, lease.worker_id, epoch).await?;
    if artifact_relay && !super::source_tickets::can_serve(pool, lease.worker_id).await? {
        // Sans adresse de transfert, personne ne pourrait tirer le fichier de travail.
        return Err(AppError::Invalid);
    }
    let node_outputs_json = serialize_node_outputs(&request.node_outputs)?;
    let relay_variables = variables_json(&request.variables)?;
    let placement = if relay_count >= MAX_RELAYS_PER_JOB {
        None
    } else {
        choose_placement(pool, id, lease.worker_id, &request.target, !artifact_relay).await?
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
                     attempts=MAX(attempts-1, 0), updated_at=datetime('now'), \
                     warning=CASE WHEN warning=1 OR ?8=1 THEN 1 ELSE 0 END \
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
            .bind(warning)
            .execute(&mut *tx)
            .await?
            .rows_affected();
            if updated != 1 {
                return Err(AppError::Conflict);
            }
            if artifact_relay {
                snapshot_steps(&mut tx, id).await?;
            }
            sqlx::query(
                "INSERT INTO job_relays(job_id, from_worker_id, relay_count, resume_node_id, \
                     requirement_json, artifact_relay, node_outputs_json, variables_json) \
                 VALUES(?1, ?2, 1, ?3, ?4, ?5, ?6, ?7) \
                 ON CONFLICT(job_id) DO UPDATE SET from_worker_id=excluded.from_worker_id, \
                     relay_count=relay_count+1, resume_node_id=excluded.resume_node_id, \
                     requirement_json=excluded.requirement_json, \
                     artifact_relay=excluded.artifact_relay, \
                     node_outputs_json=excluded.node_outputs_json, \
                     variables_json=excluded.variables_json, relayed_at=datetime('now')",
            )
            .bind(id)
            .bind(lease.worker_id)
            .bind(&request.resume_node_id)
            .bind(requirement_json)
            .bind(artifact_relay)
            .bind(node_outputs_json)
            .bind(relay_variables)
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
            if artifact_relay {
                // La tâche échoue : le fichier de travail n'a plus de repreneur.
                sqlx::query("DELETE FROM job_artifacts WHERE job_id=?1 AND kind=?2")
                    .bind(id)
                    .bind(ArtifactKind::Working.as_str())
                    .execute(&mut *tx)
                    .await?;
            }
            RelayOutcome::Failed
        }
    };
    tx.commit().await?;
    Ok(outcome)
}

/// Reprise d'une tâche relayée, lue au moment du bail.
#[derive(Debug, Default, PartialEq)]
pub(super) struct ResumeState {
    pub node_id: Option<String>,
    pub outputs: HashMap<String, RelayedNodeOutput>,
    pub variables: Map<String, Value>,
}

/// Valide puis sérialise les variables personnalisées d'un relais (`NULL` si aucune).
pub(super) fn variables_json(variables: &Map<String, Value>) -> AppResult<Option<String>> {
    if variables.is_empty() {
        return Ok(None);
    }
    FlowVariables::default()
        .merge_custom(variables)
        .map_err(|_| AppError::Invalid)?;
    serde_json::to_string(variables)
        .map(Some)
        .map_err(|_| AppError::Internal)
}

/// Relit des variables de relais stockées ; une valeur illisible est ignorée.
pub(super) fn parse_variables_json(raw: Option<&str>) -> Map<String, Value> {
    raw.and_then(|text| serde_json::from_str(text).ok())
        .unwrap_or_default()
}

/// Sorties des nœuds à stocker avec le relais ; `None` s'il n'y en a pas, refus si elles dépassent la borne.
fn serialize_node_outputs(
    outputs: &HashMap<String, RelayedNodeOutput>,
) -> AppResult<Option<String>> {
    if outputs.is_empty() {
        return Ok(None);
    }
    let json = serde_json::to_string(outputs).map_err(|_| AppError::Invalid)?;
    if json.len() > MAX_RELAYED_OUTPUTS_BYTES {
        return Err(AppError::Invalid);
    }
    Ok(Some(json))
}

/// Point de reprise d'une tâche relayée, lu au moment du bail : nœud où reprendre, sorties des
/// nœuds déjà exécutés et variables personnalisées (`vars.*`). Un relais par artefact ne reprend
/// en cours de flux que tant que l'artefact est déclaré : sans lui (reprise après perte du repreneur), la tâche repart de la source.
pub(super) async fn resume_state(pool: &SqlitePool, job: Uuid) -> AppResult<ResumeState> {
    let row: Option<(String, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT resume_node_id, node_outputs_json, variables_json FROM job_relays r WHERE r.job_id=?1 \
         AND (r.artifact_relay=0 \
         OR EXISTS(SELECT 1 FROM job_artifacts a WHERE a.job_id=r.job_id AND a.kind=?2))",
    )
    .bind(job)
    .bind(ArtifactKind::Working.as_str())
    .fetch_optional(pool)
    .await?;
    Ok(row
        .map(|(node, outputs, variables)| ResumeState {
            node_id: Some(node),
            outputs: outputs
                .and_then(|json| serde_json::from_str(&json).ok())
                .unwrap_or_default(),
            variables: parse_variables_json(variables.as_deref()),
        })
        .unwrap_or_default())
}

/// Fichier de travail que ce bail doit tirer chez son détenteur (relais par artefact, ADR 0022).
pub(super) async fn artifact_input(
    pool: &SqlitePool,
    job: Uuid,
    taker: Uuid,
) -> AppResult<Option<HandoffInput>> {
    let relayed: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM job_relays WHERE job_id=?1 AND artifact_relay=1)",
    )
    .bind(job)
    .fetch_one(pool)
    .await?;
    if !relayed {
        return Ok(None);
    }
    let Some(artifact) = super::get_artifact(pool, job, ArtifactKind::Working).await? else {
        return Ok(None);
    };
    if artifact.holder_worker_id == taker {
        return Ok(None);
    }
    Ok(Some(HandoffInput {
        source_lease_epoch: artifact.lease_epoch,
        artifact: HandoffArtifact {
            name: artifact.declaration.name,
            sha256: artifact.declaration.sha256,
            bytes: artifact.declaration.bytes,
        },
    }))
}

/// Conserve le journal des étapes déjà exécutées : le repreneur le complète sans le perdre.
pub(super) async fn snapshot_steps(tx: &mut sqlx::SqliteConnection, job: Uuid) -> AppResult<()> {
    sqlx::query("DELETE FROM job_handoff_steps WHERE job_id=?1")
        .bind(job)
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "INSERT INTO job_handoff_steps(job_id, step_index, node_id, block_kind, operation_id, \
             status, message, output_excerpt, created_at, variables_json) \
         SELECT job_id, step_index, node_id, block_kind, operation_id, status, message, \
             output_excerpt, created_at, variables_json FROM job_steps WHERE job_id=?1",
    )
    .bind(job)
    .execute(&mut *tx)
    .await?;
    Ok(())
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
    use serde_json::json;

    fn worker(name: &str, hevc: bool) -> Worker {
        let capabilities = WorkerCapabilities {
            gpu_models: Vec::new(),
            software_encoders: Vec::new(),
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
            max_cpu_jobs: None,
            max_gpu_jobs: None,
            schedule: None,
            transfer_enabled: false,
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

    #[test]
    fn relay_variables_round_trip_and_are_validated() -> AppResult<()> {
        assert_eq!(variables_json(&Map::new())?, None);
        let mut variables = Map::new();
        variables.insert("codec".into(), json!("hevc"));
        let stored = variables_json(&variables)?;
        assert_eq!(parse_variables_json(stored.as_deref()), variables);
        assert!(parse_variables_json(Some("illisible")).is_empty());
        variables.insert("nom invalide".into(), json!(1));
        assert!(matches!(variables_json(&variables), Err(AppError::Invalid)));
        Ok(())
    }
}
