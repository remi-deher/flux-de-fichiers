use crate::{
    api::{self, AppState},
    domain::{Job, JobErrorCode, JobEventType, JobStatus, LeaseRequest, LibraryOutputPolicy},
    error::{AppError, AppResult},
    job_runner::{
        self, JobExecutionResult, JobRunError,
        action_gate::{ActionGate, TaskStop},
    },
    repository, storage,
    supervision::TaskWork,
    worker::startup_purge,
    worker::timing::{
        ABANDONED_WORK_TIMEOUT, HEARTBEAT_INTERVAL, IDLE_POLL_INTERVAL, SHUTDOWN_TIMEOUT,
        SHUTDOWN_WORK_TIMEOUT, SOFTWARE_PROBE_REFRESH,
    },
    worker_transfer::{SOURCE_INTEGRITY, TransferState},
};
use forgearr_shared::jobs::LeaseProtocol;
use forgearr_shared::transfer::{
    ArtifactKind, ResultReady, SourcePublicationOutcome, SourcePublicationReport,
    SourcePublicationTask,
};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::{sync::watch, task::JoinSet};
use uuid::Uuid;

mod lease_gate;
pub(crate) mod retention;
use lease_gate::LeaseGate;

/// Variables d'environnement du serveur de transfert du worker intégré : adresse annoncée aux autres
/// workers et port d'écoute (comme `WORKER_TRANSFER_URL` et `WORKER_TRANSFER_PORT` d'un worker distant).
const LOCAL_TRANSFER_URL_VAR: &str = "LOCAL_WORKER_TRANSFER_URL";
const LOCAL_TRANSFER_PORT_VAR: &str = "LOCAL_WORKER_TRANSFER_PORT";

/// Attente d'une demande de publication avant de redemander (le courtier réveille dès qu'une arrive).
const PUBLICATION_WAIT: Duration = Duration::from_secs(15);

async fn refresh_software_capabilities(state: &AppState) {
    let roots = crate::plugins::catalog_roots(&state.pool, &state.config).await;
    let probes = crate::software_probe::discover_software_probes(&roots);
    let cache = crate::tools_cache::ToolCacheManager::new(&state.config.tools_cache_dir);
    crate::software_probe::refresh_software_capabilities(&cache, &probes).await;
}

/// Validation des billets présentés au serveur de transfert du worker intégré : directement auprès
/// de la base du central, sans appel réseau. Il sert aussi les dossiers du central, dont le dépôt
/// d'entrée `input_dir`.
pub(crate) fn local_redeemer(
    pool: sqlx::SqlitePool,
    worker_id: Uuid,
    input_dir: std::path::PathBuf,
) -> crate::worker_transfer::Redeemer {
    std::sync::Arc::new(move |token: String| {
        let pool = pool.clone();
        let input_dir = input_dir.clone();
        Box::pin(async move {
            repository::redeem_central_ticket(&pool, &token, worker_id, &input_dir)
                .await
                .map_err(|_| axum::http::StatusCode::FORBIDDEN)
        })
    })
}

/// Annonce l'adresse de transfert du worker intégré et sert ses fichiers retenus, comme un worker
/// ordinaire ; sans adresse ni port, il n'annonce rien.
async fn start_transfer_server(state: &AppState, worker_id: Uuid) {
    let (url, port) =
        crate::worker_transfer::transfer_settings(LOCAL_TRANSFER_URL_VAR, LOCAL_TRANSFER_PORT_VAR);
    let announced = url.as_deref().filter(|_| port.is_some());
    if let Err(error) = repository::set_transfer_url(&state.pool, worker_id, announced).await {
        tracing::warn!(error = %error, "adresse de transfert du worker intégré non enregistrée");
    }
    // Il traite toujours les dossiers du central ; l'adresse annoncée permet en plus de les lire
    // chez lui depuis un autre worker.
    if let Err(error) = repository::set_serves_central(&state.pool, worker_id, true).await {
        tracing::warn!(error = %error, "rôle de serveur des dossiers du central non enregistré");
    }
    if let (Some(port), Some(_)) = (port, announced) {
        let input_dir = state.config.input_dir.clone();
        let redeemer = local_redeemer(state.pool.clone(), worker_id, input_dir);
        crate::worker_transfer::spawn_listener(port, TransferState::new(redeemer));
    }
}

fn integrated_worker_name() -> String {
    std::env::var("LOCAL_WORKER_NAME")
        .unwrap_or_else(|_| forgearr_shared::workers::INTEGRATED_WORKER_NAME.into())
}

