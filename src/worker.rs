use crate::{
    central_client::CentralClient,
    config,
    domain::{Claim, ClaimRequest, JobErrorCode, ProgressReport, Register, Worker},
    job_runner::{self, JobRunError, action_gate::TaskStop},
    supervision::TaskWork,
    worker_http::{WorkerState, WorkerWeb},
    worker_settings, worker_transfer,
};
use forgearr_shared::jobs::LeaseProtocol;
use reqwest::{Client, RequestBuilder};
use std::{
    env,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
    },
    time::Duration,
};
use tokio::{sync::watch, task::JoinSet};
use uuid::Uuid;

const REMOTE_BROWSE_WAIT_MS: u64 = 10_000;
const REMOTE_BROWSE_HTTP_TIMEOUT: Duration = Duration::from_secs(20);
const REMOTE_BROWSE_REPORT_TIMEOUT: Duration = Duration::from_secs(10);
const REMOTE_BROWSE_RETRY_DELAY: Duration = Duration::from_secs(2);
mod contact;
mod handoff;
mod held_tasks;
mod lease_calls;
mod local_apply;
mod result_pull;
pub(crate) mod startup_purge;
pub(crate) mod timing;
use contact::CentralReply;
use handoff::TaskResult;
use held_tasks::HeldTasks;
use lease_calls::{CallFailure, TaskLease};
use timing::{
    ABANDONED_WORK_TIMEOUT, CENTRAL_CONNECT_TIMEOUT, CENTRAL_REQUEST_TIMEOUT, IDLE_POLL_INTERVAL,
    PROGRESS_REPORT_TIMEOUT, PUBLICATION_REPORT_PATIENCE, REGISTRATION_RETRY_DELAY,
    REREGISTRATION_EVERY_IDLE_POLLS, SHUTDOWN_TIMEOUT, SHUTDOWN_WORK_TIMEOUT,
    SOFTWARE_PROBE_REFRESH, TOOL_DISTRIBUTION_TIMEOUT,
};

#[derive(Clone, Copy)]
enum PublicationRequestKind {
    Publish,
    Inspect,
}

impl PublicationRequestKind {
    fn route(self) -> &'static str {
        match self {
            Self::Publish => "/api/v1/worker/source-publications",
            Self::Inspect => "/api/v1/worker/source-publication-inspections",
        }
    }
}

#[derive(Clone)]
pub struct WorkerConfig {
    pub url: reqwest::Url,
    pub key: String,
    pub name: String,
    pub work_dir: PathBuf,
    pub tools_cache_dir: PathBuf,
    pub ca: Option<PathBuf>,
    pub max_bytes: u64,
    /// Adresse annoncée du serveur de transfert et port d'écoute (ADR 0019), tous deux facultatifs.
    pub transfer_url: Option<String>,
    pub transfer_port: Option<u16>,
}

/// Lit `WORKER_TRANSFER_URL` (adresse annoncée) et `WORKER_TRANSFER_PORT` (écoute, sinon le port de l'adresse).
fn transfer_settings() -> (Option<String>, Option<u16>) {
    worker_transfer::transfer_settings("WORKER_TRANSFER_URL", "WORKER_TRANSFER_PORT")
}

impl WorkerConfig {
    /// Adresse du central, nom et clé viennent de `/config`, écrits par l'enrôlement (ADR 0016).
    pub fn load(config_dir: &Path) -> anyhow::Result<Self> {
        let work_dir: PathBuf = env::var("WORK_DIR")
            .unwrap_or_else(|_| "var/work".into())
            .into();
        let tools_cache_dir = env::var("TOOLS_CACHE_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| work_dir.join("tools-cache"));
        let enrolled = worker_settings::load_worker(config_dir)?.ok_or_else(|| {
            anyhow::anyhow!(
                "worker non enrôlé : aucun rôle dans {}",
                config_dir.display()
            )
        })?;
        let (transfer_url, transfer_port) = transfer_settings();
        Ok(Self {
            url: config::central_url(&enrolled.central_url)?,
            key: worker_settings::load_worker_key(config_dir)?,
            name: enrolled.name,
            work_dir,
            tools_cache_dir,
            ca: config::central_ca(),
            max_bytes: env::var("MAX_FILE_BYTES")
                .unwrap_or_else(|_| "536870912".into())
                .parse()?,
            transfer_url,
            transfer_port,
        })
    }
}
#[derive(Clone)]
struct WorkerLoop {
    client: Client,
    config: WorkerConfig,
    status: Arc<WorkerState>,
    /// Identifiant de ce processus, annoncé à son premier enregistrement (ADR 0027).
    boot_id: Uuid,
    held: HeldTasks,
    shutdown: watch::Receiver<bool>,
}
impl WorkerLoop {
    /// Se termine quand l'arrêt est demandé, même s'il l'a été avant l'appel.
    async fn stopped(&self) {
        let mut shutdown = self.shutdown.clone();
        // Émetteur disparu : le processus s'arrête aussi.
        let _ = shutdown.wait_for(|stopping| *stopping).await;
    }

