use crate::{
    api::{self, AppState},
    domain::LeaseRequest,
    media, repository, storage,
};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use uuid::Uuid;

const IDLE_POLL_INTERVAL: Duration = Duration::from_secs(1);
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(20);

pub async fn run(state: AppState) {
    let worker_name = std::env::var("LOCAL_WORKER_NAME")
        .or_else(|_| std::env::var("HOSTNAME").map(|host| format!("coordinator-{host}")))
        .unwrap_or_else(|_| "coordinator-local".into());
    let worker = match repository::register(&state.pool, &worker_name).await {
        Ok(worker) => worker,
        Err(error) => {
            tracing::error!(error=%error, "local worker registration failed");
            return;
        }
    };
    tracing::info!(worker_id=%worker.id, "local worker registered");

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
    let work_dir = state
        .config
        .data_dir
        .join("staging")
        .join(format!("{}-{}", claim.job_id, claim.lease_token));
    if let Err(error) = tokio::fs::create_dir(&work_dir).await {
        report_failure(state, claim.job_id, &lease, "transfer_failed").await;
        tracing::warn!(job_id=%claim.job_id, error=%error, "local work directory creation failed");
        return;
    }
    let source = match storage::input_path(&state.config.input_dir, &claim.source_name).await {
        Ok(path) => path,
        Err(_) => {
            report_failure(state, claim.job_id, &lease, "source_integrity").await;
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
        let warning = media::remux(&source_snapshot, &output)
            .await
            .map_err(|error| anyhow::anyhow!("remux_failed: {error}"))?;
        crate::api::transfers::publish_staged(
            state,
            claim.job_id,
            &lease,
            &job,
            &source_snapshot,
            &output,
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
        let code = if error.to_string().contains("source_integrity") {
            "source_integrity"
        } else if error.to_string().contains("remux_failed") {
            "remux_failed"
        } else {
            "transfer_failed"
        };
        report_failure(state, claim.job_id, &lease, code).await;
        tracing::warn!(job_id=%claim.job_id, error_code=code, "local processing failed");
    }
    cleanup(&work_dir).await;
}

async fn heartbeat_loop(pool: sqlx::PgPool, job: Uuid, lease: LeaseRequest) -> anyhow::Result<()> {
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

async fn report_failure(state: &AppState, job: Uuid, lease: &LeaseRequest, code: &str) {
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
