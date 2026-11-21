//! État durable de la remise d'un fichier de travail entre workers.

use crate::{
    domain::{JobErrorCode, JobStatus, LeaseRequest},
    error::{AppError, AppResult},
};
use forgearr_shared::{
    capabilities::CapabilityRequirement,
    relay::{
        HANDOFF_TTL_SECONDS, HandoffAcknowledgement, HandoffArtifact, HandoffOffer, HandoffState,
        MAX_RELAYS_PER_JOB, RelayRequest,
    },
    workers::WORKER_ONLINE_SECONDS,
};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use std::path::{Component, Path};
use uuid::Uuid;

const HANDOFF_CANDIDATE_LIMIT: i64 = 200;

fn receipt_hash(token: &str) -> String {
    format!("{:x}", Sha256::digest(token.as_bytes()))
}

fn new_receipt_token() -> String {
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}

#[derive(sqlx::FromRow)]
struct PendingHandoff {
    job_id: Uuid,
    from_worker_id: Uuid,
    source_lease_epoch: i64,
    target_worker_id: Option<Uuid>,
    requirement_json: Option<String>,
    artifact_name: String,
    artifact_sha256: String,
    artifact_bytes: i64,
}

#[derive(sqlx::FromRow)]
struct ClaimedHandoff {
    from_worker_id: Uuid,
    source_lease_epoch: i64,
    resume_node_id: String,
    requirement_json: Option<String>,
    artifact_sha256: String,
    artifact_bytes: i64,
    warning: bool,
    variables_json: Option<String>,
}

fn valid_artifact(artifact: &HandoffArtifact, max_bytes: u64) -> bool {
    let mut components = Path::new(&artifact.name).components();
    matches!(components.next(), Some(Component::Normal(_)))
        && components.next().is_none()
        && artifact.name.len() <= 255
        && !artifact.name.chars().any(char::is_control)
        && artifact.sha256.len() == 64
        && artifact.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
        && u64::try_from(artifact.bytes).is_ok_and(|bytes| bytes > 0 && bytes <= max_bytes)
}

/// Crée la remise sans libérer le bail ni modifier la file du job.
/// Rend `false` quand aucune cible n'est éligible ou que la limite de relais est atteinte.
pub async fn begin_handoff(
    pool: &SqlitePool,
    job: Uuid,
    lease: &LeaseRequest,
    request: &RelayRequest,
    artifact: &HandoffArtifact,
    max_bytes: u64,
    warning: bool,
) -> AppResult<bool> {
    if !valid_artifact(artifact, max_bytes)
        || request.resume_node_id.is_empty()
        || request.resume_node_id.len() > 128
    {
        return Err(AppError::Invalid);
    }
    super::active(pool, job, lease).await?;
    let Some(placement) =
        super::relays::choose_placement(pool, job, lease.worker_id, &request.target).await?
    else {
        return Ok(false);
    };
    let (target, requirement) = match placement {
        super::relays::Placement::Pinned(worker) => (Some(worker), None),
        super::relays::Placement::Open(requirement) => (
            None,
            Some(serde_json::to_string(&requirement).map_err(|_| AppError::Internal)?),
        ),
    };
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let active: Option<Uuid> = sqlx::query_scalar("SELECT id FROM jobs WHERE id=?1 AND worker_id=?2 AND lease_token=?3 AND lease_epoch=?4 AND status=?5 AND leased_until > datetime('now')")
        .bind(job).bind(lease.worker_id).bind(lease.lease_token).bind(lease.lease_epoch).bind(JobStatus::Running).fetch_optional(&mut *tx).await?;
    if active.is_none() {
        return Err(AppError::Conflict);
    }
    // Une tentative manuelle ultérieure ne doit pas être bloquée par une remise d'un ancien bail.
    sqlx::query("DELETE FROM job_handoffs WHERE job_id=?1 AND source_lease_epoch != ?2 AND state IN (?3,?4,?5)")
        .bind(job).bind(lease.lease_epoch).bind(HandoffState::Uploading.as_str()).bind(HandoffState::Pending.as_str()).bind(HandoffState::Claimed.as_str()).execute(&mut *tx).await?;
    let count: i64 = sqlx::query_scalar(
        "SELECT COALESCE((SELECT relay_count FROM job_relays WHERE job_id=?1), 0)",
    )
    .bind(job)
    .fetch_one(&mut *tx)
    .await?;
    if count >= MAX_RELAYS_PER_JOB {
        tx.commit().await?;
        return Ok(false);
    }
    sqlx::query("INSERT INTO job_handoffs(job_id,from_worker_id,source_lease_epoch,target_worker_id,requirement_json,resume_node_id,artifact_name,artifact_sha256,artifact_bytes,state,expires_at,warning,variables_json) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,datetime('now','+' || ?11 || ' seconds'),?12,?13)")
        .bind(job).bind(lease.worker_id).bind(lease.lease_epoch).bind(target).bind(requirement).bind(&request.resume_node_id).bind(&artifact.name).bind(artifact.sha256.to_ascii_lowercase()).bind(artifact.bytes).bind(HandoffState::Uploading.as_str()).bind(HANDOFF_TTL_SECONDS).bind(warning).bind(super::relays::variables_json(&request.variables)?).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(true)
}