    fn stopping(&self) -> bool {
        *self.shutdown.borrow()
    }

    async fn refresh_software_capabilities(&self) {
        let probes = self
            .request(
                reqwest::Method::GET,
                "/api/v1/worker/software-encoder-probes",
            )
            .timeout(REMOTE_BROWSE_HTTP_TIMEOUT)
            .send()
            .await;
        let Ok(response) = probes else {
            return;
        };
        let Ok(response) = response.error_for_status() else {
            return;
        };
        let Ok(probes) = response
            .json::<Vec<crate::software_probe::SoftwareEncoderProbe>>()
            .await
        else {
            return;
        };
        let cache = crate::tools_cache::ToolCacheManager::new(&self.config.tools_cache_dir);
        crate::software_probe::refresh_software_capabilities(&cache, &probes).await;
    }

    fn request(&self, method: reqwest::Method, path: &str) -> RequestBuilder {
        self.client
            .request(
                method,
                format!("{}{path}", self.config.url.as_str().trim_end_matches('/')),
            )
            .bearer_auth(&self.config.key)
    }
    /// Remonte la progression au central (SSE `job_progress`) sans bloquer le remux.
    /// Un seul POST par pourcentage distinct ; les échecs réseau sont ignorés.
    fn progress_callback(&self, claim: &Claim, worker: Uuid) -> crate::flows::FlowProgressCallback {
        let client = self.client.clone();
        let base = self.config.url.as_str().trim_end_matches('/').to_owned();
        let key = self.config.key.clone();
        let job_id = claim.job_id;
        let lease_token = claim.lease_token;
        let lease_epoch = claim.lease_epoch;
        let last_pct = Arc::new(AtomicU32::new(u32::MAX));
        Arc::new(move |pct: u32, line: &str| {
            let pct = pct.min(100);
            if last_pct.swap(pct, Ordering::Relaxed) == pct {
                return;
            }
            let client = client.clone();
            let base = base.clone();
            let key = key.clone();
            let message = Some(line.to_string());
            tokio::spawn(async move {
                let _ = client
                    .post(format!("{base}/api/v1/jobs/{job_id}/progress"))
                    .bearer_auth(key)
                    .timeout(PROGRESS_REPORT_TIMEOUT)
                    .json(&ProgressReport {
                        worker_id: worker,
                        lease_token,
                        lease_epoch,
                        progress: pct,
                        message,
                    })
                    .send()
                    .await;
            });
        })
    }
    /// Demande au central le contrat de distribution d'un outil, puis l'installe dans le cache local.
    async fn install_tool(
        &self,
        lease: &TaskLease,
        cache: &crate::tools_cache::ToolCacheManager,
        tool_id: &str,
        version: &str,
    ) -> anyhow::Result<()> {
        let path = format!("/api/v1/worker/tools/{tool_id}/distribution");
        let distribution: crate::tools_cache::ToolDistribution = lease
            .call(lease.budget(TOOL_DISTRIBUTION_TIMEOUT), || {
                self.request(reqwest::Method::GET, &path)
            })
            .await?
            .json()?;
        anyhow::ensure!(
            distribution.tool_id == tool_id && distribution.tool_version == version,
            "le contrat ne correspond pas à l'outil attendu"
        );
        crate::plugins::install_from_distribution(
            cache,
            &distribution,
            &crate::plugins::SyncPolicy::from_env(),
        )
        .await?;
        Ok(())
    }

