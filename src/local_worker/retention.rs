//! Fichiers que le worker intégré retient pour que d'autres workers les tirent chez lui (ADR 0022) :
//! fichier de travail d'un relais, résultat d'une tâche. Même mécanique qu'un worker distant, sans
//! appel réseau : le central est dans le même processus.

use crate::{
    api::{self, AppState},
    domain::{Job, LeaseRequest},
    error::{AppError, AppResult},
    repository,
    retained::{prune_retained, retained_name},
    storage,
};
use forgearr_shared::{
    relay::RelayRequest,
    transfer::{ArtifactDeclaration, ArtifactKind, ResultReady},
};
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// Sous-dossier de `data_dir` des fichiers retenus du worker intégré.
const RETAINED_DIR: &str = "retained";

fn retained_dir(state: &AppState) -> PathBuf {
    state.config.data_dir.join(RETAINED_DIR)
}

/// Retient `file` sous le nom de son artefact, puis le déclare au bail.
async fn retain(
    state: &AppState,
    job: Uuid,
    lease: &LeaseRequest,
    kind: ArtifactKind,
    file: &Path,
) -> AppResult<PathBuf> {
    let dir = retained_dir(state);
    tokio::fs::create_dir_all(&dir).await?;
    let name = retained_name(kind, job, lease.lease_epoch, file);
    let retained = dir.join(&name);
    tokio::fs::hard_link(file, &retained).await?;
    storage::sync_directory(&dir).await?;
    let max = state.config.max_file_bytes;
    let (sha256, bytes) = storage::fingerprint(&retained, max).await?;
    let declaration = ArtifactDeclaration {
        root: dir.to_string_lossy().into_owned(),
        name,
        sha256,
        bytes: i64::try_from(bytes).map_err(|_| AppError::TooLarge)?,
    };
    repository::declare_artifact(&state.pool, job, lease, kind, &declaration, max).await?;
    Ok(retained)
}

/// Livre le résultat par tirage direct quand un autre worker détient la bibliothèque. `false` quand
/// il doit être appliqué ici (source du central, dossier de sortie) ou que ce worker n'annonce pas
/// d'adresse de transfert : l'appelant publie alors lui-même.
pub(crate) async fn publish_retained(
    state: &AppState,
    job: &Job,
    lease: &LeaseRequest,
    file: &Path,
    ready: &ResultReady,
) -> anyhow::Result<bool> {
    if !repository::can_serve(&state.pool, lease.worker_id).await? {
        return Ok(false);
    }
    let destination = api::result_ready::pulled_destination(state, job, lease.worker_id).await?;
    if destination.is_none() {
        return Ok(false);
    }
    let retained = retain(state, job.id, lease, ArtifactKind::Result, file).await?;
    api::result_ready::publish_pulled(state, job, lease, ready).await?;
    let _ = tokio::fs::remove_file(retained).await;
    Ok(true)
}

/// Retient le fichier de travail puis relaie la tâche : le repreneur le tire chez ce worker.
pub(crate) async fn relay_working_file(
    state: &AppState,
    job: Uuid,
    lease: &LeaseRequest,
    request: &RelayRequest,
    working: (&Path, &Path),
    warning: bool,
) -> anyhow::Result<()> {
    if !repository::can_serve(&state.pool, lease.worker_id).await? {
        anyhow::bail!("local worker announces no transfer address: cannot retain a working file");
    }
    let (working_file, work_dir) = working;
    let file = storage::verified_working_path(working_file, work_dir).await?;
    let retained = retain(state, job, lease, ArtifactKind::Working, &file).await?;
    let relayed = api::jobs::relay_task(state, job, lease, request, warning).await;
    if !matches!(&relayed, Ok(response) if response.relayed) {
        let _ = tokio::fs::remove_file(retained).await;
    }
    anyhow::ensure!(relayed?.relayed, "relay refused: no eligible worker");
    Ok(())
}

/// Purge les fichiers retenus que le central ne réclame plus.
pub(crate) async fn prune(state: &AppState, worker: Uuid) {
    let dir = retained_dir(state);
    let pool = &state.pool;
    let pruned = prune_retained(&dir, |kind, job, epoch| async move {
        Ok(repository::artifact_retained(pool, job, kind, worker, epoch).await?)
    })
    .await;
    if let Err(error) = pruned {
        tracing::warn!(error = %error, "purge des fichiers retenus du worker intégré impossible");
    }
}
