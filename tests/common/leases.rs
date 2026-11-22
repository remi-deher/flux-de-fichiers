//! Baux de test (ADR 0027) : base, réservation, échéance et flux d'échec.

use forgearr::{
    domain::{
        AuthorizeAction, IrreversibleAction, JobStepEntry, JobStepStatus, LeaseRequest,
        UpdateFlowRequest,
    },
    repository::{self, LeaseChange},
};
use forgearr_shared::{jobs::LeaseProtocol, transfer::ArtifactDeclaration};
use sqlx::{SqlitePool, sqlite::SqlitePoolOptions};
use uuid::Uuid;

pub const FAILURE_FLOW: &str = "flow.echec";
const SOURCE_BYTES: i64 = 12;

pub async fn pool() -> anyhow::Result<SqlitePool> {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await?;
    sqlx::migrate!().run(&pool).await?;
    Ok(pool)
}

pub async fn queued_job(pool: &SqlitePool, name: &str) -> anyhow::Result<Uuid> {
    let job = repository::insert_job(pool, name, None, &"ab".repeat(32), SOURCE_BYTES).await?;
    Ok(job.id)
}

/// Réserve la prochaine tâche de ce worker au protocole courant.
pub async fn claim(pool: &SqlitePool, worker: Uuid) -> anyhow::Result<(Uuid, LeaseRequest)> {
    let claim = repository::claim_with_protocol(pool, worker, LeaseProtocol::LATEST)
        .await?
        .ok_or_else(|| anyhow::anyhow!("bail attendu"))?;
    let lease = LeaseRequest {
        worker_id: worker,
        lease_token: claim.lease_token,
        lease_epoch: claim.lease_epoch,
        telemetry: None,
    };
    Ok((claim.job_id, lease))
}

/// Crée une tâche et la réserve pour le worker `worker`.
pub async fn leased_job(pool: &SqlitePool, worker: &str) -> anyhow::Result<(Uuid, LeaseRequest)> {
    let worker = repository::register(pool, worker).await?;
    queued_job(pool, &format!("{}.mkv", worker.name)).await?;
    claim(pool, worker.id).await
}

pub async fn sweep(pool: &SqlitePool) -> anyhow::Result<Vec<LeaseChange>> {
    Ok(repository::sweep(pool, None, &Default::default())
        .await?
        .changes)
}

/// Dernier contact il y a `seconds` secondes.
pub async fn silent_for(pool: &SqlitePool, job: Uuid, seconds: i64) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE jobs SET lease_contact_at=datetime('now', '-' || ?2 || ' seconds') WHERE id=?1",
    )
    .bind(job)
    .bind(seconds)
    .execute(pool)
    .await?;
    Ok(())
}

/// Amène le bail à son échéance, sans le balayer.
pub async fn overdue(pool: &SqlitePool, job: Uuid) -> anyhow::Result<()> {
    sqlx::query("UPDATE jobs SET leased_until=datetime('now', '-1 seconds') WHERE id=?1")
        .bind(job)
        .execute(pool)
        .await?;
    Ok(())
}

/// Fait expirer le bail et balaye.
pub async fn expire(pool: &SqlitePool, job: Uuid) -> anyhow::Result<Vec<LeaseChange>> {
    overdue(pool, job).await?;
    sweep(pool).await
}

pub fn action(lease: &LeaseRequest, action: IrreversibleAction, node: &str) -> AuthorizeAction {
    AuthorizeAction {
        worker_id: lease.worker_id,
        lease_token: lease.lease_token,
        lease_epoch: lease.lease_epoch,
        action,
        node_id: node.into(),
        error_code: None,
    }
}

pub fn step(node: &str) -> JobStepEntry {
    JobStepEntry {
        step_index: 1,
        node_id: node.into(),
        block_kind: "test".into(),
        operation_id: None,
        status: JobStepStatus::Succeeded,
        message: None,
        output_excerpt: None,
        variables: None,
    }
}

pub fn declaration() -> ArtifactDeclaration {
    ArtifactDeclaration {
        root: "/work/retained".into(),
        name: "fichier.mkv".into(),
        sha256: "cd".repeat(32),
        bytes: 42,
    }
}

pub fn output() -> repository::PublishedOutput<'static> {
    repository::PublishedOutput {
        name: "resultat.mkv",
        sha256: "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee",
        bytes: 9,
        warning: false,
    }
}

/// Définit un flux d'échec global : toute tâche créée ensuite le fige.
pub async fn with_global_failure_flow(pool: &SqlitePool) -> anyhow::Result<()> {
    let request = UpdateFlowRequest {
        name: FAILURE_FLOW.into(),
        description: None,
        file_mask: "*.mkv".into(),
        is_default: false,
        definition:
            serde_json::json!({"nodes": [{"id": "in", "type": "file-input", "name": "Entrée"}],
                                       "connections": []})
            .to_string(),
    };
    repository::save_flow(pool, FAILURE_FLOW, &request).await?;
    repository::set_default_failure_flow(pool, Some(FAILURE_FLOW)).await?;
    Ok(())
}
