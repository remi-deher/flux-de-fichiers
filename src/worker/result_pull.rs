//! Lecture, par le détenteur de la bibliothèque, du résultat à appliquer : chez le worker qui le
//! retient (ADR 0022) ou, pour l'ancien envoi, chez le central.

use super::{REMOTE_BROWSE_HTTP_TIMEOUT, WorkerLoop};
use crate::{
    error::{AppError, AppResult},
    worker_transfer,
};
use forgearr_shared::transfer::{SourcePublicationTask, SourceTicket};
use std::path::Path;
use uuid::Uuid;

impl WorkerLoop {
    /// Lit le résultat de la demande dans `dest` : chez le worker qui le retient quand elle porte un
    /// billet, sinon chez le central. Taille et empreinte annoncées sont vérifiées.
    pub(super) async fn fetch_result(
        &self,
        prefix: &str,
        task: &SourcePublicationTask,
        dest: &Path,
    ) -> AppResult<()> {
        match task.result_ticket.clone() {
            Some(ticket) => self.pull_result(prefix, task, ticket, dest).await,
            None => self.fetch_staged_result(prefix, task, dest).await,
        }
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
        Ok(self
            .request(reqwest::Method::POST, &path)
            .timeout(REMOTE_BROWSE_HTTP_TIMEOUT)
            .send()
            .await?
            .error_for_status()?
            .json::<Option<SourceTicket>>()
            .await?)
    }

    async fn fetch_staged_result(
        &self,
        prefix: &str,
        task: &SourcePublicationTask,
        dest: &Path,
    ) -> AppResult<()> {
        let path = format!("{prefix}/staged?worker_id={}", task.worker_id);
        let response = self
            .request(reqwest::Method::GET, &path)
            .send()
            .await
            .map_err(|_| AppError::CentralUnavailable)?
            .error_for_status()
            .map_err(|_| AppError::CentralUnavailable)?;
        worker_transfer::save_verified(
            response,
            dest,
            self.config.max_bytes,
            &task.result_sha256,
            task.result_bytes,
        )
        .await
        .map_err(|_| AppError::Invalid)
    }
}