/// Le worker intégré, déjà connu, démarre avec le central : il est en ligne avant la reprise de ses
/// tâches. Sans effet à la première installation.
pub(crate) async fn announce_start(pool: &sqlx::SqlitePool) {
    let announced = async {
        let known = repository::find_worker_by_name(pool, &integrated_worker_name()).await?;
        match known {
            Some(worker) => repository::touch_worker_telemetry(pool, worker.id, None, None).await,
            None => Ok(()),
        }
    };
    if let Err(error) = announced.await {
        tracing::warn!(%error, "démarrage du worker intégré non signalé");
    }
}

pub async fn run(state: AppState) {
    refresh_software_capabilities(&state).await;
    let worker_name = integrated_worker_name();
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
    start_transfer_server(&state, worker.id).await;
    retention::prune(&state, worker.id).await;
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
                    retention::prune(&refresh_state, refresh_worker).await;
                    let snapshot = probe.sample();
                    if let Err(error) = repository::touch_worker_telemetry(&refresh_state.pool, refresh_worker, None, Some(&snapshot)).await {
                        tracing::warn!(worker_id=%refresh_worker, error=%error, "local capability telemetry update failed");
                    }
                },
            }
        }
    });

    // Dossiers de tâches d'une exécution précédente ; ceux des flux d'échec, déjà en cours, ne le sont pas.
    startup_purge::purge_task_dirs(&state.config.data_dir.join("staging")).await;

    let (stop, shutdown) = watch::channel(false);
    // Un seul écouteur durable : une tâche occupée au moment du signal le voit quand même.
    let signal = tokio::spawn(async move {
        api::shutdown_signal().await;
        let _ = stop.send(true);
    });
    let mut active = JoinSet::new();
    claim_until_stopped(&state, &worker, &shutdown, &mut active).await;
    // Arrêt propre : chaque tâche s'arrête et se rend ; celles qui tardent sont abandonnées.
    let drained = async { while active.join_next().await.is_some() {} };
    if tokio::time::timeout(SHUTDOWN_TIMEOUT, drained)
        .await
        .is_err()
    {
        active.abort_all();
    }
    signal.abort();
}

/// Se termine quand l'arrêt est demandé, même s'il l'a été avant l'appel.
async fn stopped(mut shutdown: watch::Receiver<bool>) {
    // Émetteur disparu : le processus s'arrête aussi.
    let _ = shutdown.wait_for(|stopping| *stopping).await;
}

/// Réserve et lance des tâches jusqu'à la demande d'arrêt. Une réservation n'est jamais interrompue :
/// validée sans être lancée, elle resterait sans exécutant.
async fn claim_until_stopped(
    state: &AppState,
    worker: &crate::domain::Worker,
    shutdown: &watch::Receiver<bool>,
    active: &mut JoinSet<()>,
) {
    loop {
        while active.try_join_next().is_some() {}
        let max_concurrency = repository::get_worker(&state.pool, worker.id)
            .await
            .map(|w| w.slots().total)
            .unwrap_or(worker.slots().total);
        while (active.len() as i32) < max_concurrency && !*shutdown.borrow() {
            let claim =
                repository::claim_with_protocol(&state.pool, worker.id, LeaseProtocol::LATEST);
            match claim.await {
                Ok(Some(claim)) => {
                    let (st, wid, shutdown) = (state.clone(), worker.id, shutdown.clone());
                    active.spawn(async move { execute(&st, wid, claim, shutdown).await });
                }
                Ok(None) => break,
                Err(error) => {
                    tracing::warn!(error=%error, "local worker claim failed");
                    break;
                }
            }
        }
        tokio::select! {
            () = tokio::time::sleep(IDLE_POLL_INTERVAL) => {},
            Some(_) = active.join_next(), if !active.is_empty() => {},
            () = stopped(shutdown.clone()) => return,
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
        if task.policy == LibraryOutputPolicy::OutputDir {
            return install_in_output_dir(state, task, &access, &file).await;
        }
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
                // Demande close entre-temps (délai dépassé) : son intention n'est plus la nôtre.
                state
                    .source_publication
                    .access_for_worker(task.request_id, task.worker_id)
                    .await
                    .ok_or(AppError::NotFound)?;
                repository::require_publication_grant(&state.pool, access.job_id, &access.lease)
                    .await
            },
        )
        .await
    }
    .await;
    let _ = tokio::fs::remove_file(&file).await;
    result
}

