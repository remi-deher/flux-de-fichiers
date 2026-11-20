//! Remise vérifiée du fichier de travail entre workers.

use super::{REMOTE_BROWSE_HTTP_TIMEOUT, REMOTE_BROWSE_RETRY_DELAY, WorkerLoop};
use crate::{domain::Claim, storage, worker_transfer};
use std::{
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};
use tokio_util::io::ReaderStream;
use uuid::Uuid;

const HANDOFF_REQUEST_TIMEOUT: Duration = Duration::from_secs(60 * 60);

pub(crate) fn retained_handoff_path(root: &Path, job: Uuid, epoch: i64) -> PathBuf {
    root.join(format!("handoff-{job}-{epoch}"))
}

fn retained_identity(name: &str) -> Option<(Uuid, i64)> {
    let (job, epoch) = name.strip_prefix("handoff-")?.rsplit_once('-')?;
    let epoch = epoch.parse::<i64>().ok()?;
    (epoch >= 0).then_some((Uuid::parse_str(job).ok()?, epoch))
}

/// Ne retire une copie ancienne qu'après confirmation de son inutilité par le central.
pub(crate) async fn prune_retained<F, Fut>(root: &Path, needed: F) -> anyhow::Result<()>
where
    F: Fn(Uuid, i64) -> Fut,
    Fut: std::future::Future<Output = anyhow::Result<bool>>,
{
    prune_retained_older_than(
        root,
        Duration::from_secs(forgearr_shared::relay::HANDOFF_TTL_SECONDS as u64),
        needed,
    )
    .await
}

async fn prune_retained_older_than<F, Fut>(
    root: &Path,
    max_age: Duration,
    needed: F,
) -> anyhow::Result<()>
where
    F: Fn(Uuid, i64) -> Fut,
    Fut: std::future::Future<Output = anyhow::Result<bool>>,
{
    let mut entries = match tokio::fs::read_dir(root).await {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    while let Some(entry) = entries.next_entry().await? {
        if !entry.file_type().await?.is_file() {
            continue;
        }
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        let Some((job, epoch)) = retained_identity(&name) else {
            continue;
        };
        let modified = entry.metadata().await?.modified()?;
        if SystemTime::now()
            .duration_since(modified)
            .unwrap_or_default()
            < max_age
        {
            continue;
        }
        if !needed(job, epoch).await? {
            match tokio::fs::remove_file(entry.path()).await {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    tracing::warn!(job_id=%job, error=%error, "retained handoff cleanup failed")
                }
            }
        }
    }
    Ok(())
}

impl WorkerLoop {
    pub(super) async fn prune_handoffs(&self, worker: Uuid) -> anyhow::Result<()> {
        prune_retained(&self.config.work_dir, |job, epoch| async move {
            Ok(self
                .request(
                    reqwest::Method::GET,
                    &format!("/api/v1/worker/handoffs/{job}/{epoch}/retained?worker_id={worker}"),
                )
                .timeout(REMOTE_BROWSE_HTTP_TIMEOUT)
                .send()
                .await?
                .error_for_status()?
                .json::<bool>()
                .await?)
        })
        .await
    }

    /// Télécharge et vérifie la copie avant de libérer le bail de l'émetteur.
    pub(super) async fn receive_handoff(&self, worker: Uuid) -> anyhow::Result<bool> {
        let offer = self
            .request(
                reqwest::Method::POST,
                &format!("/api/v1/worker/handoffs/claim?worker_id={worker}"),
            )
            .timeout(REMOTE_BROWSE_HTTP_TIMEOUT)
            .send()
            .await?
            .error_for_status()?
            .json::<Option<forgearr_shared::relay::HandoffOffer>>()
            .await?;
        let Some(offer) = offer else {
            return Ok(false);
        };
        let retained = retained_handoff_path(
            &self.config.work_dir,
            offer.job_id,
            offer.source_lease_epoch,
        );
        let temporary = self
            .config
            .work_dir
            .join(format!(".tmp-handoff-{}", Uuid::new_v4()));
        let received = async {
            let response = self
                .request(
                    reqwest::Method::GET,
                    &format!(
                        "/api/v1/jobs/{}/handoff/file?worker_id={worker}",
                        offer.job_id
                    ),
                )
                .header("x-handoff-token", &offer.receipt_token)
                .timeout(HANDOFF_REQUEST_TIMEOUT)
                .send()
                .await?
                .error_for_status()?;
            worker_transfer::save_verified(
                response,
                &temporary,
                self.config.max_bytes,
                &offer.artifact.sha256,
                offer.artifact.bytes,
            )
            .await?;
            tokio::fs::hard_link(&temporary, &retained).await?;
            storage::sync_directory(&self.config.work_dir).await?;
            let ack = forgearr_shared::relay::HandoffAcknowledgement {
                worker_id: worker,
                receipt_token: offer.receipt_token,
                sha256: offer.artifact.sha256,
                bytes: offer.artifact.bytes,
            };
            let deadline = tokio::time::Instant::now()
                + Duration::from_secs(forgearr_shared::relay::HANDOFF_TTL_SECONDS as u64);
            loop {
                let response = self
                    .request(
                        reqwest::Method::POST,
                        &format!("/api/v1/jobs/{}/handoff/ack", offer.job_id),
                    )
                    .timeout(REMOTE_BROWSE_HTTP_TIMEOUT)
                    .json(&ack)
                    .send()
                    .await;
                match response {
                    Ok(response) if response.status().is_success() => break,
                    Ok(response) if response.status().is_client_error() => {
                        response.error_for_status()?;
                    }
                    _ => {}
                }
                anyhow::ensure!(
                    tokio::time::Instant::now() < deadline,
                    "handoff acknowledgement timed out"
                );
                tokio::time::sleep(REMOTE_BROWSE_RETRY_DELAY).await;
            }
            Ok::<(), anyhow::Error>(())
        }
        .await;
        let _ = tokio::fs::remove_file(&temporary).await;
        received?;
        Ok(true)
    }

