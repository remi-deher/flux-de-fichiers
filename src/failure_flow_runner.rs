//! Exécution par le central des flux d'échec en attente (ADR 0026) : perte de bail ou échec
//! décidé par le central, quand aucun worker n'a pu exécuter le flux d'échec de la tâche.
//!
//! La tâche reste échouée. Le flux s'exécute sur une copie de la source si le central la voit,
//! sinon sur un fichier vide du même nom (les blocs de notification restent utilisables). Les
//! secrets restent ceux du central : aucun bail n'est émis.

use crate::{
    api::{self, AppState},
    domain::{Claim, Job, JobErrorCode, JobStepEntry},
    flows::FailureInfo,
    job_runner::{self, FlowInput},
    repository,
    worker::startup_purge,
};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Attente entre deux recherches de tâches en attente quand il n'y en a aucune.
const PENDING_POLL_INTERVAL: Duration = Duration::from_secs(5);

/// Préfixe du dossier de travail d'une exécution, sous `staging/`.
const WORK_DIR_PREFIX: &str = "failure-flow-";

/// Boucle du central : prend chaque tâche en attente (passée à `done` avant l'exécution).
pub async fn run(state: AppState) {
    // Un flux interrompu ne sera pas rejoué : son dossier ne sert plus.
    let staging = state.config.data_dir.join("staging");
    startup_purge::purge_matching(&staging, |name, is_dir| {
        is_dir && name.starts_with(WORK_DIR_PREFIX)
    })
    .await;
    loop {
        match repository::take_pending_failure_flow(&state.pool).await {
            Ok(Some((job, claim))) => {
                execute(&state, &job, claim).await;
                continue;
            }
            Ok(None) => {}
            Err(error) => {
                tracing::warn!(error = %error, "flux d'échec en attente illisible");
            }
        }
        tokio::select! {
            _ = tokio::time::sleep(PENDING_POLL_INTERVAL) => {},
            _ = api::shutdown_signal() => return,
        }
    }
}

/// Code et message transmis au flux d'échec (`error.code`, `error.message`).
pub fn failure_code(job: &Job) -> JobErrorCode {
    job.error_code.unwrap_or(JobErrorCode::OperationFailed)
}

async fn execute(state: &AppState, job: &Job, mut claim: Claim) {
    let worker_id = state.local_worker_id.get().copied().unwrap_or_default();
    if let Err(error) = api::populate_claim_contracts(
        &state.pool,
        &state.config,
        &state.catalog_cache,
        worker_id,
        &mut claim,
    )
    .await
    {
        tracing::warn!(job_id = %job.id, error = ?error, "contrats du flux d'échec indisponibles");
        record_not_run(state, job, &format!("contrats indisponibles : {error:?}")).await;
        return;
    }
    let roots = crate::plugins::catalog_roots(&state.pool, &state.config).await;
    let cache = state.tool_cache();
    crate::plugins::ensure_tools_for_requirements(
        &roots,
        &cache,
        &job_runner::collect_wanted_tools(&claim),
        &crate::plugins::SyncPolicy::from_env(),
    )
    .await;
    let registry = state.catalog_cache.operations(&roots).await;
    let work_dir = state
        .config
        .data_dir
        .join("staging")
        .join(format!("{WORK_DIR_PREFIX}{}", job.id));
    let source = match prepare_source(state, job, &claim, &work_dir).await {
        Ok(source) => source,
        Err(error) => {
            tracing::warn!(job_id = %job.id, error = %error, "dossier du flux d'échec indisponible");
            cleanup(&work_dir).await;
            record_not_run(
                state,
                job,
                &format!("dossier de travail indisponible : {error}"),
            )
            .await;
            return;
        }
    };
    let code = failure_code(job);
    let failure = FailureInfo {
        code: code.as_str(),
        message: code.as_str(),
    };
    let progress_state = state.clone();
    let job_id = job.id;
    let input = FlowInput {
        source: &source,
        work_dir: &work_dir,
        progress_cb: std::sync::Arc::new(move |pct: u32, line: &str| {
            progress_state.notify_job_progress(job_id, pct, Some(line.to_owned()));
        }),
    };
    let steps = job_runner::execute_pending_failure_flow(
        &claim,
        &cache,
        &state.http_client,
        Some(&state.config.config_dir),
        Some(&registry),
        failure,
        input,
    )
    .await;
    append_steps(state, job, &steps).await;
    cleanup(&work_dir).await;
}