/// Installe le résultat tiré dans le dossier de sortie du central, sous le nom de la tâche.
async fn install_in_output_dir(
    state: &AppState,
    task: &SourcePublicationTask,
    access: &crate::source_publication_broker::PublicationAccess,
    file: &std::path::Path,
) -> AppResult<()> {
    repository::active(&state.pool, access.job_id, &access.lease).await?;
    let name =
        crate::storage::output_file_name(access.job_id, access.lease.lease_token, &task.extension);
    let output_dir = std::path::Path::new(&task.root);
    tokio::fs::create_dir_all(output_dir).await?;
    let installed = output_dir.join(name);
    match tokio::fs::hard_link(file, &installed).await {
        Ok(()) => {}
        // Même bail, même nom : une installation précédente dont la réponse s'est perdue.
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            let max = state.config.max_file_bytes;
            let (sha256, bytes) = crate::storage::fingerprint(&installed, max).await?;
            let same = sha256 == task.result_sha256
                && i64::try_from(bytes).is_ok_and(|bytes| bytes == task.result_bytes);
            if !same {
                return Err(error.into());
            }
        }
        Err(error) => return Err(error.into()),
    }
    crate::storage::sync_directory(output_dir).await
}

/// Exécute une tâche réservée jusqu'à son issue, la perte de son bail ou l'arrêt du central. La tâche
/// n'est rendue, et son dossier supprimé, qu'une fois ses processus morts.
async fn execute(
    state: &AppState,
    worker_id: Uuid,
    claim: crate::domain::Claim,
    shutdown: watch::Receiver<bool>,
) {
    let job_id = claim.job_id;
    let lease = LeaseRequest {
        worker_id,
        lease_token: claim.lease_token,
        lease_epoch: claim.lease_epoch,
        telemetry: None,
    };
    let work_dir = state
        .config
        .data_dir
        .join("staging")
        .join(format!("{job_id}-{}", claim.lease_token));
    let mut work = TaskWork::default();
    // Abandonner le traitement tue le groupe de ses processus.
    let outcome = tokio::select! {
        outcome = work.scope(process_and_report(state, claim, &lease, &work_dir)) => outcome,
        () = heartbeat_until_lost(state, job_id, &lease) => Err(TaskStop::LeaseLost),
        () = stopped(shutdown) => Err(TaskStop::Shutdown),
    };
    if let Err(stop) = outcome {
        tracing::warn!(%job_id, reason=%stop, "tâche locale arrêtée sans échec déclaré");
    }
    let stopping = outcome == Err(TaskStop::Shutdown);
    let limit = if stopping {
        SHUTDOWN_WORK_TIMEOUT
    } else {
        ABANDONED_WORK_TIMEOUT
    };
    if !work.ended(limit).await {
        // Le démarrage suivant purgera et reprendra la tâche.
        tracing::warn!(%job_id, "travail local encore en cours : dossier conservé");
        return;
    }
    if stopping {
        release(state, job_id, &lease).await;
    }
    cleanup(&work_dir).await;
}

/// Rend la tâche, sauf pendant une publication que le central mène encore.
async fn release(state: &AppState, job_id: Uuid, lease: &LeaseRequest) {
    if state.publications.contains((job_id, lease.lease_epoch)) {
        return;
    }
    match repository::release_lease(&state.pool, job_id, lease).await {
        Ok(change) => api::leases::notify_lease_changes(state, &[change]),
        Err(error) => tracing::info!(%job_id, %error, "tâche locale non rendue"),
    }
}

/// Traite la tâche puis inscrit son échec éventuel. Un arrêt ([`TaskStop`]) ne s'inscrit pas.
async fn process_and_report(
    state: &AppState,
    claim: crate::domain::Claim,
    lease: &LeaseRequest,
    work_dir: &std::path::Path,
) -> Result<(), TaskStop> {
    let job_id = claim.job_id;
    let Err(error) = process(state, claim, lease, work_dir).await else {
        return Ok(());
    };
    if let Some(stop) = stop_reason(&error) {
        return Err(stop);
    }
    if let Some(run_error) = error.downcast_ref::<JobRunError>()
        && !run_error.steps.is_empty()
    {
        let steps = &run_error.steps;
        let _ = repository::save_job_steps_for_lease(&state.pool, job_id, lease, steps).await;
    }
    let code = failure_code(&error);
    report_failure(state, job_id, lease, code).await;
    tracing::warn!(%job_id, error_code=%code, "local processing failed");
    Ok(())
}