    async fn process(
        &self,
        claim: &Claim,
        lease: &TaskLease,
        worker: Uuid,
        dir: &std::path::Path,
    ) -> anyhow::Result<()> {
        tokio::fs::create_dir(dir).await?;
        let cache_mgr = crate::tools_cache::ToolCacheManager::new(&self.config.tools_cache_dir);
        for req in job_runner::collect_wanted_tools(claim) {
            if cache_mgr.is_ready(&req.tool_id, &req.version, crate::tools_cache::host_arch()) {
                continue;
            }
            if let Err(error) = self
                .install_tool(lease, &cache_mgr, &req.tool_id, &req.version)
                .await
            {
                tracing::warn!(tool = %req.tool_id, version = %req.version, error = %error, "installation de l'outil impossible");
            }
        }
        self.refresh_software_capabilities().await;
        job_runner::ensure_probe_ready(&cache_mgr, claim)?;

        let input = dir.join(forgearr_shared::jobs::source_working_filename(
            claim.working_source_name(),
        ));
        if let Some(handoff) = &claim.handoff_input {
            // Remise par artefact (ADR 0022) : le fichier de travail se tire chez son détenteur.
            lease.pull_handoff_input(claim, handoff, &input).await?;
        } else if let Some(root) = claim.local_source_root.as_deref() {
            let source = crate::library_scan::source_path(Path::new(root), &claim.source_name)
                .await
                .map_err(|_| {
                    JobRunError::simple(JobErrorCode::SourceIntegrity, "source_integrity")
                })?;
            crate::local_worker::copy_verified_source(
                &source,
                &input,
                self.config.max_bytes,
                &claim.source_sha256,
                claim.source_bytes,
            )
            .await
            .map_err(|_| JobRunError::simple(JobErrorCode::SourceIntegrity, "source_integrity"))?;
        } else {
            self.download_source(claim, lease, &input).await?;
        }

        let progress_cb = self.progress_callback(claim, worker);
        let outcome = job_runner::execute_claim_processing(
            claim,
            &cache_mgr,
            &self.client,
            None,
            None,
            &input,
            dir,
            progress_cb,
            lease.gate(),
        )
        .await?;
        let step_report = lease
            .report_steps(&outcome.steps, Some(&outcome.media_info))
            .await;
        if let Some(request) = outcome.relay {
            step_report?;
            return lease
                .send_handoff(claim, request, &outcome.working_file, dir, outcome.warning)
                .await;
        }
        match step_report {
            Ok(()) => {}
            Err(CallFailure::Stop(stop)) => return Err(stop.into()),
            Err(_) => {
                tracing::warn!(job_id=%claim.job_id, "step report rejected or unavailable");
            }
        }
        lease
            .publish_retained_result(
                claim,
                TaskResult {
                    file: &outcome.working_file,
                    extension: &outcome.output_spec.extension,
                    warning: outcome.warning,
                },
            )
            .await
    }

    /// Récupère la source en direct chez le worker qui la détient, avec le billet de la tâche
    /// (ADR 0019 et 0022). Sans billet, aucun worker ne l'annonce : la tâche échoue.
    async fn download_source(
        &self,
        claim: &Claim,
        lease: &TaskLease,
        input: &Path,
    ) -> anyhow::Result<()> {
        let Some(ticket) = claim.source_ticket.clone() else {
            let unreachable =
                JobRunError::simple(JobErrorCode::TransferFailed, "source_unreachable");
            return Err(unreachable.into());
        };
        let direct = worker_transfer::direct_client()?;
        worker_transfer::download_direct_resuming(
            &direct,
            ticket,
            input,
            self.config.max_bytes,
            &claim.source_sha256,
            claim.source_bytes,
            || lease.reissue_ticket(),
        )
        .await
        .map_err(|error| {
            if worker_transfer::is_source_integrity(&error) {
                JobRunError::simple(
                    JobErrorCode::SourceIntegrity,
                    worker_transfer::SOURCE_INTEGRITY,
                )
                .into()
            } else {
                error
            }
        })
    }

