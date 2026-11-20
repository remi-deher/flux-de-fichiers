//! Fichier de travail retenu chez le worker qui le produit et tiré en direct par le repreneur
//! (ADR 0022) : le central ne garde que sa description.

use super::{REMOTE_BROWSE_HTTP_TIMEOUT, WorkerLoop};
use crate::{
    domain::{Claim, JobErrorCode},
    job_runner::JobRunError,
    storage, worker_transfer,
};
use forgearr_shared::{
    relay::{HANDOFF_TTL, HandoffInput, RelayReport, RelayRequest, RelayResponse},
    transfer::ArtifactDeclaration,
};
use std::{
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};
use uuid::Uuid;

const RETAINED_PREFIX: &str = "handoff-";
/// Actions du protocole de bail pour déclarer le fichier de travail et confirmer sa réception.
const DECLARE_ACTION: &str = "artifacts/working";
const RECEIVED_ACTION: &str = "artifacts/working/received";

/// Nom du fichier retenu : `handoff-<tâche>-<époque>` suivi de l'extension du fichier de travail,
/// que les sondes et les blocs du flux lisent.
fn retained_handoff_name(job: Uuid, epoch: i64, working_file: &Path) -> String {
    let base = format!("{RETAINED_PREFIX}{job}-{epoch}");
    let extension = working_file.extension().and_then(|ext| ext.to_str());
    match extension.filter(|ext| storage::valid_output_extension(ext)) {
        Some(extension) => format!("{base}.{extension}"),
        None => base,
    }
}

fn retained_identity(name: &str) -> Option<(Uuid, i64)> {
    let stem = name.strip_prefix(RETAINED_PREFIX)?.split('.').next()?;
    let (job, epoch) = stem.rsplit_once('-')?;
    let epoch = epoch.parse::<i64>().ok()?;
    (epoch >= 0).then_some((Uuid::parse_str(job).ok()?, epoch))
}

