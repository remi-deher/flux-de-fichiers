use super::*;
use crate::{
    config::Config,
    domain::{LibraryAccess, LibraryOutputPolicy, LibraryWorkerPath, LibraryWrite},
    source_publication::apply_library_output_policy,
};
use forgearr_shared::transfer::{SourcePublicationOutcome, SourcePublicationReport};
use sqlx::sqlite::SqlitePoolOptions;
use std::{path::Path, sync::Arc, time::Duration};

pub(in crate::api) async fn setup(
    root: &Path,
) -> anyhow::Result<(AppState, Job, LeaseRequest, Uuid)> {
    let data = root.join("data");
    let input = root.join("input");
    let config_dir = root.join("config");
    for dir in [&data.join("output"), &input, &config_dir] {
        std::fs::create_dir_all(dir)?;
    }
    let config = Config {
        bind: "127.0.0.1:3000".parse()?,
        database_url: "sqlite::memory:".into(),
        data_dir: data,
        input_dir: input,
        config_dir: config_dir.clone(),
        tools_cache_dir: config_dir.join("tools"),
        plugins_dir: None,
        static_dir: root.join("static"),
        max_file_bytes: 1024 * 1024,
        local_worker: false,
        web_origin: None,
        web_session_ttl_seconds: 28_800,
        tls: None,
        smtp: None,
    };
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&config.database_url)
        .await?;
    sqlx::migrate!().run(&pool).await?;
    let holder = repository::register(&pool, "holder").await?;
    let compute = repository::register(&pool, "compute").await?;
    let library = repository::insert_library(
        &pool,
        LibraryWrite {
            name: "Films".into(),
            workers: vec![LibraryWorkerPath {
                worker_id: holder.id,
                source_key: Some("films".into()),
                local_path: root.to_string_lossy().into_owned(),
                access: LibraryAccess::ReadWrite,
            }],
            output_policy: LibraryOutputPolicy::ReplaceWithBackup,
            ..Default::default()
        },
    )
    .await?;
    let source = root.join("film.mkv");
    std::fs::write(&source, b"original")?;
    let (hash, bytes) = storage::fingerprint(&source, config.max_file_bytes).await?;
    let job = repository::insert_job_with_options(
        &pool,
        "film.mkv",
        None,
        &hash,
        bytes as i64,
        Some(library.id),
        None,
    )
    .await?;
    let claim = repository::claim(&pool, compute.id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("bail manquant"))?;
    sqlx::query("UPDATE jobs SET source_key='films', source_root=?1, source_worker_id=?2, source_writable=1 WHERE id=?3").bind(root.to_string_lossy().as_ref()).bind(holder.id).bind(job.id).execute(&pool).await?;
    sqlx::query(
        "INSERT INTO job_source_mounts(job_id,worker_id,local_path,writable) VALUES(?1,?2,?3,1)",
    )
    .bind(job.id)
    .bind(holder.id)
    .bind(root.to_string_lossy().as_ref())
    .execute(&pool)
    .await?;
    let job = repository::get_job(&pool, job.id).await?;
    let lease = LeaseRequest {
        worker_id: compute.id,
        lease_token: claim.lease_token,
        lease_epoch: claim.lease_epoch,
        telemetry: None,
    };
    Ok((AppState::new(pool, Arc::new(config)), job, lease, holder.id))
}