/// Arrêt sans échec à inscrire : bail perdu, central sans réponse, issue déjà inscrite.
fn stop_reason(error: &anyhow::Error) -> Option<TaskStop> {
    if let Some(stop) = error.downcast_ref::<TaskStop>() {
        return Some(*stop);
    }
    matches!(error.downcast_ref::<AppError>(), Some(AppError::LeaseLost))
        .then_some(TaskStop::LeaseLost)
}

fn failure_code(error: &anyhow::Error) -> JobErrorCode {
    if let Some(run_error) = error.downcast_ref::<JobRunError>() {
        return run_error.code;
    }
    let source_changed = matches!(
        error.downcast_ref::<AppError>(),
        Some(AppError::SourceChanged)
    );
    if source_changed {
        JobErrorCode::SourceChanged
    } else if error.to_string().contains(SOURCE_INTEGRITY) {
        JobErrorCode::SourceIntegrity
    } else {
        JobErrorCode::TransferFailed
    }
}

async fn process(
    state: &AppState,
    mut claim: crate::domain::Claim,
    lease: &LeaseRequest,
    work_dir: &std::path::Path,
) -> anyhow::Result<()> {
    let worker_id = lease.worker_id;
    let job = repository::active(&state.pool, claim.job_id, lease).await?;
    let gate = LeaseGate::new(state.clone(), claim.job_id, lease.clone());
    let gate: &dyn ActionGate = &gate;
    let prepared = crate::api::populate_claim_contracts(
        &state.pool,
        &state.config,
        &state.catalog_cache,
        worker_id,
        &mut claim,
    )
    .await;
    if let Err(error) = prepared {
        api::undeliverable_claim(state, worker_id, &claim, error).await;
        return Err(TaskStop::Concluded.into());
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
    job_runner::ensure_probe_ready(&cache, &claim)?;
    tokio::fs::create_dir(work_dir).await?;
    if let Err(error) = repository::attach_source_ticket(&state.pool, worker_id, &mut claim).await {
        tracing::warn!(job_id=%claim.job_id, error=%error, "local source ticket unavailable");
    }
    // Source du central, sinon lecture directe chez le worker qui la détient (billet, ADR 0019).
    let central =
        crate::library_scan::central_job_source(&state.pool, &state.config.input_dir, &job).await;
    let source = match central {
        Ok(Some(path)) => Some(path),
        Ok(None) if claim.source_ticket.is_some() => None,
        _ => anyhow::bail!(SOURCE_INTEGRITY),
    };
    let source_snapshot = work_dir.join(forgearr_shared::jobs::source_working_filename(
        &claim.source_name,
    ));
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
                        repository::reissue_source_ticket(&state.pool, claim.job_id, lease).await?,
                    )
                },
            )
            .await?;
        }
        (None, None) => anyhow::bail!(SOURCE_INTEGRITY),
    }

    let working_snapshot = if let Some(handoff) = &claim.handoff_input {
        let working_dir = work_dir.join("handoff");
        tokio::fs::create_dir(&working_dir).await?;
        let target = working_dir.join(forgearr_shared::jobs::source_working_filename(
            &handoff.artifact.name,
        ));
        pull_handoff_input(state, &claim, lease, handoff, &target).await?;
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
        work_dir,
        progress_cb,
        Some(gate),
    )
    .await?;

    let step_report = if execution.steps.is_empty() {
        Ok(())
    } else {
        repository::save_job_steps_for_lease(&state.pool, claim.job_id, lease, &execution.steps)
            .await
    };
    if let Err(error) =
        repository::save_flow_media_sample(&state.pool, claim.job_id, lease, &execution.media_info)
            .await
    {
        tracing::warn!(job_id = %claim.job_id, %error, "échantillon de métadonnées non retenu");
    }

    if let Some(request) = &execution.relay {
        step_report?;
        let working = (execution.working_file.as_path(), work_dir);
        return retention::relay_working_file(
            state,
            claim.job_id,
            lease,
            request,
            working,
            execution.warning,
        )
        .await;
    }
    if let Err(error) = step_report {
        tracing::warn!(job_id = %claim.job_id, error = %error, "failed to persist job steps");
    }
    let snapshot = (!pulls_working_file || source.is_some()).then_some(source_snapshot);
    publish(state, job, lease, execution, snapshot).await?;
    Ok(())
}

