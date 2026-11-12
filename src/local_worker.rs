use crate::{
    api::{self, AppState},
    domain::{JobErrorCode, LeaseRequest},
    media, repository, storage,
};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use uuid::Uuid;

const IDLE_POLL_INTERVAL: Duration = Duration::from_secs(1);
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(20);

pub async fn run(state: AppState) {
    let worker_name =
        std::env::var("LOCAL_WORKER_NAME").unwrap_or_else(|_| "coordinator-local".into());
    let worker = match repository::register(&state.pool, &worker_name).await {
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

    loop {
        let claim = tokio::select! {
            result = repository::claim(&state.pool, worker.id) => result,
            _ = api::shutdown_signal() => return,
        };
        match claim {
            Ok(Some(claim)) => execute(&state, worker.id, claim).await,
            Ok(None) => {}
            Err(error) => tracing::warn!(error=%error, "local worker claim failed"),
        }
        tokio::select! {
            _ = tokio::time::sleep(IDLE_POLL_INTERVAL) => {},
            _ = api::shutdown_signal() => return,
        }
    }
}

async fn execute(state: &AppState, worker_id: Uuid, claim: crate::domain::Claim) {
    let lease = LeaseRequest {
        worker_id,
        lease_token: claim.lease_token,
        telemetry: None,
    };
    let job = match repository::active(&state.pool, claim.job_id, &lease).await {
        Ok(job) => job,
        Err(error) => {
            tracing::warn!(job_id=%claim.job_id, error=%error, "local lease is no longer active");
            return;
        }
    };
    let roots = crate::plugins::catalog_roots(&state.pool, &state.config).await;
    let catalog = crate::plugins::list_catalog(&roots);
    let probe = crate::plugins::required_probe(&catalog);
    let cache = crate::tools_cache::ToolCacheManager::new(&state.config.tools_cache_dir);
    crate::plugins::ensure_job_tools(
        &roots,
        &cache,
        probe.as_ref(),
        &crate::plugins::SyncPolicy::from_env(),
    )
    .await;
    if !crate::plugins::file_job_may_start(&cache, probe.as_ref()) {
        report_failure(state, claim.job_id, &lease, JobErrorCode::ProbeUnavailable).await;
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
    let source = match storage::input_path(&state.config.input_dir, &claim.source_name).await {
        Ok(path) => path,
        Err(_) => {
            report_failure(state, claim.job_id, &lease, JobErrorCode::SourceIntegrity).await;
            cleanup(&work_dir).await;
            return;
        }
    };
    let output = work_dir.join("output.mkv");
    let source_snapshot = work_dir.join("source.mkv");
    let mut heartbeat = tokio::spawn(heartbeat_loop(
        state.pool.clone(),
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
        let cache_mgr = crate::tools_cache::ToolCacheManager::new(&state.config.tools_cache_dir);
        let default_flow = repository::default_flow(&state.pool).await?;

        let state_progress = state.clone();
        let job_id = claim.job_id;
        let progress_cb = std::sync::Arc::new(move |pct: u32, line: &str| {
            state_progress.notify_job_progress(job_id, pct, Some(line.to_string()));
        });

        let http_client = reqwest::Client::new();
        let (final_output, warning) = if let Some(flow) = default_flow {
            let arr = crate::flows::ArrContext::load(
                &state.pool,
                &state.config.config_dir,
                &state.config.input_dir,
            )
            .await;
            let engine = crate::flows::FlowEngine::new(&cache_mgr, &http_client)
                .with_secrets_dir(&state.config.config_dir)
                .with_arr(&arr);
            let outcome = engine
                .execute_flow(&flow, job_id, &source_snapshot, &work_dir, progress_cb)
                .await
                .map_err(|error| anyhow::anyhow!("flow_execution_failed: {error}"))?;
            (outcome.working_file, outcome.warning)
        } else {
            let mkvmerge_bin = cache_mgr.resolve_executable(
                crate::plugins::REMUX_TOOL_ID,
                crate::plugins::REMUX_TOOL_VERSION,
                "mkvmerge",
                "MKVMERGE",
            );
            let warning = media::remux_with_progress(
                &mkvmerge_bin,
                &source_snapshot,
                &output,
                move |pct, line| {
                    progress_cb(pct, line);
                },
            )
            .await
            .map_err(|error| anyhow::anyhow!("remux_failed: {error}"))?;
            (output, warning)
        };

        crate::api::transfers::publish_staged(
            state,
            claim.job_id,
            &lease,
            &job,
            &source_snapshot,
            &final_output,
            warning,
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
        let code = if error.to_string().contains("probe_unavailable") {
            JobErrorCode::ProbeUnavailable
        } else if error.to_string().contains("source_integrity") {
            JobErrorCode::SourceIntegrity
        } else if error.to_string().contains("remux_failed")
            || error.to_string().contains("flow_execution_failed")
        {
            JobErrorCode::RemuxFailed
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
    job: Uuid,
    lease: LeaseRequest,
) -> anyhow::Result<()> {
    loop {
        tokio::time::sleep(HEARTBEAT_INTERVAL).await;
        repository::heartbeat(&pool, job, &lease).await?;
    }
}

async fn copy_verified_source(
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
    let copied = tokio::io::copy(&mut source_file.take(max_bytes + 1), &mut snapshot_file).await?;
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