/// Métadonnées déclarées par l'émetteur, uniquement pendant son bail actif.
pub async fn handoff_upload_artifact(
    pool: &SqlitePool,
    job: Uuid,
    lease: &LeaseRequest,
) -> AppResult<HandoffArtifact> {
    super::active(pool, job, lease).await?;
    let row: Option<(String, String, i64)> = sqlx::query_as(
        "SELECT artifact_name,artifact_sha256,artifact_bytes FROM job_handoffs WHERE job_id=?1 AND from_worker_id=?2 AND source_lease_epoch=?3 AND state=?4 AND expires_at > datetime('now')",
    )
    .bind(job).bind(lease.worker_id).bind(lease.lease_epoch).bind(HandoffState::Uploading.as_str())
    .fetch_optional(pool).await?;
    let (name, sha256, bytes) = row.ok_or(AppError::Conflict)?;
    Ok(HandoffArtifact {
        name,
        sha256,
        bytes,
    })
}

/// Rend l'offre visible après stockage et vérification complets du fichier.
pub async fn finish_handoff_upload(
    pool: &SqlitePool,
    job: Uuid,
    lease: &LeaseRequest,
) -> AppResult<()> {
    let changed = sqlx::query("UPDATE job_handoffs SET state=?4 WHERE job_id=?1 AND from_worker_id=?2 AND source_lease_epoch=?3 AND state=?5 AND expires_at > datetime('now') AND EXISTS(SELECT 1 FROM jobs j WHERE j.id=?1 AND j.worker_id=?2 AND j.lease_epoch=?3 AND j.lease_token=?6 AND j.status=?7 AND j.leased_until > datetime('now'))")
        .bind(job).bind(lease.worker_id).bind(lease.lease_epoch).bind(HandoffState::Pending.as_str()).bind(HandoffState::Uploading.as_str()).bind(lease.lease_token).bind(JobStatus::Running)
        .execute(pool).await?.rows_affected();
    if changed != 1 {
        return Err(AppError::Conflict);
    }
    Ok(())
}

/// Vérifie le droit du repreneur à télécharger le fichier préparé.
pub async fn handoff_download_artifact(
    pool: &SqlitePool,
    job: Uuid,
    worker: Uuid,
    token: &str,
) -> AppResult<(i64, HandoffArtifact)> {
    let row: Option<(i64, String, String, i64)> = sqlx::query_as(
        "SELECT h.source_lease_epoch,h.artifact_name,h.artifact_sha256,h.artifact_bytes FROM job_handoffs h JOIN jobs j ON j.id=h.job_id \
         WHERE h.job_id=?1 AND h.receiver_worker_id=?2 AND h.receiver_token_hash=?3 AND h.state=?4 AND h.expires_at > datetime('now') \
         AND j.worker_id=h.from_worker_id AND j.lease_epoch=h.source_lease_epoch AND j.status=?5 AND j.leased_until > datetime('now')",
    ).bind(job).bind(worker).bind(receipt_hash(token)).bind(HandoffState::Claimed.as_str()).bind(JobStatus::Running)
        .fetch_optional(pool).await?;
    let (epoch, name, sha256, bytes) = row.ok_or(AppError::Conflict)?;
    Ok((
        epoch,
        HandoffArtifact {
            name,
            sha256,
            bytes,
        },
    ))
}