/// Publie le résultat hors de la tâche, inscrit comme publication en cours : abandonnée au milieu, elle
/// laisserait un remplacement partiel. Le balayage et l'arrêt propre attendent sa fin.
async fn publish(
    state: &AppState,
    job: Job,
    lease: &LeaseRequest,
    execution: JobExecutionResult,
    snapshot: Option<std::path::PathBuf>,
) -> AppResult<()> {
    let key = (job.id, lease.lease_epoch);
    let (st, lease) = (state.clone(), lease.clone());
    let token = crate::supervision::current_token();
    let publication = async move {
        // Le dossier de travail reste en place tant que la publication le lit.
        let _token = token;
        repository::renew_contact(&st.pool, job.id, &lease).await?;
        let file = execution.working_file.as_path();
        // Un autre worker détient la bibliothèque : il tire le résultat chez ce worker (ADR 0022).
        let ready = ResultReady {
            extension: execution.output_spec.extension.clone(),
            warning: execution.warning,
            can_apply_locally: false,
        };
        if retention::publish_retained(&st, &job, &lease, file, &ready).await? {
            return Ok(());
        }
        crate::api::transfers::publish_staged(
            &st,
            job.id,
            &lease,
            &job,
            snapshot.as_deref(),
            file,
            execution.warning,
            &execution.output_spec,
        )
        .await
    };
    state.publications.run(key, publication).await
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

/// Bat tant que le bail est tenu ; une base occupée n'arrête rien, seul un refus définitif rend la main.
async fn heartbeat_until_lost(state: &AppState, job: Uuid, lease: &LeaseRequest) {
    let probe = crate::telemetry::TelemetryProbe::with_storage_path(&state.config.data_dir);
    loop {
        tokio::time::sleep(HEARTBEAT_INTERVAL).await;
        let mut tick = lease.clone();
        tick.telemetry = Some(probe.sample());
        match repository::heartbeat(&state.pool, job, &tick).await {
            Ok(true) => {
                state.notify_job_event(JobEventType::JobRestored, job, JobStatus::Running, None);
            }
            Ok(false) => {}
            Err(AppError::LeaseLost) => return,
            Err(error) => tracing::warn!(job_id=%job, %error, "battement local sans réponse"),
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_lost_lease_is_a_stop_and_not_a_failure() {
        let stop = |error: AppError| stop_reason(&error.into());
        assert_eq!(stop(AppError::LeaseLost), Some(TaskStop::LeaseLost));
        assert_eq!(stop(AppError::Conflict), None);
        let concluded = TaskStop::Concluded;
        assert_eq!(stop_reason(&concluded.into()), Some(concluded));
        let failed = JobRunError::simple(JobErrorCode::OperationFailed, "échec");
        assert_eq!(stop_reason(&failed.into()), None);
    }

    #[test]
    fn failure_codes_follow_the_error() {
        let code = |error: anyhow::Error| failure_code(&error);
        let run_error = JobRunError::simple(JobErrorCode::ProbeUnavailable, "sonde");
        assert_eq!(code(run_error.into()), JobErrorCode::ProbeUnavailable);
        assert_eq!(
            code(AppError::SourceChanged.into()),
            JobErrorCode::SourceChanged
        );
        let altered = anyhow::anyhow!(SOURCE_INTEGRITY);
        assert_eq!(code(altered), JobErrorCode::SourceIntegrity);
        assert_eq!(
            code(AppError::Internal.into()),
            JobErrorCode::TransferFailed
        );
    }

    /// Borne d'un test qui doit se terminer de lui-même.
    const TEST_LIMIT: Duration = Duration::from_secs(10);

    #[tokio::test]
    async fn local_heartbeats_last_until_the_lease_is_lost() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let (state, job, lease, _) = api::transfers::tests::setup(dir.path()).await?;
        let beating = heartbeat_until_lost(&state, job.id, &lease);
        tokio::pin!(beating);
        let early = tokio::time::timeout(HEARTBEAT_INTERVAL * 5, &mut beating).await;
        assert!(early.is_err(), "bail tenu");
        repository::cancel_job(&state.pool, job.id).await?;
        tokio::time::timeout(TEST_LIMIT, beating).await?;
        Ok(())
    }

    #[tokio::test]
    async fn the_known_integrated_worker_is_online_before_its_jobs_are_taken_back()
    -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let (state, _, _, _) = api::transfers::tests::setup(dir.path()).await?;
        announce_start(&state.pool).await;
        let worker = repository::register(&state.pool, &integrated_worker_name()).await?;
        sqlx::query("UPDATE workers SET last_seen=datetime('now', '-1 day') WHERE id=?1")
            .bind(worker.id)
            .execute(&state.pool)
            .await?;
        announce_start(&state.pool).await;
        let seen = repository::get_worker(&state.pool, worker.id)
            .await?
            .last_seen;
        assert!(chrono::Utc::now() - seen < chrono::TimeDelta::minutes(1));
        Ok(())
    }
}
