//! Remplacement de la source par le worker qui détient la bibliothèque et traite lui-même la tâche
//! (ADR 0022) : le résultat ne quitte pas le worker, le central tient le journal et confirme.

use super::{
    REMOTE_BROWSE_HTTP_TIMEOUT,
    lease_calls::{CallFailure, TaskLease},
};
use crate::{
    domain::{Claim, JobErrorCode},
    error::{AppError, AppResult},
    job_runner::action_gate::TaskStop,
    source_publication, supervision,
};
use forgearr_shared::transfer::{LocalApplication, SourcePublicationOutcome};
use reqwest::Method;
use std::path::{Path, PathBuf};

/// Actions du protocole de bail : dernière vérification du bail, puis issue du remplacement.
const AUTHORIZE_ACTION: &str = "result/authorize";
const APPLIED_ACTION: &str = "result/applied";

/// Remplacement à mener : source de la tâche, résultat et destination.
pub(super) struct Replacement {
    pub(super) claim: Claim,
    pub(super) local: LocalApplication,
    pub(super) result_file: PathBuf,
    pub(super) extension: String,
}

impl TaskLease {
    /// Applique la politique de sortie sur le montage du worker, puis annonce l'issue au central.
    /// Une issue autre que `Published` rend l'erreur de publication du central.
    ///
    /// Une fois commencé, le remplacement va à son terme même si la tâche est abandonnée : arrêté au
    /// milieu, il laisserait des fichiers partiels à côté de la source.
    pub(super) async fn apply_locally(&self, replacement: Replacement) -> anyhow::Result<()> {
        let lease = self.clone();
        supervision::critical(async move { lease.replace_and_report(&replacement).await }).await?
    }

    async fn replace_and_report(&self, replacement: &Replacement) -> anyhow::Result<()> {
        let applied = self.replace_source(replacement).await;
        if let Some(stop) = self.contact.stop.get() {
            return Err(stop.into());
        }
        let outcome = match applied {
            Ok(()) => SourcePublicationOutcome::Published,
            Err(AppError::SourceChanged) => SourcePublicationOutcome::SourceChanged,
            Err(_) => SourcePublicationOutcome::Failed,
        };
        let reported = self
            .call(self.budget(REMOTE_BROWSE_HTTP_TIMEOUT), || {
                self.leased(Method::POST, APPLIED_ACTION).json(&outcome)
            })
            .await;
        match reported {
            Ok(_) => Ok(()),
            // Le central a inscrit l'échec en même temps qu'il a retiré le journal.
            Err(CallFailure::Refused(code))
                if self.supports_reachability()
                    && code.as_deref() == Some(JobErrorCode::SourceChanged.as_str()) =>
            {
                Err(TaskStop::Concluded.into())
            }
            Err(failure) => Err(failure.into()),
        }
    }

    async fn replace_source(&self, replacement: &Replacement) -> AppResult<()> {
        let Replacement {
            claim,
            local,
            result_file,
            extension,
        } = replacement;
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
            self.runner.config.max_bytes,
            self.authorize_replacement(),
        )
        .await
    }

    /// Dernière vérification du bail auprès du central, juste avant de modifier la source. Attend un
    /// central injoignable : le temporaire est prêt, rien n'est encore modifié.
    async fn authorize_replacement(&self) -> AppResult<()> {
        let granted = self
            .call(self.budget(REMOTE_BROWSE_HTTP_TIMEOUT), || {
                self.leased(Method::POST, AUTHORIZE_ACTION)
            })
            .await;
        match granted {
            Ok(_) => Ok(()),
            Err(CallFailure::Stop(stop)) => {
                self.contact.stop.record(stop);
                Err(AppError::LeaseLost)
            }
            Err(_) => Err(AppError::CentralUnavailable),
        }
    }
}