/// État visible par le seul worker qui a initié cette remise.
pub async fn handoff_state(
    pool: &SqlitePool,
    job: Uuid,
    worker: Uuid,
    epoch: i64,
) -> AppResult<HandoffState> {
    let state: Option<String> = sqlx::query_scalar("SELECT state FROM job_handoffs WHERE job_id=?1 AND from_worker_id=?2 AND source_lease_epoch=?3")
        .bind(job).bind(worker).bind(epoch).fetch_optional(pool).await?;
    let state = state.ok_or(AppError::NotFound)?;
    serde_json::from_value(serde_json::Value::String(state)).map_err(|_| AppError::Internal)
}

/// Une copie reçue reste nécessaire tant que sa remise est active ou que son premier bail attend ou s'exécute.
pub async fn retained_handoff_needed(
    pool: &SqlitePool,
    job: Uuid,
    worker: Uuid,
    epoch: i64,
) -> AppResult<bool> {
    Ok(sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM job_handoffs h JOIN jobs j ON j.id=h.job_id \
        WHERE h.job_id=?1 AND h.source_lease_epoch=?2 AND h.receiver_worker_id=?3 AND ( \
          (h.state=?4 AND h.expires_at > datetime('now') AND j.status=?5 AND j.worker_id=h.from_worker_id AND j.lease_epoch=h.source_lease_epoch AND j.leased_until > datetime('now')) \
          OR (h.state=?6 AND ((j.status=?7 AND j.target_worker_id=?3 AND j.lease_epoch=h.source_lease_epoch+1) \
          OR (j.status=?5 AND j.worker_id=?3 AND j.lease_epoch=h.source_lease_epoch+2)))))")
        .bind(job).bind(epoch).bind(worker).bind(HandoffState::Claimed.as_str()).bind(JobStatus::Running)
        .bind(HandoffState::Acknowledged.as_str()).bind(JobStatus::Queued).fetch_one(pool).await?)
}

/// Associe le fichier reçu uniquement au premier bail suivant l'accusé.
pub(super) async fn acknowledged_handoff_input(
    pool: &SqlitePool,
    job: Uuid,
    worker: Uuid,
    lease_epoch: i64,
) -> AppResult<Option<forgearr_shared::relay::HandoffInput>> {
    let row: Option<(i64, String, String, i64)> = sqlx::query_as(
        "SELECT source_lease_epoch,artifact_name,artifact_sha256,artifact_bytes FROM job_handoffs WHERE job_id=?1 AND receiver_worker_id=?2 AND state=?3 AND source_lease_epoch+2=?4",
    ).bind(job).bind(worker).bind(HandoffState::Acknowledged.as_str()).bind(lease_epoch).fetch_optional(pool).await?;
    Ok(row.map(
        |(source_lease_epoch, name, sha256, bytes)| forgearr_shared::relay::HandoffInput {
            source_lease_epoch,
            artifact: HandoffArtifact {
                name,
                sha256,
                bytes,
            },
        },
    ))
}

