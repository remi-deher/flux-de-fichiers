//! Relais en cours de flux par artefact (ADR 0022) : le fichier de travail reste chez son détenteur
//! et le repreneur le tire en direct, sans passage par le central.

use forgearr::{
    domain::{Claim, JobErrorCode, JobStatus, JobStepEntry, JobStepStatus, LeaseRequest},
    error::AppError,
    repository::{self, RelayOutcome},
};
use forgearr_shared::{
    relay::{HandoffArtifact, RelayRequest, RelayTarget},
    transfer::{ArtifactDeclaration, ArtifactKind},
};
use sqlx::{SqlitePool, sqlite::SqlitePoolOptions};
use uuid::Uuid;

const ORIGIN_URL: &str = "http://origine.lan:9000";

async fn pool() -> anyhow::Result<SqlitePool> {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await?;
    sqlx::migrate!().run(&pool).await?;
    Ok(pool)
}

fn declaration() -> ArtifactDeclaration {
    ArtifactDeclaration {
        root: "/work/retained".into(),
        name: "handoff-file".into(),
        sha256: "cd".repeat(32),
        bytes: 42,
    }
}

fn step(node: &str, index: i32) -> JobStepEntry {
    JobStepEntry {
        step_index: index,
        node_id: node.into(),
        block_kind: "operation".into(),
        operation_id: None,
        status: JobStepStatus::Succeeded,
        message: None,
        output_excerpt: None,
    }
}

fn lease_of(worker: Uuid, claim: &Claim) -> LeaseRequest {
    LeaseRequest {
        worker_id: worker,
        lease_token: claim.lease_token,
        lease_epoch: claim.lease_epoch,
        telemetry: None,
    }
}

fn to_worker(worker: Uuid) -> RelayRequest {
    RelayRequest {
        target: RelayTarget::Worker { worker_id: worker },
        resume_node_id: "next".into(),
    }
}

/// Le détenteur réclame une tâche, y exécute une étape et déclare son fichier de travail ; il
/// annonce (ou non) une adresse de transfert.
async fn origin_with_artifact(
    pool: &SqlitePool,
    reachable: bool,
) -> anyhow::Result<(Uuid, Uuid, LeaseRequest)> {
    let origin = repository::register(pool, "origine").await?;
    if reachable {
        repository::set_transfer_url(pool, origin.id, Some(ORIGIN_URL)).await?;
    }
    let job = repository::insert_job(pool, "film.mkv", None, &"ab".repeat(32), 10).await?;
    let claim = repository::claim(pool, origin.id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("bail absent"))?;
    let lease = lease_of(origin.id, &claim);
    repository::save_job_steps_for_lease(pool, job.id, &lease, &[step("before", 1)]).await?;
    let decl = declaration();
    repository::declare_artifact(pool, job.id, &lease, ArtifactKind::Working, &decl, 1024).await?;
    Ok((job.id, origin.id, lease))
}

#[tokio::test]
async fn the_receiver_pulls_the_working_file_from_its_holder() -> anyhow::Result<()> {
    let pool = pool().await?;
    let (job, origin, origin_lease) = origin_with_artifact(&pool, true).await?;
    let receiver = repository::register(&pool, "repreneur").await?;
    let request = to_worker(receiver.id);
    let outcome = repository::relay_job(&pool, job, &origin_lease, &request, true).await?;
    let expected = RelayOutcome::Relayed {
        target_worker_id: Some(receiver.id),
    };
    assert_eq!(outcome, expected);
    let queued = repository::get_job(&pool, job).await?;
    assert_eq!(queued.status, JobStatus::Queued);
    assert_eq!(queued.target_worker_id, Some(receiver.id));
    assert!(queued.warning);

    // Le bail est libéré tout de suite ; le fichier de travail reste déclaré chez son détenteur.
    let mut claim = repository::claim(&pool, receiver.id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("reprise absente"))?;
    assert_eq!(claim.resume_node_id.as_deref(), Some("next"));
    assert!(claim.prior_warning);
    let input = claim
        .handoff_input
        .clone()
        .ok_or_else(|| anyhow::anyhow!("fichier de travail attendu"))?;
    let artifact = HandoffArtifact {
        name: "handoff-file".into(),
        sha256: "cd".repeat(32),
        bytes: 42,
    };
    assert_eq!(input.artifact, artifact);
    assert_eq!(input.source_lease_epoch, origin_lease.lease_epoch);

    // Le billet désigne l'artefact chez son détenteur ; seul celui-ci peut le valider.
    repository::attach_source_ticket(&pool, receiver.id, &mut claim).await?;
    let ticket = claim
        .source_ticket
        .clone()
        .ok_or_else(|| anyhow::anyhow!("billet attendu"))?;
    assert_eq!(ticket.url, ORIGIN_URL);
    let grant = repository::redeem_ticket(&pool, &ticket.token, origin).await?;
    assert_eq!(grant.root, "/work/retained");
    assert_eq!(grant.source_name, "handoff-file");
    assert_eq!(grant.sha256, "cd".repeat(32));
    assert_eq!(grant.bytes, 42);

    // Un billet frais remplace le précédent.
    let taker_lease = lease_of(receiver.id, &claim);
    let fresh = repository::reissue_source_ticket(&pool, job, &taker_lease)
        .await?
        .ok_or_else(|| anyhow::anyhow!("billet frais attendu"))?;
    assert_ne!(fresh.token, ticket.token);
    let replaced = repository::redeem_ticket(&pool, &ticket.token, origin).await;
    assert!(matches!(replaced, Err(AppError::Unauthorized)));
    repository::redeem_ticket(&pool, &fresh.token, origin).await?;

    // Les étapes du détenteur sont conservées et complétées par celles du repreneur.
    repository::save_job_steps_for_lease(&pool, job, &taker_lease, &[step("after", 1)]).await?;
    let steps = repository::list_job_steps(&pool, job).await?;
    let seen: Vec<_> = steps
        .iter()
        .map(|entry| (entry.step_index, entry.node_id.as_str()))
        .collect();
    assert_eq!(seen, [(1, "before"), (2, "after")]);

    // Seul le repreneur confirme la réception ; la confirmation est répétable.
    let by_origin = repository::confirm_artifact_received(&pool, job, &origin_lease).await;
    assert!(matches!(by_origin, Err(AppError::Conflict)));
    let confirmed = repository::confirm_artifact_received(&pool, job, &taker_lease).await?;
    assert!(confirmed);
    let repeated = repository::confirm_artifact_received(&pool, job, &taker_lease).await?;
    assert!(!repeated);
    let gone = repository::get_artifact(&pool, job, ArtifactKind::Working).await?;
    assert!(gone.is_none());
    Ok(())
}

