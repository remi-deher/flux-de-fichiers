use crate::{
    api::{self, AppState},
    domain::{JobErrorCode, LeaseRequest},
    job_runner::{self, JobRunError},
    repository, storage,
};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use uuid::Uuid;

const IDLE_POLL_INTERVAL: Duration = Duration::from_secs(1);
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(10);

pub async fn run(state: AppState) {
    let worker_name = std::env::var("LOCAL_WORKER_NAME")
        .unwrap_or_else(|_| forgearr_shared::workers::INTEGRATED_WORKER_NAME.into());
    let probe = crate::telemetry::TelemetryProbe::with_storage_path(&state.config.data_dir);
    let initial_snapshot = probe.sample();
    let worker = match repository::register_with_metadata(
        &state.pool,
        &worker_name,
        Some(crate::version::app_version()),
        Some("local"),
        Some(&initial_snapshot),
    )
    .await
    {
        Ok(worker) => worker,
        Err(error) => {
            tracing::error!(error=%error, "local worker registration failed");
            return;
        }
    };
    tracing::info!(worker_id=%worker.id, "local worker registered");

    // Nettoyage préventif des dossiers staging orphelins au démarrage
    let staging_root = state.config.data_dir.join("staging");
    if let Ok(mut entries) = tokio::fs::read_dir(&staging_root).await {
        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            if entry.file_type().await.map(|t| t.is_dir()).unwrap_or(false) {
                tracing::info!(path = %path.display(), "purging orphaned local staging directory at startup");
                let _ = tokio::fs::remove_dir_all(&path).await;
            }
        }
    }

    let mut active = tokio::task::JoinSet::new();
    loop {
        while active.try_join_next().is_some() {}
        let max_concurrency = repository::get_worker(&state.pool, worker.id)
            .await
            .map(|w| w.max_concurrency)
            .unwrap_or(worker.max_concurrency);
        while (active.len() as i32) < max_concurrency {
            let claim = tokio::select! {
                result = repository::claim(&state.pool, worker.id) => result,
                _ = api::shutdown_signal() => {
                    active.abort_all();
                    return;
                }
            };
            match claim {
                Ok(Some(claim)) => {
                    let st = state.clone();
                    let wid = worker.id;
                    active.spawn(async move {
                        execute(&st, wid, claim).await;
                    });
                }
                Ok(None) => break,
                Err(error) => {
                    tracing::warn!(error=%error, "local worker claim failed");
                    break;
                }
            }
        }
        tokio::select! {
            _ = tokio::time::sleep(IDLE_POLL_INTERVAL) => {},
            Some(_) = active.join_next(), if !active.is_empty() => {},
            _ = api::shutdown_signal() => {
                active.abort_all();
                return;
            }
        }
    }
}