    /// Traite la tâche puis déclare son échec éventuel. Un arrêt ([`TaskStop`]) ne se déclare pas.
    async fn process_and_report(
        &self,
        claim: &Claim,
        lease: &TaskLease,
        worker: Uuid,
        dir: &Path,
    ) -> Result<(), TaskStop> {
        let Err(error) = self.process(claim, lease, worker, dir).await else {
            return Ok(());
        };
        if let Some(stop) = error.downcast_ref::<TaskStop>() {
            return Err(*stop);
        }
        let (code, steps) = match error.downcast_ref::<JobRunError>() {
            Some(run_error) => (run_error.code, run_error.steps.as_slice()),
            None => (JobErrorCode::TransferFailed, Default::default()),
        };
        // Do not format reqwest errors: their URLs may contain sensitive context.
        tracing::warn!(job_id=%claim.job_id, error_code=%code, "processing failed");
        match lease.report_failure(code, steps).await {
            Ok(()) => Ok(()),
            Err(CallFailure::Stop(stop)) => Err(stop),
            Err(_) => {
                tracing::warn!(job_id=%claim.job_id, "failure report rejected or unavailable");
                Ok(())
            }
        }
    }

    /// Exécute une tâche jusqu'à son issue, la perte de son bail ou l'arrêt du worker. Le dossier de
    /// travail ne disparaît et la tâche n'est rendue qu'une fois ses processus morts.
    async fn execute(&self, claim: Claim, worker: Uuid) {
        let _held = self.held.hold(claim.job_id, claim.lease_token);
        let lease = TaskLease::new(self, &claim, worker);
        let dir = self
            .config
            .work_dir
            .join(format!("{}-{}", claim.job_id, claim.lease_token));
        let mut work = TaskWork::default();
        // Abandonner le traitement tue le groupe de ses processus.
        let outcome = tokio::select! {
            outcome = work.scope(self.process_and_report(&claim, &lease, worker, &dir)) => outcome,
            () = lease.heartbeat_until_lost() => Err(TaskStop::LeaseLost),
            () = self.stopped() => Err(TaskStop::Shutdown),
        };
        if let Err(stop) = outcome {
            tracing::warn!(job_id=%claim.job_id, reason=%stop, "tâche arrêtée sans échec déclaré");
        }
        let shutdown = outcome == Err(TaskStop::Shutdown);
        let limit = if shutdown {
            SHUTDOWN_WORK_TIMEOUT
        } else {
            ABANDONED_WORK_TIMEOUT
        };
        if !work.ended(limit).await {
            // Le démarrage suivant purgera ; le central tranche pour la tâche.
            tracing::warn!(job_id=%claim.job_id, "travail encore en cours : dossier conservé");
            return;
        }
        if shutdown {
            lease.release().await;
        }
        if let Err(error) = tokio::fs::remove_dir_all(&dir).await
            && error.kind() != std::io::ErrorKind::NotFound
        {
            tracing::warn!(job_id=%claim.job_id, error=%error, "work directory cleanup failed");
        }
    }

