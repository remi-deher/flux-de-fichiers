//! Fichiers retenus chez le worker qui les produit et tirés en direct par un autre worker
//! (ADR 0022) : fichier de travail d'un relais, résultat d'une tâche. Le central ne garde que leur
//! description.

use super::{REMOTE_BROWSE_HTTP_TIMEOUT, WorkerLoop, publication_failure};
use crate::{
    domain::{Claim, JobErrorCode},
    job_runner::JobRunError,
    retained::{prune_retained, retained_name},
    storage, worker_transfer,
};
use forgearr_shared::{
    relay::{HandoffInput, RelayReport, RelayRequest, RelayResponse},
    transfer::{
        ArtifactDeclaration, ArtifactKind, FILE_TRANSFER_TIMEOUT, ResultReady, ResultReadyResponse,
    },
};
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// Actions du protocole de bail pour déclarer le fichier de travail et confirmer sa réception.
const DECLARE_ACTION: &str = "artifacts/working";
const RECEIVED_ACTION: &str = "artifacts/working/received";
/// Action du protocole de bail pour déclarer le résultat retenu, puis l'annoncer au central.
const DECLARE_RESULT_ACTION: &str = "artifacts/result";
const RESULT_READY_ACTION: &str = "result/ready";

impl WorkerLoop {
    /// Purge les fichiers retenus que le central ne réclame plus.
    pub(super) async fn prune_handoffs(&self, worker: Uuid) -> anyhow::Result<()> {
        prune_retained(&self.config.work_dir, |kind, job, epoch| async move {
            let query = format!("worker_id={worker}&lease_epoch={epoch}");
            let path = format!("/api/v1/jobs/{job}/retained/{}?{query}", kind.as_str());
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
        let kind = ArtifactKind::Working;
        let name = retained_name(kind, claim.job_id, claim.lease_epoch, &working_file);
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

    /// Retient le résultat, le déclare, puis demande au central de le faire tirer par le détenteur de
    /// la bibliothèque, ou l'applique lui-même s'il la détient. Le résultat ne passe jamais par le
    /// central : sans détenteur joignable, la tâche échoue.
    pub(super) async fn publish_retained_result(
        &self,
        claim: &Claim,
        worker: Uuid,
        result_file: &Path,
        extension: &str,
        warning: bool,
    ) -> anyhow::Result<()> {
        let (sha256, bytes) = storage::fingerprint(result_file, self.config.max_bytes).await?;
        let kind = ArtifactKind::Result;
        let name = retained_name(kind, claim.job_id, claim.lease_epoch, result_file);
        let retained = self.config.work_dir.join(&name);
        tokio::fs::hard_link(result_file, &retained).await?;
        storage::sync_directory(&self.config.work_dir).await?;
        let declaration = ArtifactDeclaration {
            root: self.config.work_dir.to_string_lossy().into_owned(),
            name,
            sha256,
            bytes: i64::try_from(bytes)?,
        };
        let ready = ResultReady {
            extension: extension.to_owned(),
            warning,
            can_apply_locally: true,
        };
        let announced = self
            .deliver_result(claim, worker, &declaration, &ready, result_file)
            .await;
        // Publié, le fichier ne sert plus ; en cas d'échec il reste jusqu'à la purge, seule copie
        // du résultat.
        if announced.is_ok() {
            let _ = tokio::fs::remove_file(&retained).await;
        }
        announced
    }

    /// Déclare le résultat, puis le livre selon la réponse du central : tiré par un autre détenteur
    /// ou appliqué ici (le worker détient la bibliothèque).
    async fn deliver_result(
        &self,
        claim: &Claim,
        worker: Uuid,
        declaration: &ArtifactDeclaration,
        ready: &ResultReady,
        result_file: &Path,
    ) -> anyhow::Result<()> {
        let announced = self
            .declare_and_announce(claim, worker, declaration, ready)
            .await?;
        let Some(local) = announced.apply_locally else {
            return Ok(());
        };
        let extension = ready.extension.as_str();
        self.apply_locally(claim, worker, &local, result_file, extension)
            .await
    }

    async fn declare_and_announce(
        &self,
        claim: &Claim,
        worker: Uuid,
        declaration: &ArtifactDeclaration,
        ready: &ResultReady,
    ) -> anyhow::Result<ResultReadyResponse> {
        self.leased(reqwest::Method::PUT, claim, worker, DECLARE_RESULT_ACTION)
            .timeout(REMOTE_BROWSE_HTTP_TIMEOUT)
            .json(declaration)
            .send()
            .await?
            .error_for_status()?;
        let response = self
            .leased(reqwest::Method::POST, claim, worker, RESULT_READY_ACTION)
            .timeout(FILE_TRANSFER_TIMEOUT)
            .json(ready)
            .send()
            .await?;
        if !response.status().is_success() {
            return Err(publication_failure(response).await);
        }
        Ok(response.json::<ResultReadyResponse>().await?)
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
