//! Couverture directe des fonctions du dépôt jobs et flux.

use forgearr::{
    domain::{JobErrorCode, JobStatus, LeaseRequest, MAX_ATTEMPTS, UpdateFlowRequest},
    error::AppError,
    repository,
};
use sqlx::sqlite::SqlitePoolOptions;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use uuid::Uuid;

async fn pool() -> anyhow::Result<sqlx::SqlitePool> {
    let pool = SqlitePoolOptions::new().connect("sqlite::memory:").await?;
    sqlx::migrate!().run(&pool).await?;
    Ok(pool)
}

#[tokio::test]
async fn job_lease_publication_and_flow_queries() -> anyhow::Result<()> {
    let pool = pool().await?;
    repository::ping(&pool).await?;

    let missing = Uuid::nil();
    assert!(!repository::worker_exists(&pool, missing).await?);
    assert!(matches!(
        repository::delete_worker(&pool, missing).await,
        Err(AppError::NotFound)
    ));
    assert!(matches!(
        repository::claim(&pool, missing).await,
        Err(AppError::NotFound)
    ));

    let worker = repository::register(&pool, "worker-a").await?;
    let again = repository::register(&pool, "worker-a").await?;
    assert_eq!(again.id, worker.id);
    assert!(repository::worker_exists(&pool, worker.id).await?);
    assert_eq!(repository::list_workers(&pool).await?.len(), 1);

    let job = repository::insert_job(&pool, "film.mkv", None, &"ab".repeat(32), 12).await?;
    assert_eq!(job.status, JobStatus::Queued);
    assert_eq!(job.error_code, None);
    assert_eq!(repository::get_job(&pool, job.id).await?.id, job.id);
    assert_eq!(repository::list_jobs(&pool, None, 10).await?.len(), 1);
    assert!(
        repository::list_jobs(&pool, Some(job.id), 10)
            .await?
            .is_empty()
    );

    let claim = repository::claim(&pool, worker.id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("bail attendu"))?;
    assert_eq!(claim.job_id, job.id);
    let lease = LeaseRequest {
        worker_id: worker.id,
        lease_token: claim.lease_token,
        lease_epoch: claim.lease_epoch,
        telemetry: None,
    };
    repository::heartbeat(&pool, job.id, &lease).await?;
    assert_eq!(
        repository::active(&pool, job.id, &lease).await?.status,
        JobStatus::Running
    );

    sqlx::query("UPDATE jobs SET leased_until=datetime('now', '-1 seconds') WHERE id=?1")
        .bind(job.id)
        .execute(&pool)
        .await?;
    // Bail expiré : échec explicite, aucune remise en file automatique.
    assert!(repository::claim(&pool, worker.id).await?.is_none());
    let lost = repository::get_job(&pool, job.id).await?;
    assert_eq!(lost.status, JobStatus::Failed);
    assert_eq!(lost.error_code, Some(JobErrorCode::LeaseExpired));
    assert!(matches!(
        repository::heartbeat(&pool, job.id, &lease).await,
        Err(AppError::Conflict)
    ));
    assert!(matches!(
        repository::retry_job(&pool, Uuid::nil()).await,
        Err(AppError::NotFound)
    ));
    let requeued = repository::retry_job(&pool, job.id).await?;
    assert_eq!(requeued.status, JobStatus::Queued);
    assert_eq!(requeued.error_code, None);
    assert!(matches!(
        repository::retry_job(&pool, job.id).await,
        Err(AppError::Conflict)
    ));
    let renewed = repository::claim(&pool, worker.id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("reprise du bail"))?;
    let running = repository::get_job(&pool, job.id).await?;
    assert_eq!(running.status, JobStatus::Running);
    assert_eq!(running.attempts, 2);
    assert_eq!(running.error_code, None);

    sqlx::query(
        "UPDATE jobs SET attempts=?1, leased_until=datetime('now', '-1 seconds') WHERE id=?2",
    )
    .bind(MAX_ATTEMPTS)
    .bind(job.id)
    .execute(&pool)
    .await?;
    assert!(repository::claim(&pool, worker.id).await?.is_none());
    let expired = repository::get_job(&pool, job.id).await?;
    assert_eq!(expired.status, JobStatus::Failed);
    assert_eq!(expired.error_code, Some(JobErrorCode::LeaseExpired));
    assert!(matches!(
        repository::retry_job(&pool, job.id).await,
        Err(AppError::Conflict)
    ));

    let second =
        repository::insert_job(&pool, "autre.mkv", Some(worker.id), &"cd".repeat(32), 8).await?;
    let claim = repository::claim(&pool, worker.id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("second bail"))?;
    assert_eq!(claim.job_id, second.id);
    let lease = LeaseRequest {
        worker_id: worker.id,
        lease_token: claim.lease_token,
        lease_epoch: claim.lease_epoch,
        telemetry: None,
    };
    let stale = LeaseRequest {
        worker_id: worker.id,
        lease_token: renewed.lease_token,
        lease_epoch: renewed.lease_epoch,
        telemetry: None,
    };
    assert!(matches!(
        repository::heartbeat(&pool, second.id, &stale).await,
        Err(AppError::Conflict)
    ));
    let linked = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&linked);
    assert!(matches!(
        repository::publish_succeeded(
            &pool,
            second.id,
            &stale,
            repository::PublishedOutput {
                name: "out.mkv",
                sha256: &"ef".repeat(32),
                bytes: 8,
                warning: false,
            },
            async move {
                flag.store(true, Ordering::SeqCst);
                Ok(())
            },
        )
        .await,
        Err(AppError::Conflict)
    ));
    assert!(!linked.load(Ordering::SeqCst));

    repository::publish_succeeded(
        &pool,
        second.id,
        &lease,
        repository::PublishedOutput {
            name: "out.mkv",
            sha256: &"ef".repeat(32),
            bytes: 8,
            warning: true,
        },
        async { Ok(()) },
    )
    .await?;
    let published = repository::get_job(&pool, second.id).await?;
    assert_eq!(published.status, JobStatus::Succeeded);
    assert_eq!(published.output_name.as_deref(), Some("out.mkv"));
    assert!(published.warning);
    assert_eq!(
        repository::succeeded_output_name(&pool, second.id).await?,
        Some("out.mkv".into())
    );
    assert_eq!(
        repository::succeeded_output_name(&pool, job.id).await?,
        None
    );

    let third = repository::insert_job(&pool, "fail.mkv", None, &"11".repeat(32), 4).await?;
    let claim = repository::claim(&pool, worker.id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("bail d'échec"))?;
    let lease = LeaseRequest {
        worker_id: worker.id,
        lease_token: claim.lease_token,
        lease_epoch: claim.lease_epoch,
        telemetry: None,
    };
    repository::fail(&pool, third.id, &lease, JobErrorCode::OperationFailed).await?;
    assert_eq!(
        repository::get_job(&pool, third.id).await?.error_code,
        Some(JobErrorCode::OperationFailed)
    );
    assert!(matches!(
        repository::fail(&pool, third.id, &lease, JobErrorCode::TransferFailed).await,
        Err(AppError::Conflict)
    ));
    assert!(matches!(
        repository::active(&pool, third.id, &lease).await,
        Err(AppError::Conflict)
    ));

    assert!(repository::default_flow(&pool).await?.is_none());
    let saved = repository::save_flow(
        &pool,
        "flow.test",
        &UpdateFlowRequest {
            name: "Test".into(),
            description: None,
            file_mask: "*.mkv".into(),
            is_default: true,
            definition: "{\"nodes\":[],\"connections\":[]}".into(),
        },
    )
    .await?;
    assert!(saved.is_default);
    assert_eq!(saved.file_mask, "*.mkv");
    let loaded = repository::default_flow(&pool)
        .await?
        .ok_or_else(|| anyhow::anyhow!("flux par défaut"))?;
    assert_eq!(loaded.id, "flow.test");
    assert_eq!(repository::list_flows(&pool).await?.len(), 1);
    assert_eq!(repository::get_flow(&pool, "flow.test").await?.name, "Test");
    assert!(matches!(
        repository::delete_flow(&pool, "flow.test").await,
        Err(AppError::Conflict)
    ));
    repository::save_flow(
        &pool,
        "flow.custom",
        &UpdateFlowRequest {
            name: "Custom".into(),
            description: Some("note".into()),
            file_mask: "*.mkv".into(),
            is_default: false,
            definition: "{}".into(),
        },
    )
    .await?;
    repository::delete_flow(&pool, "flow.custom").await?;
    assert!(matches!(
        repository::get_flow(&pool, "flow.custom").await,
        Err(AppError::NotFound)
    ));
    assert!(matches!(
        repository::delete_flow(&pool, "absent").await,
        Err(AppError::NotFound)
    ));

    let spare = repository::register(&pool, "worker-b").await?;
    repository::delete_worker(&pool, spare.id).await?;
    assert!(!repository::worker_exists(&pool, spare.id).await?);
    Ok(())
}