/// Ne retire une copie ancienne qu'après confirmation de son inutilité par le central.
async fn prune_retained<F, Fut>(root: &Path, needed: F) -> anyhow::Result<()>
where
    F: Fn(Uuid, i64) -> Fut,
    Fut: std::future::Future<Output = anyhow::Result<bool>>,
{
    prune_retained_older_than(root, HANDOFF_TTL, needed).await
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
    /// Purge les fichiers de travail que le central ne réclame plus.
    pub(super) async fn prune_handoffs(&self, worker: Uuid) -> anyhow::Result<()> {
        prune_retained(&self.config.work_dir, |job, epoch| async move {
            let query = format!("worker_id={worker}&lease_epoch={epoch}");
            let path = format!("/api/v1/jobs/{job}/artifacts/working/retained?{query}");
            Ok(self
                .request(reqwest::Method::GET, &path)
                .timeout(REMOTE_BROWSE_HTTP_TIMEOUT)
                .send()
                .await?
                .error_for_status()?
                .json::<bool>()
                .await?)
        })
        .await
    }

    /// Retient le fichier de travail, le déclare au central, puis relaie la tâche : le repreneur le
    /// tire en direct chez ce worker, qui n'a plus à garder son bail.
    pub(super) async fn send_handoff(
        &self,
        claim: &Claim,
        worker: Uuid,
        request: RelayRequest,
        working_file: &Path,
        work_dir: &Path,
        warning: bool,
    ) -> anyhow::Result<()> {
        let working_file = storage::verified_working_path(working_file, work_dir).await?;
        let (sha256, bytes) = storage::fingerprint(&working_file, self.config.max_bytes).await?;
        let name = retained_handoff_name(claim.job_id, claim.lease_epoch, &working_file);
        let retained: PathBuf = self.config.work_dir.join(&name);
        tokio::fs::hard_link(&working_file, &retained).await?;
        storage::sync_directory(&self.config.work_dir).await?;
        let declaration = ArtifactDeclaration {
            root: self.config.work_dir.to_string_lossy().into_owned(),
            name,
            sha256,
            bytes: i64::try_from(bytes)?,
        };
        let relayed = self
            .declare_and_relay(claim, worker, &declaration, request, warning)
            .await;
        if relayed.is_err() {
            let _ = tokio::fs::remove_file(&retained).await;
        }
        relayed
    }

    async fn declare_and_relay(
        &self,
        claim: &Claim,
        worker: Uuid,
        declaration: &ArtifactDeclaration,
        request: RelayRequest,
        warning: bool,
    ) -> anyhow::Result<()> {
        self.leased(reqwest::Method::PUT, claim, worker, DECLARE_ACTION)
            .timeout(REMOTE_BROWSE_HTTP_TIMEOUT)
            .json(declaration)
            .send()
            .await?
            .error_for_status()?;
        let report = RelayReport {
            worker_id: worker,
            lease_token: claim.lease_token,
            lease_epoch: claim.lease_epoch,
            request,
            warning,
        };
        let response = self
            .request(
                reqwest::Method::POST,
                &format!("/api/v1/jobs/{}/relay", claim.job_id),
            )
            .timeout(REMOTE_BROWSE_HTTP_TIMEOUT)
            .json(&report)
            .send()
            .await?
            .error_for_status()?
            .json::<RelayResponse>()
            .await?;
        anyhow::ensure!(response.relayed, "relay refused: no eligible worker");
        Ok(())
    }

    /// Tire le fichier de travail chez son détenteur, le vérifie, puis confirme la réception au
    /// central : le détenteur peut alors purger sa copie.
    pub(super) async fn pull_handoff_input(
        &self,
        claim: &Claim,
        worker: Uuid,
        input: &HandoffInput,
        dest: &Path,
    ) -> anyhow::Result<()> {
        let ticket = claim
            .source_ticket
            .clone()
            .ok_or_else(|| anyhow::anyhow!("no ticket to pull the working file"))?;
        let direct = worker_transfer::direct_client()?;
        worker_transfer::download_direct_resuming(
            &direct,
            ticket,
            dest,
            self.config.max_bytes,
            &input.artifact.sha256,
            input.artifact.bytes,
            || self.reissue_ticket(claim, worker),
        )
        .await
        .map_err(|error| {
            if error.to_string() == worker_transfer::SOURCE_INTEGRITY {
                JobRunError::simple(JobErrorCode::SourceIntegrity, "handoff_integrity").into()
            } else {
                error
            }
        })?;
        self.leased(reqwest::Method::POST, claim, worker, RECEIVED_ACTION)
            .timeout(REMOTE_BROWSE_HTTP_TIMEOUT)
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retained_names_are_scoped_to_job_and_lease_and_keep_the_extension() {
        let job = Uuid::nil();
        let with_extension = Path::new("out.mkv");
        assert_eq!(
            retained_handoff_name(job, 7, with_extension),
            format!("handoff-{job}-7.mkv")
        );
        assert_eq!(
            retained_handoff_name(job, 7, Path::new("out")),
            format!("handoff-{job}-7")
        );
        assert_ne!(
            retained_handoff_name(job, 7, with_extension),
            retained_handoff_name(job, 8, with_extension)
        );
        // Une extension qui n'en est pas une n'entre pas dans le nom.
        let odd = Path::new("out.m v");
        assert_eq!(
            retained_handoff_name(job, 7, odd),
            format!("handoff-{job}-7")
        );
    }

    #[test]
    fn retained_names_give_back_their_job_and_lease() {
        let job = Uuid::nil();
        assert_eq!(
            retained_identity(&format!("handoff-{job}-7")),
            Some((job, 7))
        );
        assert_eq!(
            retained_identity(&format!("handoff-{job}-7.mkv")),
            Some((job, 7))
        );
        assert!(retained_identity("handoff-not-a-uuid-7").is_none());
        assert!(retained_identity(&format!("handoff-{job}--1")).is_none());
        assert!(retained_identity("autre-fichier.mkv").is_none());
    }

    #[tokio::test]
    async fn cleanup_preserves_needed_or_uncertain_files() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let name = retained_handoff_name(Uuid::new_v4(), 1, Path::new("w.mkv"));
        let retained = dir.path().join(name);
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
