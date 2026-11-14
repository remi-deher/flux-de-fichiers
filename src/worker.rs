use crate::{
    central_client::CentralClient,
    config,
    domain::{Claim, ClaimRequest, Failure, LeaseRequest, ProgressReport, Register, Worker},
    media, storage,
    worker_http::{WorkerState, WorkerWeb},
    worker_settings,
};
use anyhow::Context;
use futures_util::StreamExt;
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
use tokio::io::AsyncWriteExt;
use tokio_util::io::ReaderStream;
use uuid::Uuid;

pub struct WorkerConfig {
    pub url: reqwest::Url,
    pub key: String,
    pub name: String,
    pub work_dir: PathBuf,
    pub tools_cache_dir: PathBuf,
    pub ca: Option<PathBuf>,
    pub max_bytes: u64,
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
        })
    }
}
struct WorkerLoop {
    client: Client,
    config: WorkerConfig,
    status: Arc<WorkerState>,
}
impl WorkerLoop {
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
    }
    async fn heartbeat(&self, claim: &Claim, worker: Uuid) -> anyhow::Result<()> {
        let probe = crate::telemetry::TelemetryProbe::with_storage_path(&self.config.work_dir);
        loop {
            tokio::time::sleep(Duration::from_secs(20)).await;
            self.request(
                reqwest::Method::POST,
                &format!("/api/v1/jobs/{}/heartbeat", claim.job_id),
            )
            .timeout(Duration::from_secs(10))
            .json(&LeaseRequest {
                worker_id: worker,
                lease_token: claim.lease_token,
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
                    .timeout(Duration::from_secs(5))
                    .json(&ProgressReport {
                        worker_id: worker,
                        lease_token,
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
            .timeout(Duration::from_secs(15))
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

    async fn process(
        &self,
        claim: &Claim,
        worker: Uuid,
        dir: &std::path::Path,
    ) -> anyhow::Result<()> {
        let cache_mgr = crate::tools_cache::ToolCacheManager::new(&self.config.tools_cache_dir);
        let probe = match (
            claim.probe_tool_id.as_deref(),
            claim.probe_version.as_deref(),
        ) {
            (Some(tool_id), Some(version)) => Some(crate::plugins::ProbeContract {
                tool_id: tool_id.to_owned(),
                version: version.to_owned(),
            }),
            _ => None,
        };
        let mut wanted = vec![(
            crate::plugins::REMUX_TOOL_ID.to_owned(),
            crate::plugins::REMUX_TOOL_VERSION.to_owned(),
        )];
        if let Some(contract) = probe.as_ref() {
            wanted.push((contract.tool_id.clone(), contract.version.clone()));
        }
        for (tool_id, version) in wanted {
            if cache_mgr.is_ready(&tool_id, &version, crate::tools_cache::host_arch()) {
                continue;
            }
            if let Err(error) = self.install_tool(&cache_mgr, &tool_id, &version).await {
                tracing::warn!(tool = %tool_id, error = %error, "installation de l'outil impossible");
            }
        }
        if !crate::plugins::file_job_may_start(&cache_mgr, probe.as_ref()) {
            anyhow::bail!("probe_unavailable");
        }
        let input = dir.join("source.mkv");
        let output = dir.join("output.mkv");
        let response = self
            .leased(reqwest::Method::GET, claim, worker, "source")
            .send()
            .await?
            .error_for_status()?;
        let mut chunks = response.bytes_stream();
        let mut file = tokio::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&input)
            .await?;
        let mut total = 0;
        while let Some(chunk) = chunks.next().await {
            let chunk = chunk?;
            total += chunk.len() as u64;
            anyhow::ensure!(total <= self.config.max_bytes, "source too large");
            file.write_all(&chunk).await?;
        }
        file.flush().await?;
        drop(file);
        let (hash, bytes) = storage::fingerprint(&input, self.config.max_bytes).await?;
        anyhow::ensure!(
            hash == claim.source_sha256 && bytes as i64 == claim.source_bytes,
            "source_integrity"
        );

        let http_client = self.client.clone();
        let progress_cb = self.progress_callback(claim, worker);
        let (final_output, warning) = if let Some(flow) = claim.frozen_flow.as_ref() {
            let arr = claim.arr.clone().map(crate::flows::ArrContext::from);
            let mut engine = crate::flows::FlowEngine::new(&cache_mgr, &http_client);
            if let Some(ref arr_ctx) = arr {
                engine = engine.with_arr(arr_ctx);
            }
            let outcome = engine
                .execute_flow(flow, claim.job_id, &input, dir, progress_cb)
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
            let warning =
                media::remux_with_progress(&mkvmerge_bin, &input, &output, move |pct, line| {
                    progress_cb(pct, line);
                })
                .await
                .context("remux_failed")?;
            (output, warning)
        };

        let (_, output_size) = storage::fingerprint(&final_output, self.config.max_bytes).await?;
        let body = reqwest::Body::wrap_stream(ReaderStream::new(
            tokio::fs::File::open(&final_output).await?,
        ));
        self.leased(reqwest::Method::PUT, claim, worker, "result")
            .header("content-length", output_size)
            .header("x-remux-warning", warning.to_string())
            .body(body)
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }
    async fn execute(&self, claim: Claim, worker: Uuid) -> anyhow::Result<()> {
        let dir = self
            .config
            .work_dir
            .join(format!("{}-{}", claim.job_id, claim.lease_token));
        tokio::fs::create_dir(&dir).await?;
        // Dropping processing on a lost lease or shutdown kills the mkvmerge child.
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
            let message = error.to_string();
            let code = if message == "source_integrity" || message.contains("source_integrity") {
                "source_integrity"
            } else if message == "remux_failed"
                || message.contains("remux_failed")
                || message.contains("flow_execution_failed")
            {
                "remux_failed"
            } else if message == "probe_unavailable" || message.contains("probe_unavailable") {
                "probe_unavailable"
            } else {
                "transfer_failed"
            };
            tracing::warn!(job_id=%claim.job_id, error_code=code, "processing failed");
            // Do not format reqwest errors: their URLs may contain sensitive context.
            let report = self
                .request(
                    reqwest::Method::POST,
                    &format!("/api/v1/jobs/{}/failure", claim.job_id),
                )
                .timeout(Duration::from_secs(10))
                .json(&Failure {
                    worker_id: worker,
                    lease_token: claim.lease_token,
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
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(3600))
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
    /// Enregistre le worker puis réclame des tâches jusqu'à l'arrêt.
    async fn work(&self) -> anyhow::Result<()> {
        let runner = self;
        let worker = loop {
            let attempt = async {
                runner
                    .request(reqwest::Method::POST, "/api/v1/workers")
                    .timeout(Duration::from_secs(10))
                    .json(&Register {
                        name: runner.config.name.clone(),
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
            tokio::select! { _=tokio::time::sleep(Duration::from_secs(5))=>{}, _=crate::api::shutdown_signal()=>return Ok(()) }
        };
        tracing::info!(worker_id=%worker.id, "worker registered");
        runner.status.mark_registered();
        runner.status.mark_central_reachable(true);
        runner.purge_orphaned_work_artifacts().await;
        let mut was_disconnected = false;
        loop {
            let attempt = async {
                let claim = runner
                    .request(reqwest::Method::POST, "/api/v1/claims")
                    .timeout(Duration::from_secs(10))
                    .json(&ClaimRequest {
                        worker_id: worker.id,
                    })
                    .send()
                    .await?
                    .error_for_status()?
                    .json::<Option<Claim>>()
                    .await?;
                Ok::<_, anyhow::Error>(claim)
            };
            let claim = tokio::select! { result=attempt=>result, _=crate::api::shutdown_signal()=>return Ok(()) };
            runner.status.mark_central_reachable(claim.is_ok());
            match claim {
                Ok(Some(claim)) => {
                    if was_disconnected {
                        runner.purge_orphaned_work_artifacts().await;
                        was_disconnected = false;
                    }
                    runner.execute(claim, worker.id).await?;
                }
                Ok(None) => {
                    if was_disconnected {
                        runner.purge_orphaned_work_artifacts().await;
                        was_disconnected = false;
                    }
                }
                Err(_) => {
                    was_disconnected = true;
                    tracing::warn!("central unavailable; retrying");
                }
            }
            tokio::select! { _=tokio::time::sleep(Duration::from_secs(1))=>{}, _=crate::api::shutdown_signal()=>return Ok(()) }
        }
    }
}