fn flow_request(name: &str, definition: &str) -> UpdateFlowRequest {
    UpdateFlowRequest {
        name: name.into(),
        description: None,
        file_mask: "*.mkv".into(),
        is_default: true,
        definition: definition.into(),
    }
}

#[tokio::test]
async fn jobs_keep_the_flow_version_frozen_at_creation() -> anyhow::Result<()> {
    let pool = pool().await?;
    let without_flow =
        repository::insert_job(&pool, "avant.mkv", None, &"ab".repeat(32), 1).await?;
    assert_eq!(without_flow.flow_id, None);
    assert_eq!(without_flow.flow_version, None);

    let first = repository::save_flow(&pool, "flow.v", &flow_request("V", "{\"a\":1}")).await?;
    assert_eq!(first.version, 1);
    let same = repository::save_flow(&pool, "flow.v", &flow_request("V", "{\"a\":1}")).await?;
    assert_eq!(
        same.version, 1,
        "un contenu identique ne crée pas de version"
    );

    let job = repository::insert_job(&pool, "film.mkv", None, &"ab".repeat(32), 1).await?;
    assert_eq!(job.flow_id.as_deref(), Some("flow.v"));
    assert_eq!(job.flow_version, Some(1));

    // Deux tâches créées dans la même seconde sortent dans un ordre non garanti
    // (départage par id aléatoire) : un second worker prend l'autre au besoin.
    let mut worker = repository::register(&pool, "claim-flow").await?;
    let mut claim = repository::claim(&pool, worker.id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("bail avec flux figé"))?;
    if claim.job_id != job.id {
        worker = repository::register(&pool, "claim-flow-2").await?;
        claim = repository::claim(&pool, worker.id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("bail de la tâche avec flux"))?;
    }
    assert_eq!(claim.job_id, job.id);
    let claimed_flow = claim
        .frozen_flow
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("le bail doit porter le flux figé"))?;
    assert_eq!(claimed_flow.id, "flow.v");
    assert_eq!(claimed_flow.version, 1);
    assert_eq!(claimed_flow.definition, "{\"a\":1}");
    assert!(claim.operations.is_empty());
    assert!(claim.block_operations.is_empty());
    assert!(claim.plugin_configs.is_empty());
    repository::fail(
        &pool,
        job.id,
        &LeaseRequest {
            worker_id: worker.id,
            lease_token: claim.lease_token,
            lease_epoch: claim.lease_epoch,
            telemetry: None,
        },
        JobErrorCode::OperationFailed,
    )
    .await?;

    let second = repository::save_flow(&pool, "flow.v", &flow_request("V", "{\"a\":2}")).await?;
    assert_eq!(second.version, 2);
    let kept = repository::get_job(&pool, job.id).await?;
    assert_eq!(kept.flow_version, Some(1), "la tâche garde sa version");
    assert_eq!(
        kept.status,
        JobStatus::Failed,
        "échec volontaire après le bail"
    );
    let frozen = repository::get_flow_version(&pool, "flow.v", 1).await?;
    assert_eq!(frozen.definition, "{\"a\":1}");
    assert_eq!(frozen.version, 1);
    let listed = repository::list_flow_versions(&pool, "flow.v").await?;
    assert_eq!(
        listed.iter().map(|v| v.version).collect::<Vec<_>>(),
        vec![2, 1]
    );
    assert!(matches!(
        repository::list_flow_versions(&pool, "absent").await,
        Err(AppError::NotFound)
    ));
    let next = repository::insert_job(&pool, "suite.mkv", None, &"ab".repeat(32), 1).await?;
    assert_eq!(next.flow_version, Some(2));

    assert!(
        sqlx::query("UPDATE flow_versions SET name='x' WHERE flow_id='flow.v'")
            .execute(&pool)
            .await
            .is_err(),
        "une version publiée est immuable"
    );
    assert!(
        sqlx::query("DELETE FROM flow_versions")
            .execute(&pool)
            .await
            .is_err()
    );

    repository::save_flow(&pool, "flow.autre", &flow_request("Autre", "{}")).await?;
    repository::delete_flow(&pool, "flow.v").await?;
    let survivor = repository::get_flow_version(&pool, "flow.v", 1).await?;
    assert_eq!(survivor.definition, "{\"a\":1}");
    assert!(!survivor.is_default);
    let recreated = repository::save_flow(&pool, "flow.v", &flow_request("V", "{}")).await?;
    assert_eq!(
        recreated.version, 3,
        "la numérotation reprend après suppression"
    );
    assert!(matches!(
        repository::get_flow_version(&pool, "flow.v", 9).await,
        Err(AppError::NotFound)
    ));
    Ok(())
}