    /// Interroge le central sur les versions actives d'outils et purge les versions obsolètes du cache local.
    async fn prune_stale_tools_cache(&self) {
        let Ok(resp) = self
            .request(reqwest::Method::GET, "/api/v1/worker/tools")
            .timeout(CENTRAL_REQUEST_TIMEOUT)
            .send()
            .await
        else {
            return;
        };
        if !resp.status().is_success() {
            return;
        }
        let Ok(active) = resp.json::<Vec<crate::domain::ToolRequirement>>().await else {
            return;
        };
        let keep: Vec<(String, String)> = active
            .into_iter()
            .map(|req| (req.tool_id, req.version))
            .collect();
        let cache = crate::tools_cache::ToolCacheManager::new(&self.config.tools_cache_dir);
        if let Ok(pruned) = cache.prune_unused(&keep)
            && pruned > 0
        {
            tracing::info!(pruned, "anciennes versions d'outils purgées du cache");
        }
    }
}

pub async fn run(config: WorkerConfig) -> anyhow::Result<()> {
    anyhow::ensure!(
        worker_settings::is_valid_worker_name(&config.name),
        "nom de worker invalide dans les réglages d'enrôlement"
    );
    anyhow::ensure!(config.max_bytes > 0, "MAX_FILE_BYTES must be positive");
    tokio::fs::create_dir_all(&config.work_dir).await?;
    let mut builder = Client::builder()
        .connect_timeout(CENTRAL_CONNECT_TIMEOUT)
        .timeout(forgearr_shared::transfer::FILE_TRANSFER_TIMEOUT)
        .redirect(reqwest::redirect::Policy::none());
    builder = config::trust_ca(builder, config.ca.as_deref())?;
    let status = WorkerState::new(&config.name, config.url.as_str().trim_end_matches('/'));
    let client = builder.build()?;
    let central = CentralClient::new(client.clone(), config.url.clone(), config.key.clone());
    let web = WorkerWeb::new(
        status.clone(),
        Arc::new(crate::config::Config::load()?),
        central,
        config.tools_cache_dir.clone(),
    );
    let http = tokio::spawn(crate::worker_http::serve(web.clone()));
    let account_sync = tokio::spawn(web.sync_accounts_loop());
    let (stop, shutdown) = watch::channel(false);
    // Un seul écouteur durable : une tâche occupée au moment du signal le voit quand même.
    let signal = tokio::spawn(async move {
        crate::api::shutdown_signal().await;
        let _ = stop.send(true);
    });
    let runner = WorkerLoop {
        client,
        config,
        status,
        boot_id: Uuid::new_v4(),
        held: HeldTasks::default(),
        shutdown,
    };
    let outcome = runner.work().await;
    signal.abort();
    http.abort();
    account_sync.abort();
    outcome
}

impl WorkerLoop {
    /// Répond aux demandes d'exploration de dossiers relayées par le central.
    async fn respond_to_browse_requests(&self, worker_id: Uuid) {
        let query = format!(
            "/api/v1/worker/filesystem/pending?worker_id={worker_id}&wait_ms={REMOTE_BROWSE_WAIT_MS}"
        );
        loop {
            let fetch = self
                .request(reqwest::Method::GET, &query)
                .timeout(REMOTE_BROWSE_HTTP_TIMEOUT)
                .send();
            let response = tokio::select! {
                res = fetch => res,
                () = self.stopped() => return,
            };
            let tasks = match response {
                Ok(resp) if resp.status().is_success() => resp
                    .json::<Vec<crate::domain::WorkerBrowseTask>>()
                    .await
                    .unwrap_or_default(),
                _ => {
                    tokio::select! {
                        _ = tokio::time::sleep(REMOTE_BROWSE_RETRY_DELAY) => continue,
                        () = self.stopped() => return,
                    }
                }
            };
            for task in tasks {
                let mut report = crate::domain::WorkerBrowseReport {
                    request_id: task.request_id,
                    worker_id,
                    listing: None,
                    scan: None,
                    error: None,
                };
                let result = if let Some(mut options) = task.scan {
                    options.max_bytes = options.max_bytes.min(self.config.max_bytes);
                    match task.path.as_deref() {
                        Some(path) => crate::library_scan::scan_folder(
                            Path::new(path),
                            &options,
                            &[&self.config.work_dir, &self.config.tools_cache_dir],
                        )
                        .await
                        .map(|scan| report.scan = Some(scan)),
                        None => Err(crate::error::AppError::Invalid),
                    }
                } else {
                    crate::filesystem_browser::browse_local(worker_id, task.path.as_deref(), None)
                        .await
                        .map(|listing| report.listing = Some(listing))
                };
                if let Err(err) = result {
                    report.error =
                        Some(crate::filesystem_browser::error_code_for_report(&err).to_owned());
                }
                let _ = self
                    .request(reqwest::Method::POST, "/api/v1/worker/filesystem")
                    .timeout(REMOTE_BROWSE_REPORT_TIMEOUT)
                    .json(&report)
                    .send()
                    .await;
            }
        }
    }

