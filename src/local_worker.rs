use crate::{
    api::{self, AppState},
    domain::{JobErrorCode, LeaseRequest},
    error::{AppError, AppResult},
    job_runner::{self, JobRunError},
    repository, storage,
    worker::timing::{HEARTBEAT_INTERVAL, IDLE_POLL_INTERVAL, SOFTWARE_PROBE_REFRESH},
};
use forgearr_shared::transfer::{
    ArtifactKind, SourcePublicationOutcome, SourcePublicationReport, SourcePublicationTask,
};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use uuid::Uuid;

/// Attente d'une demande de publication avant de redemander (le courtier réveille dès qu'une arrive).
const PUBLICATION_WAIT: Duration = Duration::from_secs(15);

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
    let _ = state.local_worker_id.set(worker.id);
    tokio::spawn(respond_to_publications(state.clone(), worker.id));
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

/// Le worker intégré détient les sources que le central voit : il reçoit les résultats à appliquer,
/// les tire chez le worker qui les retient et applique la politique de sortie (ADR 0022).
pub(crate) async fn respond_to_publications(state: AppState, worker_id: Uuid) {
    loop {
        let tasks = tokio::select! {
            _ = api::shutdown_signal() => return,
            tasks = state.source_publication.take_for_worker(worker_id, PUBLICATION_WAIT) => tasks,
        };
        for task in tasks {
            let state = state.clone();
            tokio::spawn(async move { publish_local(&state, task).await });
        }
    }
}

async fn publish_local(state: &AppState, task: SourcePublicationTask) {
    let outcome = match apply_pulled_result(state, &task).await {
        Ok(()) => SourcePublicationOutcome::Published,
        Err(AppError::SourceChanged) => SourcePublicationOutcome::SourceChanged,
        Err(error) => {
            tracing::warn!(error = %error, "publication locale du résultat impossible");
            SourcePublicationOutcome::Failed
        }
    };
    let report = SourcePublicationReport {
        request_id: task.request_id,
        worker_id: task.worker_id,
        outcome,
    };
    if !state.source_publication.report(report).await {
        tracing::warn!(request_id = %task.request_id, "réponse de publication locale refusée");
    }
}

/// Tire le résultat chez le worker qui le retient, le vérifie, puis applique la politique de sortie
/// sur la source du central. Le fichier tiré est temporaire.
async fn apply_pulled_result(state: &AppState, task: &SourcePublicationTask) -> AppResult<()> {
    let access = state
        .source_publication
        .access_for_worker(task.request_id, task.worker_id)
        .await
        .ok_or(AppError::NotFound)?;
    let ticket = task.result_ticket.clone().ok_or(AppError::Invalid)?;
    let name = format!(".source-publication-{}.tmp", task.request_id);
    let file = state.config.data_dir.join("staging").join(name);
    let max = state.config.max_file_bytes;
    let result = async {
        crate::worker_transfer::download_direct_resuming(
            &crate::worker_transfer::direct_client().map_err(|_| AppError::CentralUnavailable)?,
            ticket,
            &file,
            max,
            &task.result_sha256,
            task.result_bytes,
            || async {
                let kind = ArtifactKind::Result;
                let epoch = access.lease.lease_epoch;
                let fresh = repository::issue_artifact_ticket(
                    &state.pool,
                    access.job_id,
                    kind,
                    task.worker_id,
                    epoch,
                )
                .await?;
                anyhow::Ok(fresh)
            },
        )
        .await
        .map_err(|_| AppError::Invalid)?;
        let root = std::path::Path::new(&task.root);
        let source = crate::library_scan::source_path(root, &task.source_name)
            .await
            .map_err(crate::source_publication::missing_source_is_changed)?;
        crate::source_publication::apply_library_output_policy(
            task.policy,
            &source,
            &file,
            &task.extension,
            &task.source_sha256,
            task.source_bytes,
            max,
            async {
                let active = repository::active(&state.pool, access.job_id, &access.lease).await;
                active.map(|_| ())
            },
        )
        .await
    }
    .await;
    let _ = tokio::fs::remove_file(&file).await;
    result
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
        let pulls_working_file = claim.handoff_input.is_some();
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
            // Le billet désigne le fichier de travail : la source se vérifie chez son détenteur.
            (None, _) if pulls_working_file => {}
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
            let working_dir = work_dir.join("handoff");
            tokio::fs::create_dir(&working_dir).await?;
            let target = working_dir.join(forgearr_shared::jobs::source_working_filename(
                &handoff.artifact.name,
            ));
            pull_handoff_input(state, &claim, &lease, handoff, &target).await?;
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

        if execution.relay.is_some() {
            step_report?;
            // Le worker intégré ne sert pas encore de fichiers (ADR 0022, étape 5) : il ne peut pas
            // retenir un fichier de travail pour un repreneur.
            anyhow::bail!("local worker cannot retain working files yet");
        }
        if let Err(error) = step_report {
            tracing::warn!(job_id = %claim.job_id, error = %error, "failed to persist job steps");
        }

        crate::api::transfers::publish_staged(
            state,
            claim.job_id,
            &lease,
            &job,
            (!pulls_working_file || source.is_some()).then_some(source_snapshot.as_path()),
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

/// Tire le fichier de travail chez son détenteur, le vérifie, puis confirme la réception au central.
async fn pull_handoff_input(
    state: &AppState,
    claim: &crate::domain::Claim,
    lease: &LeaseRequest,
    input: &forgearr_shared::relay::HandoffInput,
    dest: &std::path::Path,
) -> anyhow::Result<()> {
    let ticket = claim
        .source_ticket
        .clone()
        .ok_or_else(|| anyhow::anyhow!("no ticket to pull the working file"))?;
    crate::worker_transfer::download_direct_resuming(
        &crate::worker_transfer::direct_client()?,
        ticket,
        dest,
        state.config.max_file_bytes,
        &input.artifact.sha256,
        input.artifact.bytes,
        || async {
            anyhow::Ok(repository::reissue_source_ticket(&state.pool, claim.job_id, lease).await?)
        },
    )
    .await?;
    repository::confirm_artifact_received(&state.pool, claim.job_id, lease).await?;
    Ok(())
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