/// Rend visible un flux d'échec abandonné avant son exécution (étape de synthèse en échec).
async fn record_not_run(state: &AppState, job: &Job, reason: &str) {
    let failed = (job.id, job.lease_epoch);
    log_append(
        job,
        repository::record_failure_flow_not_run(&state.pool, failed, reason).await,
    );
}

async fn append_steps(state: &AppState, job: &Job, steps: &[JobStepEntry]) {
    let failed = (job.id, job.lease_epoch);
    log_append(
        job,
        repository::append_failure_steps(&state.pool, failed, steps).await,
    );
}

fn log_append(job: &Job, appended: crate::error::AppResult<bool>) {
    match appended {
        Ok(true) => {}
        Ok(false) => {
            tracing::warn!(job_id = %job.id, "tâche relancée : journal du flux d'échec abandonné");
        }
        Err(error) => {
            tracing::warn!(job_id = %job.id, error = %error, "journal du flux d'échec non enregistré");
        }
    }
}

/// Copie vérifiée de la source si le central la lit, sinon fichier vide du même nom.
async fn prepare_source(
    state: &AppState,
    job: &Job,
    claim: &Claim,
    work_dir: &Path,
) -> anyhow::Result<PathBuf> {
    // Un dossier laissé par un arrêt brutal est remplacé : l'exécution est déjà comptée.
    cleanup(work_dir).await;
    tokio::fs::create_dir_all(work_dir).await?;
    let snapshot = work_dir.join(forgearr_shared::jobs::source_working_filename(
        &claim.source_name,
    ));
    let central =
        crate::library_scan::central_job_source(&state.pool, &state.config.input_dir, job)
            .await
            .ok()
            .flatten();
    if let Some(path) = central {
        let copied = crate::local_worker::copy_verified_source(
            &path,
            &snapshot,
            state.config.max_file_bytes,
            &claim.source_sha256,
            claim.source_bytes,
        )
        .await;
        match copied {
            Ok(()) => return Ok(snapshot),
            Err(error) => {
                tracing::info!(job_id = %job.id, error = %error, "source inutilisable : flux d'échec sans source");
                let _ = tokio::fs::remove_file(&snapshot).await;
            }
        }
    }
    tokio::fs::File::create(&snapshot).await?;
    Ok(snapshot)
}

async fn cleanup(path: &Path) {
    if let Err(error) = tokio::fs::remove_dir_all(path).await
        && error.kind() != std::io::ErrorKind::NotFound
    {
        tracing::warn!(error = %error, "nettoyage du dossier du flux d'échec impossible");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_job_without_code_reports_an_operation_failure() -> anyhow::Result<()> {
        let mut job: Job = serde_json::from_value(serde_json::json!({
            "id": uuid::Uuid::nil(),
            "source_name": "film.mkv",
            "warning": false,
            "status": "failed",
            "attempts": 1,
            "lease_epoch": 2,
            "priority": 0,
            "created_at": "2026-11-22T00:00:00Z",
            "updated_at": "2026-11-22T00:00:00Z"
        }))?;
        assert_eq!(failure_code(&job), JobErrorCode::OperationFailed);
        job.error_code = Some(JobErrorCode::LeaseExpired);
        assert_eq!(failure_code(&job), JobErrorCode::LeaseExpired);
        Ok(())
    }
}