    /// Répond aux demandes de retour du résultat chez le détenteur de la source.
    async fn respond_to_publication_requests(&self, worker_id: Uuid, kind: PublicationRequestKind) {
        let pending = format!(
            "{}/pending?worker_id={worker_id}&wait_ms={REMOTE_BROWSE_WAIT_MS}",
            kind.route()
        );
        loop {
            let tasks = tokio::select! {
                () = self.stopped() => return,
                result = async {
                    self.request(reqwest::Method::GET, &pending)
                        .timeout(REMOTE_BROWSE_HTTP_TIMEOUT).send().await?.error_for_status()?
                        .json::<Vec<forgearr_shared::transfer::SourcePublicationTask>>().await
                } => result,
            };
            let Ok(tasks) = tasks else {
                tokio::time::sleep(REMOTE_BROWSE_RETRY_DELAY).await;
                continue;
            };
            for task in tasks {
                let runner = self.clone();
                tokio::spawn(async move {
                    runner.publish_source_result(task, kind).await;
                });
            }
        }
    }

    async fn publish_source_result(
        &self,
        task: forgearr_shared::transfer::SourcePublicationTask,
        kind: PublicationRequestKind,
    ) {
        use forgearr_shared::transfer::{SourcePublicationOutcome, SourcePublicationReport};
        let prefix = format!("{}/{}", kind.route(), task.request_id);
        let file = self
            .config
            .work_dir
            .join(format!(".source-publication-{}.tmp", task.request_id));
        let result: crate::error::AppResult<()> = async {
            if matches!(kind, PublicationRequestKind::Inspect) {
                let matches = crate::source_publication::published_source_matches(
                    &crate::source_publication::PublishedSourceExpectation {
                        root: Path::new(&task.root),
                        source_name: &task.source_name,
                        source_sha256: &task.source_sha256,
                        source_bytes: task.source_bytes,
                        result_sha256: &task.result_sha256,
                        result_bytes: task.result_bytes,
                        extension: &task.extension,
                        policy: task.policy,
                    },
                    self.config.max_bytes,
                )
                .await?;
                return if matches {
                    Ok(())
                } else {
                    Err(crate::error::AppError::Invalid)
                };
            }
            self.fetch_result(&prefix, &task, &file).await?;
            let source = crate::library_scan::source_path(Path::new(&task.root), &task.source_name)
                .await
                .map_err(crate::source_publication::missing_source_is_changed)?;
            crate::source_publication::apply_library_output_policy(
                task.policy,
                &source,
                &file,
                &task.extension,
                &task.source_sha256,
                task.source_bytes,
                self.config.max_bytes,
                async {
                    let path = format!("{prefix}/authorize?worker_id={}", task.worker_id);
                    let granted = self
                        .attempt_for(PUBLICATION_REPORT_PATIENCE, || {
                            self.request(reqwest::Method::GET, &path)
                                .timeout(REMOTE_BROWSE_HTTP_TIMEOUT)
                        })
                        .await;
                    if granted.reply == CentralReply::Accepted {
                        Ok(())
                    } else {
                        Err(crate::error::AppError::CentralUnavailable)
                    }
                },
            )
            .await
        }
        .await;
        let outcome = match result {
            Ok(()) => SourcePublicationOutcome::Published,
            Err(crate::error::AppError::SourceChanged) => SourcePublicationOutcome::SourceChanged,
            Err(_) => SourcePublicationOutcome::Failed,
        };
        if matches!(kind, PublicationRequestKind::Publish) {
            let _ = tokio::fs::remove_file(file).await;
        }
        let report = SourcePublicationReport {
            request_id: task.request_id,
            worker_id: task.worker_id,
            outcome,
        };
        let path = format!("{prefix}/report");
        let reported = self
            .attempt_for(PUBLICATION_REPORT_PATIENCE, || {
                self.request(reqwest::Method::POST, &path)
                    .timeout(REMOTE_BROWSE_REPORT_TIMEOUT)
                    .json(&report)
            })
            .await;
        if reported.reply != CentralReply::Accepted {
            tracing::warn!(request_id=%task.request_id, "rapport de publication sans réponse");
        }
    }

    /// Démarre le serveur de transfert en lecture seule si une adresse et un port sont configurés.
    fn spawn_transfer_server(&self, worker_id: Uuid) -> Option<tokio::task::JoinHandle<()>> {
        let port = self.config.transfer_port?;
        self.config.transfer_url.as_ref()?;
        let state = worker_transfer::TransferState::new(worker_transfer::central_redeemer(
            self.client.clone(),
            self.config.url.clone(),
            self.config.key.clone(),
            worker_id,
        ));
        Some(worker_transfer::spawn_listener(port, state))
    }

