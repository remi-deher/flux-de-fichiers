use crate::{
    api::{self, AppState},
    domain::{JobErrorCode, JobEventType, JobStatus, LeaseRequest},
    job_runner::{self, JobRunError},
    repository, storage,
};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use uuid::Uuid;

const IDLE_POLL_INTERVAL: Duration = Duration::from_secs(1);
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(10);
const SOFTWARE_PROBE_REFRESH: Duration = Duration::from_secs(60);
const RECEIVED_HANDOFF_DIRECTORY: &str = "received-handoffs";

async fn refresh_software_capabilities(state: &AppState) {
    let roots = crate::plugins::catalog_roots(&state.pool, &state.config).await;
    let probes = crate::software_probe::discover_software_probes(&roots);
    let cache = crate::tools_cache::ToolCacheManager::new(&state.config.tools_cache_dir);
    crate::software_probe::refresh_software_capabilities(&cache, &probes).await;
}

pub async fn run(state: AppState) {
    refresh_software_capabilities(&state).await;
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
    let refresh_state = state.clone();
    let refresh_worker = worker.id;
    tokio::spawn(async move {
        let probe =
            crate::telemetry::TelemetryProbe::with_storage_path(&refresh_state.config.data_dir);
        loop {
            tokio::select! {
                _ = api::shutdown_signal() => return,
                _ = tokio::time::sleep(SOFTWARE_PROBE_REFRESH) => {
                    refresh_software_capabilities(&refresh_state).await;
                    let snapshot = probe.sample();
                    if let Err(error) = repository::touch_worker_telemetry(&refresh_state.pool, refresh_worker, None, Some(&snapshot)).await {
                        tracing::warn!(worker_id=%refresh_worker, error=%error, "local capability telemetry update failed");
                    }
                    let pool = refresh_state.pool.clone();
                    if let Err(error) = crate::worker::prune_retained(&refresh_state.config.data_dir.join(RECEIVED_HANDOFF_DIRECTORY), |job, epoch| {
                        let pool = pool.clone();
                        async move { Ok(repository::retained_handoff_needed(&pool, job, refresh_worker, epoch).await?) }
                    }).await {
                        tracing::warn!(error=%error, "local retained handoff cleanup failed");
                    }
                },
            }
        }
    });

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
            match receive_handoff(&state, worker.id).await {
                Ok(true) => {}
                Ok(false) => {}
                Err(error) => tracing::warn!(error=%error, "local handoff reception failed"),
            }
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

/// Le central vérifie sa copie locale avant que le worker intégré accuse réception.
async fn receive_handoff(state: &AppState, worker: Uuid) -> anyhow::Result<bool> {
    let Some(offer) = repository::claim_handoff(&state.pool, worker).await? else {
        return Ok(false);
    };
    let staging = state.config.data_dir.join("staging");
    let source = crate::api::handoffs::artifact_path(state, offer.job_id, offer.source_lease_epoch);
    let received_root = state.config.data_dir.join(RECEIVED_HANDOFF_DIRECTORY);
    tokio::fs::create_dir_all(&received_root).await?;
    let retained = crate::worker::retained_handoff_path(
        &received_root,
        offer.job_id,
        offer.source_lease_epoch,
    );
    let temp = staging.join(format!(".tmp-handoff-{}", Uuid::new_v4()));
    let result = async {
        copy_verified_source(&source, &temp, state.config.max_file_bytes, &offer.artifact.sha256, offer.artifact.bytes).await?;
        tokio::fs::hard_link(&temp, &retained).await?;
        storage::sync_directory(&received_root).await?;
        let ack = forgearr_shared::relay::HandoffAcknowledgement { worker_id: worker, receipt_token: offer.receipt_token,
            sha256: offer.artifact.sha256, bytes: offer.artifact.bytes };
        if repository::acknowledge_handoff(&state.pool, offer.job_id, &ack).await? {
            state.notify_job_event(JobEventType::JobRelayed, offer.job_id, JobStatus::Queued, None);
            if let Err(error) = tokio::fs::remove_file(&source).await
                && error.kind() != std::io::ErrorKind::NotFound
            {
                tracing::warn!(job_id=%offer.job_id, error=%error, "handoff staging cleanup failed");
            }
        }
        Ok::<(), anyhow::Error>(())
    }.await;
    let _ = tokio::fs::remove_file(temp).await;
    result?;
    Ok(true)
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
    refresh_software_capabilities(state).await;
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
    if let Err(error) = repository::attach_source_ticket(&state.pool, worker_id, &mut claim).await {
        tracing::warn!(job_id=%claim.job_id, error=%error, "local source ticket unavailable");
    }
    // Source du central, sinon lecture directe chez le worker qui la détient (billet, ADR 0019).
    let source =
        match crate::library_scan::central_job_source(&state.pool, &state.config.input_dir, &job)
            .await
        {
            Ok(Some(path)) => Some(path),
            Ok(None) if claim.source_ticket.is_some() => None,
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
        match (&source, claim.source_ticket.as_ref()) {
            (Some(source), _) => {
                copy_verified_source(
                    source,
                    &source_snapshot,
                    state.config.max_file_bytes,
                    &claim.source_sha256,
                    claim.source_bytes,
                )
                .await?;
            }
            (None, Some(ticket)) => {
                crate::worker_transfer::download_direct_resuming(
                    &crate::worker_transfer::direct_client()?,
                    ticket.clone(),
                    &source_snapshot,
                    state.config.max_file_bytes,
                    &claim.source_sha256,
                    claim.source_bytes,
                    || async {
                        anyhow::Ok(
                            repository::reissue_source_ticket(&state.pool, claim.job_id, &lease)
                                .await?,
                        )
                    },
                )
                .await?;
            }
            (None, None) => anyhow::bail!("source_integrity"),
        }

        let working_snapshot = if let Some(handoff) = &claim.handoff_input {
            let retained = crate::worker::retained_handoff_path(
                &state.config.data_dir.join(RECEIVED_HANDOFF_DIRECTORY),
                claim.job_id,
                handoff.source_lease_epoch,
            );
            let working_dir = work_dir.join("handoff");
            tokio::fs::create_dir(&working_dir).await?;
            let target = working_dir.join(forgearr_shared::jobs::source_working_filename(
                &handoff.artifact.name,
            ));
            copy_verified_source(
                &retained,
                &target,
                state.config.max_file_bytes,
                &handoff.artifact.sha256,
                handoff.artifact.bytes,
            )
            .await?;
            tokio::fs::remove_file(retained).await?;
            target
        } else {
            source_snapshot.clone()
        };

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
            &working_snapshot,
            &work_dir,
            progress_cb,
        )
        .await?;

        let step_report = if !execution.steps.is_empty() {
            repository::save_job_steps_for_lease(
                &state.pool,
                claim.job_id,
                &lease,
                &execution.steps,
            )
            .await
        } else {
            Ok(())
        };

        if let Some(request) = execution.relay.as_ref() {
            step_report?;
            send_handoff(
                state,
                claim.job_id,
                &lease,
                request,
                &execution.working_file,
                &work_dir,
                execution.warning,
            )
            .await?;
            return anyhow::Ok(());
        }
        if let Err(error) = step_report {
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
                let _ = repository::save_job_steps_for_lease(
                    &state.pool,
                    claim.job_id,
                    &lease,
                    &run_err.steps,
                )
                .await;
            }
            run_err.code
        } else if matches!(
            error.downcast_ref::<crate::error::AppError>(),
            Some(crate::error::AppError::SourceChanged)
        ) {
            JobErrorCode::SourceChanged
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

/// Conserve le dossier du traitement intégré jusqu'à la réception vérifiée.
async fn send_handoff(
    state: &AppState,
    job: Uuid,
    lease: &LeaseRequest,
    request: &forgearr_shared::relay::RelayRequest,
    working_file: &std::path::Path,
    work_dir: &std::path::Path,
    warning: bool,
) -> anyhow::Result<()> {
    use forgearr_shared::relay::{HandoffArtifact, HandoffState};
    let working_file = storage::verified_working_path(working_file, work_dir).await?;
    let (sha256, bytes) = storage::fingerprint(&working_file, state.config.max_file_bytes).await?;
    let artifact = HandoffArtifact {
        name: working_file
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or_else(|| anyhow::anyhow!("invalid handoff filename"))?
            .to_owned(),
        sha256,
        bytes: i64::try_from(bytes)?,
    };
    crate::api::handoffs::begin_or_fail(state, job, lease, request, &artifact, warning).await?;
    let staged = crate::api::handoffs::artifact_path(state, job, lease.lease_epoch);
    copy_verified_source(
        &working_file,
        &staged,
        state.config.max_file_bytes,
        &artifact.sha256,
        artifact.bytes,
    )
    .await?;
    storage::sync_directory(&state.config.data_dir.join("staging")).await?;
    if let Err(error) = repository::finish_handoff_upload(&state.pool, job, lease).await {
        let _ = tokio::fs::remove_file(staged).await;
        return Err(error.into());
    }
    let deadline = tokio::time::Instant::now()
        + Duration::from_secs(forgearr_shared::relay::HANDOFF_TTL_SECONDS as u64);
    loop {
        repository::expire_handoffs(&state.pool).await?;
        match repository::handoff_state(&state.pool, job, lease.worker_id, lease.lease_epoch)
            .await?
        {
            HandoffState::Acknowledged => return Ok(()),
            HandoffState::Failed => anyhow::bail!("handoff failed"),
            HandoffState::Uploading | HandoffState::Pending | HandoffState::Claimed => {}
        }
        anyhow::ensure!(tokio::time::Instant::now() < deadline, "handoff timed out");
        tokio::time::sleep(IDLE_POLL_INTERVAL).await;
    }
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