async fn execute(state: &AppState, worker_id: Uuid, mut claim: crate::domain::Claim) {
    let lease = LeaseRequest {
        worker_id,
        lease_token: claim.lease_token,
        lease_epoch: claim.lease_epoch,
        telemetry: None,
    };
    let job = match repository::active(&state.pool, claim.job_id, &lease).await {
        Ok(job) => job,
        Err(error) => {
            tracing::warn!(job_id=%claim.job_id, error=%error, "local lease is no longer active");
            return;
        }
    };
    if let Err(error) = crate::api::populate_claim_contracts(
        &state.pool,
        &state.config,
        &state.catalog_cache,
        worker_id,
        &mut claim,
    )
    .await
    {
        report_failure(state, claim.job_id, &lease, JobErrorCode::OperationFailed).await;
        tracing::warn!(job_id=%claim.job_id, error=?error, "local claim contract preparation failed");
        return;
    }
    let roots = crate::plugins::catalog_roots(&state.pool, &state.config).await;
    let cache = crate::tools_cache::ToolCacheManager::new(&state.config.tools_cache_dir);
    let wanted = job_runner::collect_wanted_tools(&claim);
    crate::plugins::ensure_tools_for_requirements(
        &roots,
        &cache,
        &wanted,
        &crate::plugins::SyncPolicy::from_env(),
    )
    .await;
    if let Err(err) = job_runner::ensure_probe_ready(&cache, &claim) {
        report_failure(state, claim.job_id, &lease, err.code).await;
        tracing::warn!(
            job_id = %claim.job_id,
            "probe contract is not ready on the local worker"
        );
        return;
    }
    let work_dir = state
        .config
        .data_dir
        .join("staging")
        .join(format!("{}-{}", claim.job_id, claim.lease_token));
    if let Err(error) = tokio::fs::create_dir(&work_dir).await {
        report_failure(state, claim.job_id, &lease, JobErrorCode::TransferFailed).await;
        tracing::warn!(job_id=%claim.job_id, error=%error, "local work directory creation failed");
        return;
    }
    let source =
        match crate::library_scan::central_job_source(&state.pool, &state.config.input_dir, &job)
            .await
        {
            Ok(Some(path)) => path,
            _ => {
                report_failure(state, claim.job_id, &lease, JobErrorCode::SourceIntegrity).await;
                cleanup(&work_dir).await;
                return;
            }
        };
    let source_snapshot = work_dir.join(forgearr_shared::jobs::source_working_filename(
        &claim.source_name,
    ));
    let mut heartbeat = tokio::spawn(heartbeat_loop(
        state.pool.clone(),
        state.config.data_dir.clone(),
        claim.job_id,
        lease.clone(),
    ));
    let processing = async {
        copy_verified_source(
            &source,
            &source_snapshot,
            state.config.max_file_bytes,
            &claim.source_sha256,
            claim.source_bytes,
        )
        .await?;

        let state_progress = state.clone();
        let job_id = claim.job_id;
        let progress_cb = std::sync::Arc::new(move |pct: u32, line: &str| {
            state_progress.notify_job_progress(job_id, pct, Some(line.to_string()));
        });
        let registry = state.catalog_cache.operations(&roots).await;

        let execution = job_runner::execute_claim_processing(
            &claim,
            &cache,
            &state.http_client,
            Some(&state.config.config_dir),
            Some(&registry),
            &source_snapshot,
            &work_dir,
            progress_cb,
        )
        .await?;

        if !execution.steps.is_empty()
            && let Err(error) =
                repository::save_job_steps(&state.pool, claim.job_id, &execution.steps).await
        {
            tracing::warn!(job_id = %claim.job_id, error = %error, "failed to persist job steps");
        }

        crate::api::transfers::publish_staged(
            state,
            claim.job_id,
            &lease,
            &job,
            Some(&source_snapshot),
            &execution.working_file,
            execution.warning,
            &execution.output_spec,
        )
        .await?;
        anyhow::Ok(())
    };
    let outcome = tokio::select! {
        result = processing => {
            heartbeat.abort();
            result
        }
        result = &mut heartbeat => match result {
            Ok(Err(error)) => Err(anyhow::anyhow!("lease renewal failed: {error}")),
            Ok(Ok(())) => Err(anyhow::anyhow!("lease renewal stopped unexpectedly")),
            Err(error) => Err(anyhow::anyhow!("lease renewal task failed: {error}")),
        }
    };
    if let Err(error) = outcome {
        let code = if let Some(run_err) = error.downcast_ref::<JobRunError>() {
            if !run_err.steps.is_empty() {
                let _ = repository::save_job_steps(&state.pool, claim.job_id, &run_err.steps).await;
            }
            run_err.code
        } else if error.to_string().contains("source_integrity") {
            JobErrorCode::SourceIntegrity
        } else {
            JobErrorCode::TransferFailed
        };
        report_failure(state, claim.job_id, &lease, code).await;
        tracing::warn!(job_id=%claim.job_id, error_code=%code, "local processing failed");
    }
    cleanup(&work_dir).await;
}

async fn heartbeat_loop(
    pool: sqlx::SqlitePool,
    data_dir: std::path::PathBuf,
    job: Uuid,
    lease: LeaseRequest,
) -> anyhow::Result<()> {
    let probe = crate::telemetry::TelemetryProbe::with_storage_path(&data_dir);
    loop {
        tokio::time::sleep(HEARTBEAT_INTERVAL).await;
        let mut tick = lease.clone();
        tick.telemetry = Some(probe.sample());
        repository::heartbeat(&pool, job, &tick).await?;
    }
}

pub(crate) async fn copy_verified_source(
    source: &std::path::Path,
    snapshot: &std::path::Path,
    max_bytes: u64,
    expected_hash: &str,
    expected_bytes: i64,
) -> anyhow::Result<()> {
    let source_file = tokio::fs::File::open(source).await?;
    let mut snapshot_file = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(snapshot)
        .await?;
    let copied = tokio::io::copy(
        &mut source_file.take(max_bytes.saturating_add(1)),
        &mut snapshot_file,
    )
    .await?;
    anyhow::ensure!(copied <= max_bytes, "source_integrity");
    snapshot_file.flush().await?;
    snapshot_file.sync_all().await?;
    drop(snapshot_file);
    let (actual_hash, actual_bytes) = storage::fingerprint(snapshot, max_bytes).await?;
    anyhow::ensure!(
        actual_hash == expected_hash && actual_bytes as i64 == expected_bytes,
        "source_integrity"
    );
    Ok(())
}

async fn report_failure(state: &AppState, job: Uuid, lease: &LeaseRequest, code: JobErrorCode) {
    if let Err(error) = repository::fail(&state.pool, job, lease, code).await {
        tracing::warn!(job_id=%job, error=%error, "local failure report rejected");
    }
}

async fn cleanup(path: &std::path::Path) {
    if let Err(error) = tokio::fs::remove_dir_all(path).await
        && error.kind() != std::io::ErrorKind::NotFound
    {
        tracing::warn!(error=%error, "local work directory cleanup failed");
    }
}