    /// Enregistre le worker puis réclame des tâches jusqu'à l'arrêt.
    async fn work(&self) -> anyhow::Result<()> {
        let runner = self;
        runner.refresh_software_capabilities().await;
        let probe = crate::telemetry::TelemetryProbe::with_storage_path(&runner.config.work_dir);
        let worker = loop {
            tokio::select! {
                result = runner.register(probe.sample(), Some(runner.boot_id)) => match result {
                    Ok(worker) => break worker,
                    Err(_) => tracing::warn!("registration unavailable; retrying"),
                },
                () = runner.stopped() => return Ok(()),
            }
            tokio::select! {
                () = tokio::time::sleep(REGISTRATION_RETRY_DELAY) => {}
                () = runner.stopped() => return Ok(()),
            }
        };
        tracing::info!(worker_id=%worker.id, "worker registered");
        runner.status.mark_registered();
        runner.status.mark_central_reachable(true);
        // Aucune tâche n'est encore tenue : tout ce qui reste vient du processus précédent.
        startup_purge::purge(&runner.config.work_dir).await;
        runner.prune_stale_tools_cache().await;
        let browse_responder = {
            let browse_runner = runner.clone();
            tokio::spawn(async move {
                browse_runner.respond_to_browse_requests(worker.id).await;
            })
        };
        let transfer_server = runner.spawn_transfer_server(worker.id);
        let source_publication_responder = {
            let publication_runner = runner.clone();
            tokio::spawn(async move {
                publication_runner
                    .respond_to_publication_requests(worker.id, PublicationRequestKind::Publish)
                    .await;
            })
        };
        let source_inspection_responder = {
            let inspection_runner = runner.clone();
            let worker_id = worker.id;
            tokio::spawn(async move {
                inspection_runner
                    .respond_to_publication_requests(worker_id, PublicationRequestKind::Inspect)
                    .await;
            })
        };
        let software_probe_refresher = {
            let probe_runner = runner.clone();
            let probe_worker_id = worker.id;
            tokio::spawn(async move {
                if let Err(error) = probe_runner.prune_handoffs(probe_worker_id).await {
                    tracing::warn!(error=%error, "retained handoff cleanup unavailable");
                }
                loop {
                    tokio::select! {
                        () = probe_runner.stopped() => return,
                        _ = tokio::time::sleep(SOFTWARE_PROBE_REFRESH) => {
                            probe_runner.refresh_software_capabilities().await;
                            if let Err(error) = probe_runner.prune_handoffs(probe_worker_id).await {
                                tracing::warn!(error=%error, "retained handoff cleanup unavailable");
                            }
                        },
                    }
                }
            })
        };
        let mut active = JoinSet::new();
        runner
            .claim_until_stopped(&worker, &probe, &mut active)
            .await;
        runner.drain(&mut active).await;
        browse_responder.abort();
        source_publication_responder.abort();
        source_inspection_responder.abort();
        software_probe_refresher.abort();
        if let Some(server) = transfer_server {
            server.abort();
        }
        Ok(())
    }

    /// Enregistre ce worker. `boot_id` n'accompagne que le premier enregistrement du processus : le
    /// répéter annulerait les tâches d'un autre processus du même nom, sans fin (ADR 0027).
    async fn register(
        &self,
        telemetry: crate::telemetry::TelemetrySnapshot,
        boot_id: Option<Uuid>,
    ) -> reqwest::Result<Worker> {
        self.request(reqwest::Method::POST, "/api/v1/workers")
            .timeout(CENTRAL_REQUEST_TIMEOUT)
            .json(&Register {
                name: self.config.name.clone(),
                version: Some(crate::version::app_version().to_owned()),
                telemetry: Some(telemetry),
                transfer_url: self.config.transfer_url.clone(),
                boot_id,
            })
            .send()
            .await?
            .error_for_status()?
            .json::<Worker>()
            .await
    }