#[tokio::test]
async fn a_receiver_lost_after_confirmation_restarts_from_the_source() -> anyhow::Result<()> {
    let pool = pool().await?;
    let (job, _, origin_lease) = origin_with_artifact(&pool, true).await?;
    let receiver = repository::register(&pool, "repreneur").await?;
    let request = to_worker(receiver.id);
    repository::relay_job(&pool, job, &origin_lease, &request, false).await?;
    let claim = repository::claim(&pool, receiver.id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("reprise absente"))?;
    let taker_lease = lease_of(receiver.id, &claim);
    let confirmed = repository::confirm_artifact_received(&pool, job, &taker_lease).await?;
    assert!(confirmed);

    // Le repreneur disparaît : la tâche revient en file sans fichier de travail à reprendre.
    sqlx::query(
        "UPDATE jobs SET status=?2, worker_id=NULL, lease_token=NULL, leased_until=NULL, \
             lease_epoch=lease_epoch+1, target_worker_id=NULL WHERE id=?1",
    )
    .bind(job)
    .bind(JobStatus::Queued)
    .execute(&pool)
    .await?;
    let again = repository::claim(&pool, receiver.id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("nouvelle reprise absente"))?;
    assert_eq!(again.resume_node_id, None);
    assert!(again.handoff_input.is_none());
    Ok(())
}

#[tokio::test]
async fn a_holder_without_transfer_address_cannot_relay_a_working_file() -> anyhow::Result<()> {
    let pool = pool().await?;
    let (job, _, origin_lease) = origin_with_artifact(&pool, false).await?;
    let receiver = repository::register(&pool, "repreneur").await?;
    let request = to_worker(receiver.id);
    let outcome = repository::relay_job(&pool, job, &origin_lease, &request, false).await;
    assert!(matches!(outcome, Err(AppError::Invalid)));
    let running = repository::get_job(&pool, job).await?;
    assert_eq!(running.status, JobStatus::Running);
    Ok(())
}

#[tokio::test]
async fn no_eligible_receiver_fails_the_job_and_releases_the_artifact() -> anyhow::Result<()> {
    let pool = pool().await?;
    let (job, origin, origin_lease) = origin_with_artifact(&pool, true).await?;
    let request = to_worker(origin);
    let outcome = repository::relay_job(&pool, job, &origin_lease, &request, false).await?;
    assert_eq!(outcome, RelayOutcome::Failed);
    let failed = repository::get_job(&pool, job).await?;
    assert_eq!(failed.status, JobStatus::Failed);
    let gone = repository::get_artifact(&pool, job, ArtifactKind::Working).await?;
    assert!(gone.is_none());
    Ok(())
}

#[tokio::test]
async fn a_finished_job_no_longer_needs_its_retained_file() -> anyhow::Result<()> {
    let pool = pool().await?;
    let (job, origin, origin_lease) = origin_with_artifact(&pool, true).await?;
    let epoch = origin_lease.lease_epoch;
    let receiver = repository::register(&pool, "repreneur").await?;
    let request = to_worker(receiver.id);
    repository::relay_job(&pool, job, &origin_lease, &request, false).await?;
    let kind = ArtifactKind::Working;
    let waiting = repository::artifact_retained(&pool, job, kind, origin, epoch).await?;
    assert!(waiting, "tâche en attente : le fichier reste réclamé");
    let other_epoch = repository::artifact_retained(&pool, job, kind, origin, epoch + 1).await?;
    assert!(!other_epoch);
    let other_worker = repository::artifact_retained(&pool, job, kind, receiver.id, epoch).await?;
    assert!(!other_worker);

    let claim = repository::claim(&pool, receiver.id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("reprise absente"))?;
    let taker_lease = lease_of(receiver.id, &claim);
    repository::fail(&pool, job, &taker_lease, JobErrorCode::TransferFailed).await?;
    let finished = repository::artifact_retained(&pool, job, kind, origin, epoch).await?;
    assert!(!finished, "tâche échouée : le détenteur peut purger");
    Ok(())
}