#[tokio::test]
async fn jobs_freeze_library_flow_record_steps_and_clear_output_explicitly() -> anyhow::Result<()> {
    let pool = pool().await?;
    repository::save_flow(&pool, "flow.default", &flow_request("Default", "{\"d\":1}")).await?;
    let mut lib_flow_req = flow_request("Lib", "{\"l\":1}");
    lib_flow_req.is_default = false;
    repository::save_flow(&pool, "flow.lib", &lib_flow_req).await?;

    let worker = repository::register(&pool, "w-lib").await?;
    let library = repository::insert_library(
        &pool,
        forgearr::domain::LibraryWrite {
            name: "Séries".into(),
            flow_id: Some("flow.lib".into()),
            workers: vec![forgearr::domain::LibraryWorkerPath {
                worker_id: worker.id,
                local_path: "/srv/series".into(),
                access: forgearr::domain::LibraryAccess::ReadWrite,
            }],
            ..Default::default()
        },
    )
    .await?;

    let invalid = repository::insert_job_with_options(
        &pool,
        "film.mkv",
        None,
        &"ab".repeat(32),
        10,
        Some(uuid::Uuid::new_v4()),
        None,
    )
    .await;
    assert!(matches!(invalid, Err(AppError::Invalid)));

    let job = repository::insert_job_with_options(
        &pool,
        "film.mkv",
        None,
        &"ab".repeat(32),
        10,
        Some(library.id),
        None,
    )
    .await?;
    assert_eq!(job.library_id, Some(library.id));
    assert_eq!(job.flow_id.as_deref(), Some("flow.lib"));
    assert_eq!(
        repository::library_worker_local_path(&pool, library.id, worker.id)
            .await?
            .as_deref(),
        Some("/srv/series")
    );

    let claim = repository::claim(&pool, worker.id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("bail attendu"))?;
    assert_eq!(claim.library_id, Some(library.id));
    let lease = LeaseRequest {
        worker_id: worker.id,
        lease_token: claim.lease_token,
        lease_epoch: claim.lease_epoch,
        telemetry: None,
    };

    repository::save_job_steps(
        &pool,
        job.id,
        &[forgearr::domain::JobStepEntry {
            step_index: 0,
            node_id: "node-1".into(),
            block_kind: "encode".into(),
            operation_id: Some("ffmpeg.encode".into()),
            status: forgearr::domain::JobStepStatus::Succeeded,
            message: Some("OK".into()),
            output_excerpt: Some("frame=10".into()),
        }],
    )
    .await?;
    let steps = repository::list_job_steps(&pool, job.id).await?;
    assert_eq!(steps.len(), 1);
    assert_eq!(steps[0].operation_id.as_deref(), Some("ffmpeg.encode"));

    assert!(matches!(
        repository::clear_job_output_name(&pool, job.id).await,
        Err(AppError::Conflict)
    ));
    repository::publish_succeeded(
        &pool,
        job.id,
        &lease,
        repository::PublishedOutput {
            name: "out.mp4",
            sha256: &"cd".repeat(32),
            bytes: 42,
            warning: false,
        },
        async { Ok(()) },
    )
    .await?;
    assert_eq!(
        repository::clear_job_output_name(&pool, job.id)
            .await?
            .as_deref(),
        Some("out.mp4")
    );
    assert_eq!(
        repository::succeeded_output_name(&pool, job.id).await?,
        None
    );
    Ok(())
}