/// Réserve une remise à un seul repreneur éligible, sans modifier le bail d'origine.
pub async fn claim_handoff(pool: &SqlitePool, worker_id: Uuid) -> AppResult<Option<HandoffOffer>> {
    expire_handoffs(pool).await?;
    let worker = super::get_worker(pool, worker_id).await?;
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let candidates: Vec<PendingHandoff> = sqlx::query_as(
        "SELECT h.job_id,h.from_worker_id,h.source_lease_epoch,h.target_worker_id,h.requirement_json,h.artifact_name,h.artifact_sha256,h.artifact_bytes \
         FROM job_handoffs h JOIN jobs j ON j.id=h.job_id \
         WHERE h.state=?1 AND h.expires_at > datetime('now') AND h.from_worker_id != ?2 \
           AND (h.target_worker_id IS NULL OR h.target_worker_id=?2) \
           AND j.status=?3 AND j.worker_id=h.from_worker_id AND j.lease_epoch=h.source_lease_epoch \
           AND j.leased_until > datetime('now') ORDER BY h.created_at,h.job_id LIMIT ?4",
    ).bind(HandoffState::Pending.as_str()).bind(worker_id).bind(JobStatus::Running).bind(HANDOFF_CANDIDATE_LIMIT).fetch_all(&mut *tx).await?;
    for candidate in candidates {
        if candidate.target_worker_id.is_none() {
            let requirement = candidate
                .requirement_json
                .as_deref()
                .and_then(|raw| serde_json::from_str::<CapabilityRequirement>(raw).ok());
            if !requirement.is_some_and(|requirement| {
                worker
                    .telemetry
                    .as_ref()
                    .is_some_and(|telemetry| telemetry.capabilities.satisfies(&requirement))
            }) {
                continue;
            }
        }
        let access = format!(
            "SELECT {access} AND {reachable} FROM jobs WHERE id=?1",
            access = super::library_access::job_access_sql("?2", "?3"),
            reachable = super::library_access::source_reachable_sql("?4")
        );
        let eligible: Option<bool> = sqlx::query_scalar(&access)
            .bind(candidate.job_id)
            .bind(worker_id)
            .bind(forgearr_shared::workers::INTEGRATED_WORKER_NAME)
            .bind(WORKER_ONLINE_SECONDS)
            .fetch_optional(&mut *tx)
            .await?;
        if eligible != Some(true) {
            continue;
        }
        let token = new_receipt_token();
        let changed = sqlx::query("UPDATE job_handoffs SET state=?2,receiver_worker_id=?3,receiver_token_hash=?4 WHERE job_id=?1 AND state=?5 AND expires_at > datetime('now')")
            .bind(candidate.job_id).bind(HandoffState::Claimed.as_str()).bind(worker_id).bind(receipt_hash(&token)).bind(HandoffState::Pending.as_str()).execute(&mut *tx).await?.rows_affected();
        if changed != 1 {
            continue;
        }
        tx.commit().await?;
        return Ok(Some(HandoffOffer {
            job_id: candidate.job_id,
            from_worker_id: candidate.from_worker_id,
            source_lease_epoch: candidate.source_lease_epoch,
            artifact: HandoffArtifact {
                name: candidate.artifact_name,
                sha256: candidate.artifact_sha256,
                bytes: candidate.artifact_bytes,
            },
            receipt_token: token,
        }));
    }
    tx.commit().await?;
    Ok(None)
}

/// Termine les remises périmées sans remise en file ; un bail perdu garde son code propre.
pub async fn expire_handoffs(pool: &SqlitePool) -> AppResult<u64> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let changed = sqlx::query("UPDATE jobs SET status=?1,error_code=CASE WHEN leased_until <= datetime('now') THEN ?2 ELSE ?3 END, \
        lease_token=NULL,leased_until=NULL,lease_epoch=lease_epoch+1,updated_at=datetime('now') \
        WHERE status=?4 AND EXISTS(SELECT 1 FROM job_handoffs h WHERE h.job_id=jobs.id \
          AND h.source_lease_epoch=jobs.lease_epoch AND h.state IN (?5,?6,?7) \
          AND (h.expires_at <= datetime('now') OR jobs.leased_until <= datetime('now')))")
        .bind(JobStatus::Failed).bind(JobErrorCode::LeaseExpired).bind(JobErrorCode::TransferFailed).bind(JobStatus::Running)
        .bind(HandoffState::Uploading.as_str()).bind(HandoffState::Pending.as_str()).bind(HandoffState::Claimed.as_str()).execute(&mut *tx).await?.rows_affected();
    sqlx::query("UPDATE job_handoffs SET state=?1,receiver_token_hash=NULL WHERE state IN (?2,?3,?5) \
        AND (expires_at <= datetime('now') OR NOT EXISTS(SELECT 1 FROM jobs j WHERE j.id=job_handoffs.job_id \
          AND j.status=?4 AND j.worker_id=job_handoffs.from_worker_id AND j.lease_epoch=job_handoffs.source_lease_epoch \
          AND j.leased_until > datetime('now')))")
        .bind(HandoffState::Failed.as_str()).bind(HandoffState::Uploading.as_str()).bind(HandoffState::Pending.as_str())
        .bind(JobStatus::Running).bind(HandoffState::Claimed.as_str()).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(changed)
}

