//! Remplacement de la source par le worker qui détient la bibliothèque et traite lui-même la tâche
//! (ADR 0022) : le résultat ne quitte pas le worker, le central tient le journal et confirme.

use super::{REMOTE_BROWSE_HTTP_TIMEOUT, WorkerLoop, publication_failure};
use crate::{
    domain::Claim,
    error::{AppError, AppResult},
    source_publication,
};
use forgearr_shared::transfer::{LocalApplication, SourcePublicationOutcome};
use std::path::Path;
use uuid::Uuid;

/// Actions du protocole de bail : dernière vérification du bail, puis issue du remplacement.
const AUTHORIZE_ACTION: &str = "result/authorize";
const APPLIED_ACTION: &str = "result/applied";

impl WorkerLoop {
    /// Applique la politique de sortie sur le montage du worker, puis annonce l'issue au central.
    /// Une issue autre que `Published` rend l'erreur de publication du central.
    pub(super) async fn apply_locally(
        &self,
        claim: &Claim,
        worker: Uuid,
        local: &LocalApplication,
        result_file: &Path,
        extension: &str,
    ) -> anyhow::Result<()> {
        let applied = self
            .replace_source(claim, worker, local, result_file, extension)
            .await;
        let outcome = match applied {
            Ok(()) => SourcePublicationOutcome::Published,
            Err(AppError::SourceChanged) => SourcePublicationOutcome::SourceChanged,
            Err(_) => SourcePublicationOutcome::Failed,
        };
        let response = self
            .leased(reqwest::Method::POST, claim, worker, APPLIED_ACTION)
            .timeout(REMOTE_BROWSE_HTTP_TIMEOUT)
            .json(&outcome)
            .send()
            .await?;
        if !response.status().is_success() {
            return Err(publication_failure(response).await);
        }
        Ok(())
    }

    async fn replace_source(
        &self,
        claim: &Claim,
        worker: Uuid,
        local: &LocalApplication,
        result_file: &Path,
        extension: &str,
    ) -> AppResult<()> {
        let source = crate::library_scan::source_path(Path::new(&local.root), &claim.source_name)
            .await
            .map_err(source_publication::missing_source_is_changed)?;
        source_publication::apply_library_output_policy(
            local.policy,
            &source,
            result_file,
            extension,
            &claim.source_sha256,
            claim.source_bytes,
            self.config.max_bytes,
            self.authorize_replacement(claim, worker),
        )
        .await
    }

    /// Dernière vérification du bail auprès du central, juste avant de modifier la source.
    async fn authorize_replacement(&self, claim: &Claim, worker: Uuid) -> AppResult<()> {
        let response = self
            .leased(reqwest::Method::POST, claim, worker, AUTHORIZE_ACTION)
            .timeout(REMOTE_BROWSE_HTTP_TIMEOUT)
            .send()
            .await
            .map_err(|_| AppError::CentralUnavailable)?;
        if response.status().is_success() {
            Ok(())
        } else {
            Err(AppError::CentralUnavailable)
        }
    }
}
