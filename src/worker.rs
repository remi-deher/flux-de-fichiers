use crate::{
    config,
    domain::{Claim, ClaimRequest, Failure, LeaseRequest, Register, Worker},
    media, storage,
};
use anyhow::Context;
use futures_util::StreamExt;
use reqwest::{Client, RequestBuilder};
use std::{env, path::PathBuf, time::Duration};
use tokio::io::AsyncWriteExt;
use tokio_util::io::ReaderStream;
use uuid::Uuid;

pub struct WorkerConfig {
    pub url: reqwest::Url,
    pub key: String,
    pub name: String,
    pub work_dir: PathBuf,
    pub ca: Option<PathBuf>,
    pub max_bytes: u64,
}
impl WorkerConfig {
    pub fn load() -> anyhow::Result<Self> {
        Ok(Self {
            url: config::coordinator_url(&env::var("COORDINATOR_URL")?)?,
            key: config::secret("WORKER_KEY")?,
            name: env::var("WORKER_NAME").or_else(|_| env::var("HOSTNAME"))?,
            work_dir: env::var("WORK_DIR")
                .unwrap_or_else(|_| "var/work".into())
                .into(),
            ca: env::var("CA_CERT").ok().map(Into::into),
            max_bytes: env::var("MAX_FILE_BYTES")
                .unwrap_or_else(|_| "536870912".into())
                .parse()?,
        })
    }
}
struct Agent {
    client: Client,
    config: WorkerConfig,
}
impl Agent {
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
            })
            .send()
            .await?
            .error_for_status()?;
        }
    }
    async fn process(
        &self,
        claim: &Claim,
        worker: Uuid,
        dir: &std::path::Path,
    ) -> anyhow::Result<()> {
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
        let warning = media::remux(&input, &output)
            .await
            .context("remux_failed")?;
        let (_, output_size) = storage::fingerprint(&output, self.config.max_bytes).await?;
        let body =
            reqwest::Body::wrap_stream(ReaderStream::new(tokio::fs::File::open(&output).await?));
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
            let code = if error.to_string() == "source_integrity" {
                "source_integrity"
            } else if error.to_string() == "remux_failed" {
                "remux_failed"
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
}

pub async fn run(config: WorkerConfig) -> anyhow::Result<()> {
    anyhow::ensure!(
        !config.name.is_empty()
            && config.name.len() <= 80
            && config
                .name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c)),
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
    let agent = Agent {
        client: builder.build()?,
        config,
    };
    let worker = loop {
        let attempt = async {
            agent
                .request(reqwest::Method::POST, "/api/v1/workers")
                .timeout(Duration::from_secs(10))
                .json(&Register {
                    name: agent.config.name.clone(),
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
    loop {
        let attempt = async {
            let claim = agent
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
        match claim {
            Ok(Some(claim)) => agent.execute(claim, worker.id).await?,
            Ok(None) => {}
            Err(_) => tracing::warn!("coordinator unavailable; retrying"),
        }
        tokio::select! { _=tokio::time::sleep(Duration::from_secs(1))=>{}, _=crate::api::shutdown_signal()=>return Ok(()) }
    }
}
