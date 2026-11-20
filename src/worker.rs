use crate::{
    central_client::CentralClient,
    config,
    domain::{
        Claim, ClaimRequest, Failure, JobErrorCode, JobStepEntry, LeaseRequest, ProgressReport,
        Register, StepsReport, Worker,
    },
    job_runner::{self, JobRunError},
    storage,
    worker_http::{WorkerState, WorkerWeb},
    worker_settings, worker_transfer,
};
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
use tokio_util::io::ReaderStream;
use uuid::Uuid;

const REMOTE_BROWSE_WAIT_MS: u64 = 10_000;
const REMOTE_BROWSE_HTTP_TIMEOUT: Duration = Duration::from_secs(20);
const REMOTE_BROWSE_REPORT_TIMEOUT: Duration = Duration::from_secs(10);
const REMOTE_BROWSE_RETRY_DELAY: Duration = Duration::from_secs(2);
mod handoff;
pub(crate) mod timing;
use timing::{
    CENTRAL_CONNECT_TIMEOUT, CENTRAL_REQUEST_TIMEOUT, HEARTBEAT_INTERVAL, IDLE_POLL_INTERVAL,
    PROGRESS_REPORT_TIMEOUT, REGISTRATION_RETRY_DELAY, REREGISTRATION_EVERY_IDLE_POLLS,
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

fn publication_error_code(error: &serde_json::Value) -> JobErrorCode {
    if error
        .pointer("/error/code")
        .and_then(serde_json::Value::as_str)
        == Some(JobErrorCode::SourceChanged.as_str())
    {
        JobErrorCode::SourceChanged
    } else {
        JobErrorCode::TransferFailed
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
    let url = env::var("WORKER_TRANSFER_URL")
        .ok()
        .and_then(|raw| crate::repository::valid_transfer_url(&raw));
    let port = env::var("WORKER_TRANSFER_PORT")
        .ok()
        .and_then(|raw| raw.trim().parse().ok())
        .or_else(|| {
            url.as_deref()
                .and_then(|raw| reqwest::Url::parse(raw).ok())
                .and_then(|parsed| parsed.port())
        });
    (url, port)
}

impl WorkerConfig {
    /// Un worker enrôlé lit adresse, nom et clé dans `/config` ; sinon (automatisation) dans l'environnement.
    pub fn load(config_dir: &Path) -> anyhow::Result<Self> {
        let work_dir: PathBuf = env::var("WORK_DIR")
            .unwrap_or_else(|_| "var/work".into())
            .into();
        let tools_cache_dir = env::var("TOOLS_CACHE_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| work_dir.join("tools-cache"));
        let (url, key, name) = match worker_settings::load_worker(config_dir)? {
            Some(enrolled) => (
                config::central_url(&enrolled.central_url)?,
                worker_settings::load_worker_key(config_dir)?,
                enrolled.name,
            ),
            None => (
                config::central_url(&env::var("CENTRAL_URL")?)?,
                config::secret("WORKER_KEY")?,
                env::var("WORKER_NAME").or_else(|_| env::var("HOSTNAME"))?,
            ),
        };
        let (transfer_url, transfer_port) = transfer_settings();
        Ok(Self {
            url,
            key,
            name,
            work_dir,
            tools_cache_dir,
            ca: env::var("CA_CERT").ok().map(Into::into),
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
}
impl WorkerLoop {
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
    fn leased(
        &self,
        method: reqwest::Method,
        claim: &Claim,
        worker: Uuid,
        action: &str,
    ) -> RequestBuilder {
        self.request(method, &format!("/api/v1/jobs/{}/{action}", claim.job_id))
            .header("x-worker-id", worker.to_string())
            .header("x-lease-token", claim.lease_token.to_string())
            .header("x-lease-epoch", claim.lease_epoch.to_string())
    }
    async fn heartbeat(&self, claim: &Claim, worker: Uuid) -> anyhow::Result<()> {
        let probe = crate::telemetry::TelemetryProbe::with_storage_path(&self.config.work_dir);
        loop {
            tokio::time::sleep(HEARTBEAT_INTERVAL).await;
            self.request(
                reqwest::Method::POST,
                &format!("/api/v1/jobs/{}/heartbeat", claim.job_id),
            )
            .timeout(CENTRAL_REQUEST_TIMEOUT)
            .json(&LeaseRequest {
                worker_id: worker,
                lease_token: claim.lease_token,
                lease_epoch: claim.lease_epoch,
                telemetry: Some(probe.sample()),
            })
            .send()
            .await?
            .error_for_status()?;
        }
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
        cache: &crate::tools_cache::ToolCacheManager,
        tool_id: &str,
        version: &str,
    ) -> anyhow::Result<()> {
        let raw = self
            .request(
                reqwest::Method::GET,
                &format!("/api/v1/worker/tools/{tool_id}/distribution"),
            )
            .timeout(TOOL_DISTRIBUTION_TIMEOUT)
            .send()
            .await?
            .error_for_status()?
            .text()
            .await?;
        let distribution = crate::plugins::parse_distribution(&raw)?;
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

    async fn report_steps(
        &self,
        claim: &Claim,
        worker: Uuid,
        steps: &[JobStepEntry],
    ) -> anyhow::Result<()> {
        if steps.is_empty() {
            return Ok(());
        }
        self.request(
            reqwest::Method::POST,
            &format!("/api/v1/jobs/{}/steps", claim.job_id),
        )
        .timeout(CENTRAL_REQUEST_TIMEOUT)
        .json(&StepsReport {
            worker_id: worker,
            lease_token: claim.lease_token,
            lease_epoch: claim.lease_epoch,
            steps: steps.to_vec(),
        })
        .send()
        .await?
        .error_for_status()?;
        Ok(())
    }

    async fn process(
        &self,
        claim: &Claim,
        worker: Uuid,
        dir: &std::path::Path,
    ) -> anyhow::Result<()> {
        let cache_mgr = crate::tools_cache::ToolCacheManager::new(&self.config.tools_cache_dir);
        for req in job_runner::collect_wanted_tools(claim) {
            if cache_mgr.is_ready(&req.tool_id, &req.version, crate::tools_cache::host_arch()) {
                continue;
            }
            if let Err(error) = self
                .install_tool(&cache_mgr, &req.tool_id, &req.version)
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
            self.pull_handoff_input(claim, worker, handoff, &input)
                .await?;
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
            self.download_source(claim, worker, &input).await?;
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
        )
        .await?;
        let step_report = self.report_steps(claim, worker, &outcome.steps).await;
        if let Some(request) = outcome.relay {
            step_report?;
            self.send_handoff(
                claim,
                worker,
                request,
                &outcome.working_file,
                dir,
                outcome.warning,
            )
            .await?;
            return Ok(());
        }
        if step_report.is_err() {
            tracing::warn!(job_id=%claim.job_id, "step report rejected or unavailable");
        }

        let (_, output_size) =
            storage::fingerprint(&outcome.working_file, self.config.max_bytes).await?;
        let body = reqwest::Body::wrap_stream(ReaderStream::new(
            tokio::fs::File::open(&outcome.working_file).await?,
        ));
        let response = self
            .leased(reqwest::Method::PUT, claim, worker, "result")
            .header("content-length", output_size)
            .header("x-remux-warning", outcome.warning.to_string())
            .header("x-output-ext", &outcome.output_spec.extension)
            .body(body)
            .send()
            .await?;
        if !response.status().is_success() {
            let error = response
                .json::<serde_json::Value>()
                .await
                .unwrap_or_default();
            let code = publication_error_code(&error);
            return Err(JobRunError::simple(code, code.as_str()).into());
        }
        Ok(())
    }
    /// Demande au central un billet frais pour la source de cette tâche (bail actif exigé).
    async fn reissue_ticket(
        &self,
        claim: &Claim,
        worker: Uuid,
    ) -> anyhow::Result<Option<forgearr_shared::transfer::SourceTicket>> {
        Ok(self
            .leased(reqwest::Method::POST, claim, worker, "source/ticket")
            .timeout(REMOTE_BROWSE_HTTP_TIMEOUT)
            .send()
            .await?
            .error_for_status()?
            .json::<Option<forgearr_shared::transfer::SourceTicket>>()
            .await?)
    }

    /// Récupère la source : voie directe chez le worker qui la détient (si un billet est fourni),
    /// puis repli par le central (ADR 0019).
    async fn download_source(
        &self,
        claim: &Claim,
        worker: Uuid,
        input: &Path,
    ) -> anyhow::Result<()> {
        if let Some(ticket) = claim.source_ticket.clone() {
            let direct = worker_transfer::direct_client()?;
            match worker_transfer::download_direct_resuming(
                &direct,
                ticket,
                input,
                self.config.max_bytes,
                &claim.source_sha256,
                claim.source_bytes,
                || self.reissue_ticket(claim, worker),
            )
            .await
            {
                Ok(()) => return Ok(()),
                Err(error) => {
                    tracing::warn!(job_id = %claim.job_id, error = %error, "voie directe indisponible, repli par le central");
                    let _ = tokio::fs::remove_file(input).await;
                }
            }
        }
        let response = self
            .leased(reqwest::Method::GET, claim, worker, "source")
            .send()
            .await?
            .error_for_status()?;
        worker_transfer::save_verified(
            response,
            input,
            self.config.max_bytes,
            &claim.source_sha256,
            claim.source_bytes,
        )
        .await
        .map_err(|error| {
            if error.to_string() == worker_transfer::SOURCE_INTEGRITY {
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

    async fn execute(&self, claim: Claim, worker: Uuid) -> anyhow::Result<()> {
        let dir = self
            .config
            .work_dir
            .join(format!("{}-{}", claim.job_id, claim.lease_token));
        tokio::fs::create_dir(&dir).await?;
        // Dropping processing on a lost lease or shutdown kills the child process.
        let result = tokio::select! {
            result = self.process(&claim, worker, &dir) => result,
            result = self.heartbeat(&claim, worker) => result,
            _ = crate::api::shutdown_signal() => Err(anyhow::anyhow!("shutdown")),
        };
        if let Err(ref error) = result {
            if error.to_string() == "shutdown" {
                tokio::fs::remove_dir_all(&dir).await?;
                return result;
            }
            let (code, steps) = if let Some(run_err) = error.downcast_ref::<JobRunError>() {
                (run_err.code.as_str(), Some(run_err.steps.as_slice()))
            } else {
                ("transfer_failed", None)
            };
            if let Some(steps) = steps
                && self.report_steps(&claim, worker, steps).await.is_err()
            {
                tracing::warn!(job_id=%claim.job_id, "step report rejected or unavailable");
            }
            tracing::warn!(job_id=%claim.job_id, error_code=code, "processing failed");
            // Do not format reqwest errors: their URLs may contain sensitive context.
            let report = self
                .request(
                    reqwest::Method::POST,
                    &format!("/api/v1/jobs/{}/failure", claim.job_id),
                )
                .timeout(CENTRAL_REQUEST_TIMEOUT)
                .json(&Failure {
                    worker_id: worker,
                    lease_token: claim.lease_token,
                    lease_epoch: claim.lease_epoch,
                    error_code: code.into(),
                })
                .send()
                .await;
            if !matches!(report, Ok(r) if r.status().is_success()) {
                tracing::warn!(job_id=%claim.job_id, "failure report rejected or unavailable");
            }
        }
        tokio::fs::remove_dir_all(dir).await?;
        Ok(())
    }

    /// Purge les résidus orphelins (anciens répertoires job-* ou fichiers .tmp)
    /// dans le répertoire de travail local sans toucher aux outils ou à la configuration.
    async fn purge_orphaned_work_artifacts(&self) {
        let mut entries = match tokio::fs::read_dir(&self.config.work_dir).await {
            Ok(e) => e,
            Err(_) => return,
        };
        while let Ok(Some(entry)) = entries.next_entry().await {
            let file_name = entry.file_name();
            let name = file_name.to_string_lossy();
            if name.starts_with("job-") || name.starts_with(".tmp") || name.ends_with(".tmp") {
                let path = entry.path();
                let is_dir = entry.file_type().await.map(|t| t.is_dir()).unwrap_or(false);
                tracing::info!(path = %path.display(), "purging orphaned work artifact");
                if is_dir {
                    let _ = tokio::fs::remove_dir_all(&path).await;
                } else {
                    let _ = tokio::fs::remove_file(&path).await;
                }
            }
        }
        self.prune_stale_tools_cache().await;
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
        "Invalid WORKER_NAME"
    );
    anyhow::ensure!(config.max_bytes > 0, "MAX_FILE_BYTES must be positive");
    tokio::fs::create_dir_all(&config.work_dir).await?;
    let mut builder = Client::builder()
        .connect_timeout(CENTRAL_CONNECT_TIMEOUT)
        .timeout(forgearr_shared::transfer::FILE_TRANSFER_TIMEOUT)
        .redirect(reqwest::redirect::Policy::none());
    if let Some(ref ca) = config.ca {
        builder = builder
            .add_root_certificate(reqwest::Certificate::from_pem(&tokio::fs::read(ca).await?)?);
    }
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
    let runner = WorkerLoop {
        client,
        config,
        status,
    };
    let outcome = runner.work().await;
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
                _ = crate::api::shutdown_signal() => return,
            };
            let tasks = match response {
                Ok(resp) if resp.status().is_success() => resp
                    .json::<Vec<crate::domain::WorkerBrowseTask>>()
                    .await
                    .unwrap_or_default(),
                _ => {
                    tokio::select! {
                        _ = tokio::time::sleep(REMOTE_BROWSE_RETRY_DELAY) => continue,
                        _ = crate::api::shutdown_signal() => return,
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

    /// Envoie au central, à sa demande, les sources que ce worker détient (repli, ADR 0019).
    async fn respond_to_source_relay_requests(&self, worker_id: Uuid) {
        let pending = format!(
            "/api/v1/worker/source-relay/pending?worker_id={worker_id}&wait_ms={REMOTE_BROWSE_WAIT_MS}"
        );
        loop {
            let tasks = tokio::select! {
                _ = crate::api::shutdown_signal() => return,
                result = async {
                    self.request(reqwest::Method::GET, &pending)
                        .timeout(REMOTE_BROWSE_HTTP_TIMEOUT)
                        .send()
                        .await?
                        .error_for_status()?
                        .json::<Vec<forgearr_shared::transfer::SourceRelayTask>>()
                        .await
                } => result,
            };
            let Ok(tasks) = tasks else {
                tokio::time::sleep(REMOTE_BROWSE_RETRY_DELAY).await;
                continue;
            };
            for task in tasks {
                let runner = self.clone();
                tokio::spawn(async move {
                    if let Err(error) = runner.send_source_to_central(task).await {
                        tracing::warn!(error = %error, "envoi de source au central interrompu");
                    }
                });
            }
        }
    }

    async fn send_source_to_central(
        &self,
        task: forgearr_shared::transfer::SourceRelayTask,
    ) -> anyhow::Result<()> {
        let path =
            crate::library_scan::source_path(Path::new(&task.root), &task.source_name).await?;
        let file = tokio::fs::File::open(path).await?;
        self.request(
            reqwest::Method::PUT,
            &format!(
                "/api/v1/worker/source-relay/{}?worker_id={}",
                task.request_id, task.worker_id
            ),
        )
        .body(reqwest::Body::wrap_stream(ReaderStream::new(file)))
        .send()
        .await?
        .error_for_status()?;
        Ok(())
    }

    /// Répond aux demandes de retour du résultat chez le détenteur de la source.
    async fn respond_to_publication_requests(&self, worker_id: Uuid, kind: PublicationRequestKind) {
        let pending = format!(
            "{}/pending?worker_id={worker_id}&wait_ms={REMOTE_BROWSE_WAIT_MS}",
            kind.route()
        );
        loop {
            let tasks = tokio::select! {
                _ = crate::api::shutdown_signal() => return,
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
            let response = self
                .request(
                    reqwest::Method::GET,
                    &format!("{prefix}/staged?worker_id={}", task.worker_id),
                )
                .send()
                .await
                .map_err(|_| crate::error::AppError::CentralUnavailable)?
                .error_for_status()
                .map_err(|_| crate::error::AppError::CentralUnavailable)?;
            worker_transfer::save_verified(
                response,
                &file,
                self.config.max_bytes,
                &task.result_sha256,
                task.result_bytes,
            )
            .await
            .map_err(|_| crate::error::AppError::Invalid)?;
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
                    let response = self
                        .request(
                            reqwest::Method::GET,
                            &format!("{prefix}/authorize?worker_id={}", task.worker_id),
                        )
                        .send()
                        .await
                        .map_err(|_| crate::error::AppError::CentralUnavailable)?;
                    if response.status().is_success() {
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
        let _ = self
            .request(reqwest::Method::POST, &format!("{prefix}/report"))
            .timeout(REMOTE_BROWSE_REPORT_TIMEOUT)
            .json(&report)
            .send()
            .await;
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
        Some(tokio::spawn(async move {
            let address = std::net::SocketAddr::from(([0, 0, 0, 0], port));
            match tokio::net::TcpListener::bind(address).await {
                Ok(listener) => {
                    tracing::info!(%address, "serveur de transfert de sources démarré");
                    let _ = axum::serve(listener, worker_transfer::transfer_router(state)).await;
                }
                Err(error) => {
                    tracing::warn!(%address, error = %error, "serveur de transfert indisponible");
                }
            }
        }))
    }

    /// Enregistre le worker puis réclame des tâches jusqu'à l'arrêt.
    async fn work(&self) -> anyhow::Result<()> {
        let runner = self;
        runner.refresh_software_capabilities().await;
        let probe = crate::telemetry::TelemetryProbe::with_storage_path(&runner.config.work_dir);
        let worker = loop {
            let attempt = async {
                runner
                    .request(reqwest::Method::POST, "/api/v1/workers")
                    .timeout(CENTRAL_REQUEST_TIMEOUT)
                    .json(&Register {
                        name: runner.config.name.clone(),
                        version: Some(crate::version::app_version().to_owned()),
                        telemetry: Some(probe.sample()),
                        transfer_url: runner.config.transfer_url.clone(),
                    })
                    .send()
                    .await?
                    .error_for_status()?
                    .json::<Worker>()
                    .await
            };
            tokio::select! {
                result = attempt => match result { Ok(worker) => break worker, Err(_) => tracing::warn!("registration unavailable; retrying") },
                _ = crate::api::shutdown_signal() => return Ok(()),
            }
            tokio::select! { _=tokio::time::sleep(REGISTRATION_RETRY_DELAY)=>{}, _=crate::api::shutdown_signal()=>return Ok(()) }
        };
        tracing::info!(worker_id=%worker.id, "worker registered");
        runner.status.mark_registered();
        runner.status.mark_central_reachable(true);
        runner.purge_orphaned_work_artifacts().await;
        let browse_responder = {
            let browse_runner = runner.clone();
            tokio::spawn(async move {
                browse_runner.respond_to_browse_requests(worker.id).await;
            })
        };
        let transfer_server = runner.spawn_transfer_server(worker.id);
        let source_relay_responder = {
            let relay_runner = runner.clone();
            tokio::spawn(async move {
                relay_runner
                    .respond_to_source_relay_requests(worker.id)
                    .await;
            })
        };
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
                        _ = crate::api::shutdown_signal() => return,
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
        let outcome: anyhow::Result<()> = async {
            let mut was_disconnected = false;
            let mut idle_polls: u32 = 0;
            let mut max_concurrency = worker.max_concurrency.max(1);
            let mut active = tokio::task::JoinSet::new();
            loop {
                while active.try_join_next().is_some() {}
                if idle_polls > 0
                    && idle_polls.is_multiple_of(REREGISTRATION_EVERY_IDLE_POLLS)
                    && let Ok(resp) = runner
                        .request(reqwest::Method::POST, "/api/v1/workers")
                        .timeout(CENTRAL_REQUEST_TIMEOUT)
                        .json(&Register {
                            name: runner.config.name.clone(),
                            version: Some(crate::version::app_version().to_owned()),
                            telemetry: Some(probe.sample()),
                            transfer_url: runner.config.transfer_url.clone(),
                        })
                        .send()
                        .await
                    && let Ok(updated) = resp.json::<Worker>().await
                {
                    max_concurrency = updated.max_concurrency.max(1);
                }
                let telemetry = if idle_polls.is_multiple_of(REREGISTRATION_EVERY_IDLE_POLLS) {
                    Some(probe.sample())
                } else {
                    None
                };
                idle_polls = idle_polls.wrapping_add(1);
                while (active.len() as i32) < max_concurrency {
                    let attempt = async {
                        let claim = runner
                            .request(reqwest::Method::POST, "/api/v1/claims")
                            .timeout(CENTRAL_REQUEST_TIMEOUT)
                            .json(&ClaimRequest {
                                worker_id: worker.id,
                                telemetry: telemetry.clone(),
                            })
                            .send()
                            .await?
                            .error_for_status()?
                            .json::<Option<Claim>>()
                            .await?;
                        Ok::<_, anyhow::Error>(claim)
                    };
                    let claim = tokio::select! {
                        result = attempt => result,
                        _ = crate::api::shutdown_signal() => {
                            active.abort_all();
                            return Ok(());
                        }
                    };
                    runner.status.mark_central_reachable(claim.is_ok());
                    match claim {
                        Ok(Some(mut claim)) => {
                            if was_disconnected {
                                runner.purge_orphaned_work_artifacts().await;
                                was_disconnected = false;
                            }
                            if let Err(error) = crate::flows::unseal_claim_secrets_for_worker(
                                &mut claim,
                                &runner.config.key,
                            ) {
                                tracing::error!(job_id = %claim.job_id, error = %error, "impossible de déchiffrer les secrets du bail");
                                let _ = runner
                                    .request(
                                        reqwest::Method::POST,
                                        &format!("/api/v1/jobs/{}/failure", claim.job_id),
                                    )
                                    .timeout(CENTRAL_REQUEST_TIMEOUT)
                                    .json(&Failure {
                                        worker_id: worker.id,
                                        lease_token: claim.lease_token,
                                        lease_epoch: claim.lease_epoch,
                                        error_code: JobErrorCode::TransferFailed.as_str().to_owned(),
                                    })
                                    .send()
                                    .await;
                            } else {
                                let spawned_runner = runner.clone();
                                let wid = worker.id;
                                active.spawn(async move {
                                    if let Err(err) = spawned_runner.execute(claim, wid).await {
                                        tracing::warn!(error = %err, "worker execute task exited with error");
                                    }
                                });
                            }
                        }
                        Ok(None) => {
                            if was_disconnected {
                                runner.purge_orphaned_work_artifacts().await;
                                was_disconnected = false;
                            }
                            break;
                        }
                        Err(_) => {
                            was_disconnected = true;
                            tracing::warn!("central unavailable; retrying");
                            break;
                        }
                    }
                }
                tokio::select! {
                    _ = tokio::time::sleep(IDLE_POLL_INTERVAL) => {},
                    Some(_) = active.join_next(), if !active.is_empty() => {},
                    _ = crate::api::shutdown_signal() => {
                        active.abort_all();
                        return Ok(());
                    }
                }
            }
        }
        .await;
        browse_responder.abort();
        source_relay_responder.abort();
        source_publication_responder.abort();
        source_inspection_responder.abort();
        software_probe_refresher.abort();
        if let Some(server) = transfer_server {
            server.abort();
        }
        outcome
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

    #[test]
    fn result_error_distinguishes_source_changes_from_other_failures() {
        assert_eq!(
            publication_error_code(&serde_json::json!({"error":{"code":"source_changed"}})),
            JobErrorCode::SourceChanged
        );
        assert_eq!(
            publication_error_code(
                &serde_json::json!({"error":{"code":"lease_expired_or_conflict"}})
            ),
            JobErrorCode::TransferFailed
        );
        assert_eq!(
            publication_error_code(&serde_json::Value::Null),
            JobErrorCode::TransferFailed
        );
    }
}