    async fn claim(
        &self,
        worker_id: Uuid,
        telemetry: Option<crate::telemetry::TelemetrySnapshot>,
    ) -> reqwest::Result<Option<Claim>> {
        self.request(reqwest::Method::POST, "/api/v1/claims")
            .timeout(CENTRAL_REQUEST_TIMEOUT)
            .json(&ClaimRequest {
                worker_id,
                telemetry,
                lease_protocol: Some(LeaseProtocol::LATEST.number()),
                active_jobs: Some(self.held.jobs()),
            })
            .send()
            .await?
            .error_for_status()?
            .json::<Option<Claim>>()
            .await
    }

    /// Réserve et lance des tâches jusqu'à la demande d'arrêt.
    async fn claim_until_stopped(
        &self,
        worker: &Worker,
        probe: &crate::telemetry::TelemetryProbe,
        active: &mut JoinSet<()>,
    ) {
        let mut was_disconnected = false;
        let mut idle_polls: u32 = 0;
        // Total des places ; la limite par classe (CPU/GPU) est appliquée par `claim`.
        let mut max_concurrency = worker.slots().total.max(1);
        while !self.stopping() {
            while active.try_join_next().is_some() {}
            let report_due = idle_polls.is_multiple_of(REREGISTRATION_EVERY_IDLE_POLLS);
            if idle_polls > 0
                && report_due
                && let Ok(updated) = self.register(probe.sample(), None).await
            {
                max_concurrency = updated.slots().total.max(1);
            }
            let telemetry = report_due.then(|| probe.sample());
            idle_polls = idle_polls.wrapping_add(1);
            while (active.len() as i32) < max_concurrency {
                let claim = tokio::select! {
                    claim = self.claim(worker.id, telemetry.clone()) => claim,
                    () = self.stopped() => return,
                };
                self.status.mark_central_reachable(claim.is_ok());
                if claim.is_ok() && std::mem::take(&mut was_disconnected) && active.is_empty() {
                    self.prune_stale_tools_cache().await;
                }
                match claim {
                    Ok(Some(claim)) => {
                        self.start(claim, worker.id, active);
                    }
                    Ok(None) => break,
                    Err(_) => {
                        was_disconnected = true;
                        tracing::warn!("central unavailable; retrying");
                        break;
                    }
                }
            }
            tokio::select! {
                () = tokio::time::sleep(IDLE_POLL_INTERVAL) => {},
                Some(_) = active.join_next(), if !active.is_empty() => {},
                () = self.stopped() => return,
            }
        }
    }

    /// Lance la tâche réservée ; un bail dont les secrets ne se lisent pas est déclaré en échec.
    fn start(&self, mut claim: Claim, worker_id: Uuid, active: &mut JoinSet<()>) {
        let runner = self.clone();
        let Err(error) =
            crate::flows::unseal_claim_secrets_for_worker(&mut claim, &self.config.key)
        else {
            active.spawn(async move { runner.execute(claim, worker_id).await });
            return;
        };
        tracing::error!(job_id = %claim.job_id, error = %error, "impossible de déchiffrer les secrets du bail");
        active.spawn(async move {
            let _held = runner.held.hold(claim.job_id, claim.lease_token);
            let lease = TaskLease::new(&runner, &claim, worker_id);
            let reported = tokio::select! {
                reported = lease.report_failure(JobErrorCode::TransferFailed, &[]) => reported,
                () = runner.stopped() => return,
            };
            if let Err(failure) = reported {
                tracing::warn!(job_id = %claim.job_id, ?failure, "failure report rejected or unavailable");
            }
        });
    }

    /// Arrêt propre : chaque tâche s'arrête et se rend ; celles qui tardent sont abandonnées.
    async fn drain(&self, active: &mut JoinSet<()>) {
        let drained = async { while active.join_next().await.is_some() {} };
        if tokio::time::timeout(SHUTDOWN_TIMEOUT, drained)
            .await
            .is_err()
        {
            tracing::warn!("arrêt : des tâches n'ont pas pu être rendues");
            active.abort_all();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inspections_use_a_distinct_read_only_channel() {
        assert_ne!(
            PublicationRequestKind::Publish.route(),
            PublicationRequestKind::Inspect.route()
        );
        assert!(
            PublicationRequestKind::Inspect
                .route()
                .contains("inspections")
        );
    }
}
