//! Fichiers retenus chez le worker qui les produit et tirés en direct par un autre worker
//! (ADR 0022) : fichier de travail d'un relais, résultat d'une tâche. Le central ne garde que leur
//! description.

use super::{
    REMOTE_BROWSE_HTTP_TIMEOUT, WorkerLoop,
    lease_calls::{CallBudget, TaskLease},
    local_apply::Replacement,
    timing::{RESULT_READY_ATTEMPT, RESULT_READY_OVERALL},
};
use crate::{
    domain::{Claim, JobErrorCode},
    job_runner::{JobRunError, action_gate::TaskStop},
    retained::{prune_retained, retained_name},
    storage, worker_transfer,
};
use forgearr_shared::{
    relay::{HandoffInput, RelayReport, RelayRequest, RelayResponse},
    transfer::{
        ArtifactDeclaration, ArtifactKind, FILE_TRANSFER_TIMEOUT, ResultReady, ResultReadyResponse,
        SourceTicket,
    },
};
use reqwest::Method;
use std::{path::Path, time::Duration};
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
}

/// Fichier de sortie d'une tâche, à publier.
pub(super) struct TaskResult<'a> {
    pub(super) file: &'a Path,
    pub(super) extension: &'a str,
    pub(super) warning: bool,
}

impl TaskLease {
    /// Retient `file` sous le nom de son artefact, dans le dossier de travail du worker.
    async fn retain(
        &self,
        kind: ArtifactKind,
        claim: &Claim,
        file: &Path,
    ) -> anyhow::Result<ArtifactDeclaration> {
        let config = &self.runner.config;
        let (sha256, bytes) = storage::fingerprint(file, config.max_bytes).await?;
        let name = retained_name(kind, claim.job_id, claim.lease_epoch, file);
        tokio::fs::hard_link(file, config.work_dir.join(&name)).await?;
        storage::sync_directory(&config.work_dir).await?;
        Ok(ArtifactDeclaration {
            root: config.work_dir.to_string_lossy().into_owned(),
            name,
            sha256,
            bytes: i64::try_from(bytes)?,
        })
    }

    async fn declare(&self, action: &str, declaration: &ArtifactDeclaration) -> anyhow::Result<()> {
        self.call(self.budget(REMOTE_BROWSE_HTTP_TIMEOUT), || {
            self.leased(Method::PUT, action).json(declaration)
        })
        .await?;
        Ok(())
    }

    async fn discard(&self, declaration: &ArtifactDeclaration) {
        let retained = self.runner.config.work_dir.join(&declaration.name);
        let _ = tokio::fs::remove_file(retained).await;
    }

    /// Retient le fichier de travail, le déclare au central, puis relaie la tâche : le repreneur le
    /// tire en direct chez ce worker, qui n'a plus à garder son bail.
    ///
    /// Le fichier retenu n'est supprimé que si le central a refusé le relais : une issue inconnue peut
    /// être un relais réussi, et la purge périodique demande au central.
    pub(super) async fn send_handoff(
        &self,
        claim: &Claim,
        request: RelayRequest,
        working_file: &Path,
        work_dir: &Path,
        warning: bool,
    ) -> anyhow::Result<()> {
        let working_file = storage::verified_working_path(working_file, work_dir).await?;
        let declaration = self
            .retain(ArtifactKind::Working, claim, &working_file)
            .await?;
        self.declare(DECLARE_ACTION, &declaration).await?;
        let lease = self.request_body();
        let report = RelayReport {
            worker_id: lease.worker_id,
            lease_token: lease.lease_token,
            lease_epoch: lease.lease_epoch,
            request,
            warning,
        };
        let response = self
            .call(self.budget(REMOTE_BROWSE_HTTP_TIMEOUT), || {
                self.request(Method::POST, "relay").json(&report)
            })
            .await?
            .json::<RelayResponse>()?;
        if response.relayed {
            return Ok(());
        }
        // Aucun worker capable : le central a déjà inscrit l'échec.
        self.discard(&declaration).await;
        Err(TaskStop::Concluded.into())
    }

    /// Retient le résultat, le déclare, puis demande au central de le faire tirer par le détenteur de
    /// la bibliothèque, ou l'applique lui-même s'il la détient. Le résultat ne passe jamais par le
    /// central : sans détenteur joignable, la tâche échoue.
    pub(super) async fn publish_retained_result(
        &self,
        claim: &Claim,
        result: TaskResult<'_>,
    ) -> anyhow::Result<()> {
        let declaration = self
            .retain(ArtifactKind::Result, claim, result.file)
            .await?;
        self.declare(DECLARE_RESULT_ACTION, &declaration).await?;
        let ready = ResultReady {
            extension: result.extension.to_owned(),
            warning: result.warning,
            can_apply_locally: true,
        };
        let announced = self
            .call(self.ready_budget(), || {
                self.leased(Method::POST, RESULT_READY_ACTION).json(&ready)
            })
            .await?
            .json::<ResultReadyResponse>()?;
        if let Some(local) = announced.apply_locally {
            self.apply_locally(Replacement {
                claim: claim.clone(),
                local,
                result_file: result.file.to_owned(),
                extension: ready.extension,
            })
            .await?;
        }
        // Publié, le fichier ne sert plus ; en cas d'échec il reste jusqu'à la purge, seule copie
        // du résultat.
        self.discard(&declaration).await;
        Ok(())
    }

    /// Une annonce répétée rejoint la publication en cours ; un ancien central l'annulerait.
    fn ready_budget(&self) -> CallBudget {
        if self.supports_reachability() {
            CallBudget {
                attempt: RESULT_READY_ATTEMPT,
                overall: RESULT_READY_OVERALL,
            }
        } else {
            CallBudget {
                attempt: FILE_TRANSFER_TIMEOUT,
                overall: Duration::ZERO,
            }
        }
    }

    /// Tire le fichier de travail chez son détenteur, le vérifie, puis confirme la réception au
    /// central : le détenteur peut alors purger sa copie.
    pub(super) async fn pull_handoff_input(
        &self,
        claim: &Claim,
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
            self.runner.config.max_bytes,
            &input.artifact.sha256,
            input.artifact.bytes,
            || self.reissue_ticket(),
        )
        .await
        .map_err(|error| {
            if error.to_string() == worker_transfer::SOURCE_INTEGRITY {
                JobRunError::simple(JobErrorCode::SourceIntegrity, "handoff_integrity").into()
            } else {
                error
            }
        })?;
        self.call(self.budget(REMOTE_BROWSE_HTTP_TIMEOUT), || {
            self.leased(Method::POST, RECEIVED_ACTION)
        })
        .await?;
        Ok(())
    }

    /// Demande au central un billet frais pour la source de cette tâche (bail actif exigé) ; attend le
    /// retour d'un central injoignable, pour reprendre un téléchargement interrompu par la coupure.
    pub(super) async fn reissue_ticket(&self) -> anyhow::Result<Option<SourceTicket>> {
        self.call(self.budget(REMOTE_BROWSE_HTTP_TIMEOUT), || {
            self.leased(Method::POST, "source/ticket")
        })
        .await?
        .json()
    }
}