/// Libère le bail d'origine uniquement après réception vérifiée et accuse une seule fois.
pub async fn acknowledge_handoff(
    pool: &SqlitePool,
    job_id: Uuid,
    ack: &HandoffAcknowledgement,
) -> AppResult<bool> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let already: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM job_handoffs WHERE job_id=?1 AND state=?2 AND receiver_worker_id=?3 AND receiver_token_hash=?4 AND artifact_sha256=?5 AND artifact_bytes=?6)")
        .bind(job_id).bind(HandoffState::Acknowledged.as_str()).bind(ack.worker_id).bind(receipt_hash(&ack.receipt_token)).bind(&ack.sha256).bind(ack.bytes)
        .fetch_one(&mut *tx).await?;
    if already {
        tx.commit().await?;
        return Ok(false);
    }
    let handoff: Option<ClaimedHandoff> = sqlx::query_as(
        "SELECT from_worker_id,source_lease_epoch,resume_node_id,requirement_json,artifact_sha256,artifact_bytes,warning,variables_json \
         FROM job_handoffs WHERE job_id=?1 AND state=?2 AND receiver_worker_id=?3 \
           AND receiver_token_hash=?4 AND expires_at > datetime('now')",
    ).bind(job_id).bind(HandoffState::Claimed.as_str()).bind(ack.worker_id).bind(receipt_hash(&ack.receipt_token)).fetch_optional(&mut *tx).await?;
    let handoff = handoff.ok_or(AppError::Conflict)?;
    if ack.sha256 != handoff.artifact_sha256 || ack.bytes != handoff.artifact_bytes {
        return Err(AppError::Conflict);
    }
    let changed = sqlx::query("UPDATE jobs SET status=?4,worker_id=NULL,lease_token=NULL,lease_epoch=lease_epoch+1,leased_until=NULL,target_worker_id=?5,attempts=MAX(attempts-1,0),warning=CASE WHEN warning=1 OR ?7=1 THEN 1 ELSE 0 END,updated_at=datetime('now') \
         WHERE id=?1 AND worker_id=?2 AND lease_epoch=?3 AND status=?6 AND leased_until > datetime('now')")
        .bind(job_id).bind(handoff.from_worker_id).bind(handoff.source_lease_epoch).bind(JobStatus::Queued).bind(ack.worker_id).bind(JobStatus::Running).bind(handoff.warning)
        .execute(&mut *tx).await?.rows_affected();
    if changed != 1 {
        return Err(AppError::Conflict);
    }
    sqlx::query("DELETE FROM job_handoff_steps WHERE job_id=?1")
        .bind(job_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("INSERT INTO job_handoff_steps(job_id,step_index,node_id,block_kind,operation_id,status,message,output_excerpt,created_at,variables_json) SELECT job_id,step_index,node_id,block_kind,operation_id,status,message,output_excerpt,created_at,variables_json FROM job_steps WHERE job_id=?1")
        .bind(job_id).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO job_relays(job_id,from_worker_id,relay_count,resume_node_id,requirement_json,variables_json) VALUES(?1,?2,1,?3,?4,?5) \
        ON CONFLICT(job_id) DO UPDATE SET from_worker_id=excluded.from_worker_id,relay_count=relay_count+1, \
        resume_node_id=excluded.resume_node_id,requirement_json=excluded.requirement_json, \
        variables_json=excluded.variables_json,relayed_at=datetime('now')")
        .bind(job_id).bind(handoff.from_worker_id).bind(handoff.resume_node_id).bind(handoff.requirement_json).bind(handoff.variables_json).execute(&mut *tx).await?;
    sqlx::query("UPDATE job_handoffs SET state=?2,acknowledged_at=datetime('now') WHERE job_id=?1 AND source_lease_epoch=?3 AND state=?4")
        .bind(job_id).bind(HandoffState::Acknowledged.as_str()).bind(handoff.source_lease_epoch).bind(HandoffState::Claimed.as_str()).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn artifact_validation_rejects_traversal_invalid_hashes_and_oversize() {
        let mut artifact = HandoffArtifact {
            name: "working.mkv".into(),
            sha256: "ab".repeat(32),
            bytes: 10,
        };
        assert!(valid_artifact(&artifact, 10));
        artifact.name = "../working.mkv".into();
        assert!(!valid_artifact(&artifact, 10));
        artifact.name = "working.mkv".into();
        artifact.sha256 = "zz".repeat(32);
        assert!(!valid_artifact(&artifact, 10));
        artifact.sha256 = "ab".repeat(32);
        artifact.bytes = 11;
        assert!(!valid_artifact(&artifact, 10));
    }
}
