//! Garantie de l'ADR 0027 : jamais deux titulaires ; toute écriture de l'ancien titulaire est refusée.

mod common;

use common::leases::{
    action, claim, declaration, expire, leased_job, output, pool, queued_job, sweep,
};
use forgearr::{
    api::connect_database,
    config::Config,
    domain::{IrreversibleAction, JobErrorCode, JobStatus, LeaseRequest, LibraryOutputPolicy},
    error::{AppError, AppResult},
    repository,
};
use forgearr_shared::{
    jobs::LeaseProtocol,
    relay::{RelayRequest, RelayTarget},
    transfer::ArtifactKind,
};
use sqlx::SqlitePool;
use std::path::Path;
use uuid::Uuid;

const MAX_BYTES: u64 = 1024;
const CONTENDERS: usize = 8;

/// Base sur disque : plusieurs connexions réellement concurrentes.
async fn file_pool(dir: &Path) -> anyhow::Result<SqlitePool> {
    connect_database(&Config {
        bind: ([127, 0, 0, 1], 3000).into(),
        database_url: dir.join("flux.db").to_string_lossy().into_owned(),
        data_dir: dir.to_path_buf(),
        input_dir: dir.join("input"),
        config_dir: dir.join("config"),
        tools_cache_dir: dir.join("tools"),
        plugins_dir: None,
        detected_plugins_dir: None,
        static_dir: dir.join("static"),
        max_file_bytes: MAX_BYTES,
        local_worker: false,
        web_origin: None,
        web_session_ttl_seconds: 28_800,
        tls: None,
        smtp: None,
    })
    .await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_claims_give_one_job_to_a_single_worker() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let pool = file_pool(dir.path()).await?;
    let job = queued_job(&pool, "disputee.mkv").await?;
    let mut claims = tokio::task::JoinSet::new();
    for index in 0..CONTENDERS {
        let worker = repository::register(&pool, &format!("w{index}")).await?;
        let pool = pool.clone();
        claims.spawn(async move {
            repository::claim_with_protocol(&pool, worker.id, LeaseProtocol::LATEST).await
        });
    }
    let mut holders = 0;
    while let Some(outcome) = claims.join_next().await {
        if let Some(claim) = outcome?? {
            assert_eq!(claim.job_id, job);
            holders += 1;
        }
    }
    assert_eq!(holders, 1);
    assert_eq!(repository::get_job(&pool, job).await?.attempts, 1);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_beat_racing_the_sweep_is_either_kept_or_refused() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let pool = file_pool(dir.path()).await?;
    for round in 0..CONTENDERS {
        let (job, lease) = leased_job(&pool, &format!("course{round}")).await?;
        common::leases::overdue(&pool, job).await?;
        let (beat_pool, beat_lease) = (pool.clone(), lease.clone());
        let beat =
            tokio::spawn(async move { repository::heartbeat(&beat_pool, job, &beat_lease).await });
        let sweep_pool = pool.clone();
        let swept = tokio::spawn(async move { sweep(&sweep_pool).await });
        let beat = beat.await?;
        swept.await??;
        let status = repository::get_job(&pool, job).await?.status;
        match beat {
            Ok(_) => assert_eq!(status, JobStatus::Running),
            Err(AppError::LeaseLost) => assert_eq!(status, JobStatus::Queued),
            Err(error) => return Err(error.into()),
        }
        // La tâche ne doit pas être offerte au tour suivant.
        repository::cancel_job(&pool, job).await?;
    }
    Ok(())
}