#[tokio::test]
async fn remote_replacement_waits_for_holder_and_refuses_changed_source() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let (state, job, lease, holder) = setup(dir.path()).await?;
    let staging = dir.path().join("staging.mkv");
    std::fs::write(&staging, b"result")?;
    std::fs::write(dir.path().join("film.mkv"), b"modified")?;
    let submit_state = state.clone();
    let submit_job = job.clone();
    let submit_lease = lease.clone();
    let submit_staging = staging.clone();
    let publishing = tokio::spawn(async move {
        publish_staged(
            &submit_state,
            submit_job.id,
            &submit_lease,
            &submit_job,
            None,
            &submit_staging,
            false,
            &media::OutputSpec {
                extension: "mkv".into(),
                ..Default::default()
            },
        )
        .await
    });
    let tasks = state
        .source_publication
        .take_for_worker(holder, Duration::from_secs(5))
        .await;
    assert_eq!(tasks.len(), 1);
    let task = &tasks[0];
    let source = dir.path().join(&task.source_name);
    let replacement = apply_library_output_policy(
        task.policy,
        &source,
        &staging,
        &task.extension,
        &task.source_sha256,
        task.source_bytes,
        state.config.max_file_bytes,
        async { Ok(()) },
    )
    .await;
    assert!(matches!(replacement, Err(AppError::SourceChanged)));
    assert!(
        state
            .source_publication
            .report(SourcePublicationReport {
                request_id: task.request_id,
                worker_id: holder,
                outcome: SourcePublicationOutcome::SourceChanged
            })
            .await
    );
    assert!(matches!(publishing.await?, Err(AppError::SourceChanged)));
    assert!(
        !state
            .config
            .data_dir
            .join("output")
            .join(format!("{}-{}.mkv", job.id, lease.lease_token))
            .exists()
    );
    assert_eq!(std::fs::read(source)?, b"modified");
    assert_eq!(
        repository::get_job(&state.pool, job.id).await?.status,
        JobStatus::Running
    );
    repository::fail(
        &state.pool,
        job.id,
        &lease,
        crate::domain::JobErrorCode::SourceChanged,
    )
    .await?;
    let failed = repository::get_job(&state.pool, job.id).await?;
    assert_eq!(failed.status, JobStatus::Failed);
    assert_eq!(
        failed.error_code,
        Some(crate::domain::JobErrorCode::SourceChanged)
    );
    Ok(())
}

#[tokio::test]
async fn unavailable_output_storage_does_not_request_a_source_replacement() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let (state, job, lease, holder) = setup(dir.path()).await?;
    let staging = dir.path().join("staging.mkv");
    std::fs::write(&staging, b"result")?;
    std::fs::remove_dir(state.config.data_dir.join("output"))?;
    assert!(
        publish_staged(
            &state,
            job.id,
            &lease,
            &job,
            None,
            &staging,
            false,
            &media::OutputSpec {
                extension: "mkv".into(),
                ..Default::default()
            }
        )
        .await
        .is_err()
    );
    assert!(
        state
            .source_publication
            .take_for_worker(holder, Duration::ZERO)
            .await
            .is_empty()
    );
    assert_eq!(std::fs::read(dir.path().join("film.mkv"))?, b"original");
    assert!(!dir.path().join("film.mkv.bak").exists());
    Ok(())
}

#[tokio::test]
async fn remote_replacement_succeeds_only_after_holder_confirms() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let (state, job, lease, holder) = setup(dir.path()).await?;
    // La source choisie pour le job peut être une vue centrale en lecture seule ;
    // un autre montage vérifié reste inscriptible chez le détenteur distant.
    sqlx::query("UPDATE jobs SET source_worker_id=NULL,source_writable=0 WHERE id=?1")
        .bind(job.id)
        .execute(&state.pool)
        .await?;
    let staging = dir.path().join("staging.mkv");
    std::fs::write(&staging, b"result")?;
    let submit_state = state.clone();
    let submit_job = job.clone();
    let submit_lease = lease.clone();
    let submit_staging = staging.clone();
    let publishing = tokio::spawn(async move {
        publish_staged(
            &submit_state,
            submit_job.id,
            &submit_lease,
            &submit_job,
            None,
            &submit_staging,
            false,
            &media::OutputSpec {
                extension: "mkv".into(),
                ..Default::default()
            },
        )
        .await
    });
    let tasks = state
        .source_publication
        .take_for_worker(holder, Duration::from_secs(5))
        .await;
    assert_eq!(tasks.len(), 1);
    assert_eq!(
        repository::get_job(&state.pool, job.id).await?.status,
        JobStatus::Running
    );
    let task = &tasks[0];
    let source = dir.path().join(&task.source_name);
    apply_library_output_policy(
        task.policy,
        &source,
        &staging,
        &task.extension,
        &task.source_sha256,
        task.source_bytes,
        state.config.max_file_bytes,
        async { Ok(()) },
    )
    .await?;
    assert!(
        state
            .source_publication
            .report(SourcePublicationReport {
                request_id: task.request_id,
                worker_id: holder,
                outcome: SourcePublicationOutcome::Published
            })
            .await
    );
    publishing.await??;
    assert_eq!(std::fs::read(source)?, b"result");
    assert_eq!(std::fs::read(dir.path().join("film.mkv.bak"))?, b"original");
    assert_eq!(
        repository::get_job(&state.pool, job.id).await?.status,
        JobStatus::Succeeded
    );
    Ok(())
}
