//! Lecture, par le détenteur de la bibliothèque, du résultat à appliquer : chez le worker qui le
//! retient, avec le billet de la demande (ADR 0022).

use super::{
    REMOTE_BROWSE_HTTP_TIMEOUT, WorkerLoop, contact::CentralReply,
    timing::PUBLICATION_REPORT_PATIENCE,
};
use crate::{
    error::{AppError, AppResult},
    worker_transfer,
};
use forgearr_shared::transfer::{SourcePublicationTask, SourceTicket};
use std::path::Path;
use uuid::Uuid;

impl WorkerLoop {
    /// Lit le résultat de la demande dans `dest`, chez le worker qui le retient. Taille et empreinte
    /// annoncées sont vérifiées.
    pub(super) async fn fetch_result(
        &self,
        prefix: &str,
        task: &SourcePublicationTask,
        dest: &Path,
    ) -> AppResult<()> {
        let ticket = task.result_ticket.clone().ok_or(AppError::Invalid)?;
        self.pull_result(prefix, task, ticket, dest).await
    }

    async fn pull_result(
        &self,
        prefix: &str,
        task: &SourcePublicationTask,
        ticket: SourceTicket,
        dest: &Path,
    ) -> AppResult<()> {
        let direct = worker_transfer::direct_client().map_err(|_| AppError::CentralUnavailable)?;
        worker_transfer::download_direct_resuming(
            &direct,
            ticket,
            dest,
            self.config.max_bytes,
            &task.result_sha256,
            task.result_bytes,
            || self.reissue_result_ticket(prefix, task.worker_id),
        )
        .await
        .map_err(|_| AppError::Invalid)
    }

    /// Demande au central un billet frais pour tirer le résultat (reprise d'un long transfert).
    async fn reissue_result_ticket(
        &self,
        prefix: &str,
        worker_id: Uuid,
    ) -> anyhow::Result<Option<SourceTicket>> {
        let path = format!("{prefix}/ticket?worker_id={worker_id}");
        let attempt = self
            .attempt_for(PUBLICATION_REPORT_PATIENCE, || {
                self.request(reqwest::Method::POST, &path)
                    .timeout(REMOTE_BROWSE_HTTP_TIMEOUT)
            })
            .await;
        anyhow::ensure!(
            attempt.reply == CentralReply::Accepted,
            "result ticket refused"
        );
        attempt.json()
    }
}