    /// Conserve le répertoire du traitement jusqu'à la réception vérifiée par la cible.
    pub(super) async fn send_handoff(
        &self,
        claim: &Claim,
        worker: Uuid,
        request: forgearr_shared::relay::RelayRequest,
        working_file: &Path,
        work_dir: &Path,
        warning: bool,
    ) -> anyhow::Result<()> {
        use forgearr_shared::relay::{HandoffArtifact, HandoffBegin, HandoffState};
        let working_file = storage::verified_working_path(working_file, work_dir).await?;
        let name = working_file
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| anyhow::anyhow!("invalid handoff filename"))?
            .to_owned();
        let (sha256, bytes) = storage::fingerprint(&working_file, self.config.max_bytes).await?;
        let begin = HandoffBegin {
            worker_id: worker,
            lease_token: claim.lease_token,
            lease_epoch: claim.lease_epoch,
            request,
            artifact: HandoffArtifact {
                name,
                sha256,
                bytes: i64::try_from(bytes)?,
            },
            warning,
        };
        let path = format!("/api/v1/jobs/{}/handoff", claim.job_id);
        self.request(reqwest::Method::POST, &path)
            .timeout(REMOTE_BROWSE_HTTP_TIMEOUT)
            .json(&begin)
            .send()
            .await?
            .error_for_status()?;
        let file = tokio::fs::File::open(&working_file).await?;
        self.leased(reqwest::Method::PUT, claim, worker, "handoff")
            .header("content-length", bytes)
            .timeout(HANDOFF_REQUEST_TIMEOUT)
            .body(reqwest::Body::wrap_stream(ReaderStream::new(file)))
            .send()
            .await?
            .error_for_status()?;
        let deadline = tokio::time::Instant::now()
            + Duration::from_secs(forgearr_shared::relay::HANDOFF_TTL_SECONDS as u64);
        loop {
            let query = format!(
                "{path}?worker_id={worker}&lease_epoch={}",
                claim.lease_epoch
            );
            let state = self
                .request(reqwest::Method::GET, &query)
                .timeout(REMOTE_BROWSE_HTTP_TIMEOUT)
                .send()
                .await?
                .error_for_status()?
                .json::<HandoffState>()
                .await?;
            match state {
                HandoffState::Acknowledged => return Ok(()),
                HandoffState::Failed => anyhow::bail!("handoff failed"),
                HandoffState::Uploading | HandoffState::Pending | HandoffState::Claimed => {}
            }
            anyhow::ensure!(tokio::time::Instant::now() < deadline, "handoff timed out");
            tokio::time::sleep(REMOTE_BROWSE_RETRY_DELAY).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retained_path_is_scoped_to_job_and_source_lease() {
        let root = Path::new("/work");
        let job = Uuid::nil();
        assert_eq!(
            retained_handoff_path(root, job, 7),
            root.join(format!("handoff-{job}-7"))
        );
        assert_ne!(
            retained_handoff_path(root, job, 7),
            retained_handoff_path(root, job, 8)
        );
        assert_eq!(
            retained_identity(&format!("handoff-{job}-7")),
            Some((job, 7))
        );
        assert!(retained_identity("handoff-not-a-uuid-7").is_none());
        assert!(retained_identity(&format!("handoff-{job}--1")).is_none());
    }

    #[tokio::test]
    async fn cleanup_preserves_needed_or_uncertain_files() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let retained = retained_handoff_path(dir.path(), Uuid::new_v4(), 1);
        tokio::fs::write(&retained, b"data").await?;
        prune_retained_older_than(dir.path(), Duration::ZERO, |_, _| async { Ok(true) }).await?;
        assert!(retained.exists());
        assert!(
            prune_retained_older_than(dir.path(), Duration::ZERO, |_, _| async {
                Err(anyhow::anyhow!("offline"))
            })
            .await
            .is_err()
        );
        assert!(retained.exists());
        prune_retained_older_than(dir.path(), Duration::ZERO, |_, _| async { Ok(false) }).await?;
        assert!(!retained.exists());
        Ok(())
    }
}