#[tokio::test]
async fn priority_cancellation_batch_concurrency_schedule_and_stats() -> anyhow::Result<()> {
    let pool = pool().await?;
    let worker = repository::register(&pool, "worker-sched").await?;
    assert_eq!(worker.max_concurrency, 1);

    let low = repository::insert_job_with_priority(
        &pool,
        "low.mkv",
        None,
        &"11".repeat(32),
        1_000,
        None,
        None,
        -10,
    )
    .await?;
    let high = repository::insert_job_with_priority(
        &pool,
        "high.mkv",
        None,
        &"22".repeat(32),
        2_000,
        None,
        None,
        20,
    )
    .await?;
    let third = repository::insert_job_with_priority(
        &pool,
        "third.mkv",
        None,
        &"33".repeat(32),
        500,
        None,
        None,
        5,
    )
    .await?;

    // La priorité la plus haute est réclamée en premier.
    let first_claim = repository::claim(&pool, worker.id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("high priority claim expected"))?;
    assert_eq!(first_claim.job_id, high.id);

    // Concurrence max = 1 bloque une deuxième réclamation.
    assert!(repository::claim(&pool, worker.id).await?.is_none());

    // Passer la concurrence à 2 débloque la tâche suivante (third, priorité 5 > -10).
    let updated_worker = repository::update_worker_settings(&pool, worker.id, 2, None).await?;
    assert_eq!(updated_worker.max_concurrency, 2);
    let second_claim = repository::claim(&pool, worker.id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("second concurrent claim expected"))?;
    assert_eq!(second_claim.job_id, third.id);

    // Annuler une tâche en cours invalide son bail immédiatement.
    let canceled_running = repository::cancel_job(&pool, third.id).await?;
    assert_eq!(canceled_running.status, JobStatus::Canceled);
    let stale_lease = LeaseRequest {
        worker_id: worker.id,
        lease_token: second_claim.lease_token,
        lease_epoch: second_claim.lease_epoch,
        telemetry: None,
    };
    assert!(matches!(
        repository::heartbeat(&pool, third.id, &stale_lease).await,
        Err(AppError::Conflict)
    ));

    // Changer la priorité d'une tâche en attente puis la publier en succès.
    let reprioritized = repository::update_job_priority(&pool, low.id, 15).await?;
    assert_eq!(reprioritized.priority, 15);
    let high_lease = LeaseRequest {
        worker_id: worker.id,
        lease_token: first_claim.lease_token,
        lease_epoch: first_claim.lease_epoch,
        telemetry: None,
    };
    repository::publish_succeeded(
        &pool,
        high.id,
        &high_lease,
        repository::PublishedOutput {
            name: "high.hevc.mkv",
            sha256: &"44".repeat(32),
            bytes: 1_200,
            warning: false,
        },
        async { Ok(()) },
    )
    .await?;

    // Fenêtre horaire restrictive : bloque claim en dehors du créneau.
    use chrono::Timelike;
    let current_hour = chrono::Local::now().hour() as u8;
    let blocked_start = (current_hour + 2) % 24;
    let blocked_end = (current_hour + 3) % 24;
    repository::update_worker_settings(
        &pool,
        worker.id,
        2,
        Some(&forgearr::domain::WorkerSchedule {
            enabled: true,
            start_hour: blocked_start,
            end_hour: blocked_end,
            days: Vec::new(),
        }),
    )
    .await?;
    assert!(repository::claim(&pool, worker.id).await?.is_none());

    // Statistiques d'économie d'espace disque.
    let stats = repository::job_storage_stats(&pool).await?;
    assert_eq!(stats.total_jobs, 3);
    assert_eq!(stats.succeeded_jobs, 1);
    assert_eq!(stats.canceled_jobs, 1);
    assert_eq!(stats.total_source_bytes, 2_000);
    assert_eq!(stats.total_output_bytes, 1_200);
    assert_eq!(stats.saved_bytes, 800);

    // Action par lot : annulation de low + suppression de third (high ignorée car succeeded).
    let (batch_cancel, events) = repository::batch_jobs(
        &pool,
        &[low.id, high.id],
        forgearr::domain::JobBatchAction::Cancel,
    )
    .await?;
    assert_eq!(batch_cancel.updated, 1);
    assert_eq!(batch_cancel.skipped, 1);
    assert_eq!(events.len(), 1);
    let (batch_del, _) = repository::batch_jobs(
        &pool,
        &[third.id, low.id],
        forgearr::domain::JobBatchAction::Delete,
    )
    .await?;
    assert_eq!(batch_del.updated, 2);
    assert_eq!(batch_del.skipped, 0);
    Ok(())
}
