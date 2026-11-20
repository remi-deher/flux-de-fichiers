use super::*;
use crate::{
    config::Config,
    domain::{LibraryAccess, LibraryOutputPolicy, LibraryWorkerPath, LibraryWrite},
};
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
async fn a_source_held_by_another_worker_is_refused_before_any_change() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let (state, job, lease, _) = setup(dir.path()).await?;
    let staging = dir.path().join("staging.mkv");
    std::fs::write(&staging, b"result")?;
    let spec = media::OutputSpec {
        extension: "mkv".into(),
        ..Default::default()
    };
    let refused = publish_staged(&state, job.id, &lease, &job, None, &staging, false, &spec).await;
    assert!(matches!(refused, Err(AppError::ResultUndeliverable)));
    let copies = std::fs::read_dir(state.config.data_dir.join("output"))?.count();
    assert_eq!(copies, 0, "aucune copie centrale");
    let journal = repository::pending_source_publications(&state.pool).await?;
    assert!(journal.is_empty());
    assert_eq!(std::fs::read(dir.path().join("film.mkv"))?, b"original");
    let running = repository::get_job(&state.pool, job.id).await?;
    assert_eq!(running.status, JobStatus::Running);
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