/// Issue de chaque écriture possible d'un titulaire, par nom.
async fn every_write(
    pool: &SqlitePool,
    job: Uuid,
    lease: &LeaseRequest,
    receiver: Uuid,
) -> Vec<(&'static str, AppResult<()>)> {
    let policy = LibraryOutputPolicy::ReplaceInPlace;
    let relay = RelayRequest {
        target: RelayTarget::Worker {
            worker_id: receiver,
        },
        resume_node_id: "n2".into(),
        variables: Default::default(),
        node_outputs: Default::default(),
    };
    let in_place = action(lease, IrreversibleAction::InPlace, "n1");
    let http = action(lease, IrreversibleAction::HttpCall, "n1");
    let mut flow = action(lease, IrreversibleAction::FailureFlow, "");
    flow.error_code = Some(JobErrorCode::OperationFailed);
    let sample = serde_json::json!({"codec": "hevc"});
    let mut writes = vec![
        (
            "battement",
            repository::heartbeat(pool, job, lease).await.map(drop),
        ),
        // La progression n'écrit rien : sa route exige ce même contrôle.
        (
            "progression",
            repository::active(pool, job, lease).await.map(drop),
        ),
        (
            "étapes",
            repository::save_job_steps_for_lease(pool, job, lease, &[]).await,
        ),
        (
            "échantillon",
            repository::save_flow_media_sample(pool, job, lease, &sample).await,
        ),
        (
            "billet",
            repository::reissue_source_ticket(pool, job, lease)
                .await
                .map(drop),
        ),
        (
            "réception",
            repository::confirm_artifact_received(pool, job, lease)
                .await
                .map(drop),
        ),
        (
            "opération en place",
            repository::authorize_action(pool, job, &in_place).await,
        ),
        (
            "appel HTTP",
            repository::authorize_action(pool, job, &http).await,
        ),
        (
            "flux d'échec",
            repository::authorize_action(pool, job, &flow).await,
        ),
        (
            "intention de publication",
            repository::begin_source_publication(pool, job, lease, output(), policy).await,
        ),
        (
            "intention tirée",
            repository::begin_pulled_publication(pool, job, lease, output(), "mkv", policy).await,
        ),
        (
            "application locale",
            repository::begin_local_publication(pool, job, lease, output(), "mkv", policy).await,
        ),
        (
            "autorisation de publication",
            repository::authorize_source_publication(pool, job, lease)
                .await
                .map(drop),
        ),
        (
            "source modifiée",
            repository::fail_changed_source(pool, job, lease).await,
        ),
        (
            "relais",
            repository::relay_job(pool, job, lease, &relay, false)
                .await
                .map(drop),
        ),
        (
            "échec",
            repository::fail(pool, job, lease, JobErrorCode::OperationFailed).await,
        ),
        (
            "résultat",
            repository::publish_succeeded(pool, job, lease, output(), async {
                Err(AppError::Internal)
            })
            .await,
        ),
        (
            "restitution",
            repository::release_lease(pool, job, lease).await.map(drop),
        ),
    ];
    for kind in [ArtifactKind::Working, ArtifactKind::Result] {
        let decl = declaration();
        let declared = repository::declare_artifact(pool, job, lease, kind, &decl, MAX_BYTES).await;
        writes.push(("artefact", declared));
    }
    writes
}

/// Toutes les colonnes qu'un titulaire peut faire bouger.
async fn snapshot(pool: &SqlitePool, job: Uuid) -> anyhow::Result<String> {
    let row: String = sqlx::query_scalar(
        "SELECT json_array(status, hex(worker_id), hex(lease_token), lease_epoch, attempts, \
         leased_until, lease_contact_at, unreachable_since, lease_orphaned, error_code, output_name, failure_flow_state, warning, \
         (SELECT COUNT(*) FROM job_artifacts), (SELECT COUNT(*) FROM job_steps), \
         (SELECT COUNT(*) FROM source_publication_journal), (SELECT COUNT(*) FROM source_tickets), \
         (SELECT COUNT(*) FROM job_irreversible_actions), (SELECT COUNT(*) FROM job_relays), ungated_lease) \
         FROM jobs WHERE id=?1",
    )
    .bind(job)
    .fetch_one(pool)
    .await?;
    Ok(row)
}

async fn assert_every_write_is_refused(
    pool: &SqlitePool,
    job: Uuid,
    lease: &LeaseRequest,
    receiver: Uuid,
) -> anyhow::Result<()> {
    let before = snapshot(pool, job).await?;
    for (name, outcome) in every_write(pool, job, lease, receiver).await {
        assert!(
            matches!(outcome, Err(AppError::LeaseLost)),
            "{name} : {outcome:?}"
        );
    }
    assert_eq!(snapshot(pool, job).await?, before, "aucune trace");
    Ok(())
}

#[tokio::test]
async fn every_write_of_a_former_holder_is_refused() -> anyhow::Result<()> {
    let pool = pool().await?;
    let (job, old) = leased_job(&pool, "ancien").await?;
    let next = repository::register(&pool, "nouveau").await?;
    expire(&pool, job).await?;
    // En file, avant toute nouvelle réservation.
    assert_every_write_is_refused(&pool, job, &old, next.id).await?;
    // Puis sous le nouveau titulaire, qui n'en est pas affecté.
    let (again, fresh) = claim(&pool, next.id).await?;
    assert_eq!(again, job);
    assert_every_write_is_refused(&pool, job, &old, next.id).await?;
    repository::heartbeat(&pool, job, &fresh).await?;
    Ok(())
}

#[tokio::test]
async fn the_same_worker_cannot_write_with_its_former_lease() -> anyhow::Result<()> {
    let pool = pool().await?;
    let (job, old) = leased_job(&pool, "w").await?;
    expire(&pool, job).await?;
    claim(&pool, old.worker_id).await?;
    assert_every_write_is_refused(&pool, job, &old, old.worker_id).await
}

#[tokio::test]
async fn a_job_canceled_while_unreachable_refuses_its_former_holder() -> anyhow::Result<()> {
    let pool = pool().await?;
    let (job, lease) = leased_job(&pool, "w").await?;
    repository::restart_contacts(&pool).await?;
    repository::cancel_job(&pool, job).await?;
    assert_every_write_is_refused(&pool, job, &lease, lease.worker_id).await
}
