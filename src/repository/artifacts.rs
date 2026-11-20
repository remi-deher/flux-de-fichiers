//! Artefacts retenus chez un worker (ADR 0022) : le central n'en garde que la description, les
//! octets restent chez le détenteur et sont tirés en direct par le preneur.

use crate::{
    domain::{JobStatus, LeaseRequest},
    error::{AppError, AppResult},
    opaque_token::is_valid_sha256_hex,
};
use forgearr_shared::transfer::{ArtifactDeclaration, ArtifactKind};
use sqlx::SqlitePool;
use std::path::{Component, Path};
use uuid::Uuid;

/// Longueur maximale du nom d'un artefact (limite courante des systèmes de fichiers).
const ARTIFACT_NAME_MAX_LEN: usize = 255;
/// Longueur maximale de la racine annoncée par le détenteur.
const ARTIFACT_ROOT_MAX_LEN: usize = 1024;

/// Artefact tel que le central le connaît.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JobArtifact {
    pub holder_worker_id: Uuid,
    /// Époque du bail sous lequel le détenteur l'a déclaré.
    pub lease_epoch: i64,
    pub declaration: ArtifactDeclaration,
}

/// Un nom d'artefact est un seul composant de chemin, sans séparateur ni caractère de contrôle.
pub fn valid_artifact_name(name: &str) -> bool {
    let mut components = Path::new(name).components();
    matches!(components.next(), Some(Component::Normal(_)))
        && components.next().is_none()
        && name.len() <= ARTIFACT_NAME_MAX_LEN
        && !name.chars().any(char::is_control)
}

fn valid_declaration(declaration: &ArtifactDeclaration, max_bytes: u64) -> bool {
    !declaration.root.is_empty()
        && declaration.root.len() <= ARTIFACT_ROOT_MAX_LEN
        && !declaration.root.chars().any(char::is_control)
        && valid_artifact_name(&declaration.name)
        && is_valid_sha256_hex(&declaration.sha256)
        && u64::try_from(declaration.bytes).is_ok_and(|bytes| bytes > 0 && bytes <= max_bytes)
}

/// Le worker qui détient le bail déclare (ou remplace) un artefact qu'il retient pour ce job.
pub async fn declare_artifact(
    pool: &SqlitePool,
    job_id: Uuid,
    lease: &LeaseRequest,
    kind: ArtifactKind,
    declaration: &ArtifactDeclaration,
    max_bytes: u64,
) -> AppResult<()> {
    if !valid_declaration(declaration, max_bytes) {
        return Err(AppError::Invalid);
    }
    super::active(pool, job_id, lease).await?;
    sqlx::query(
        "INSERT INTO job_artifacts(job_id, kind, holder_worker_id, lease_epoch, root, name, \
             sha256, bytes) \
         VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8) \
         ON CONFLICT(job_id, kind) DO UPDATE SET holder_worker_id=excluded.holder_worker_id, \
             lease_epoch=excluded.lease_epoch, root=excluded.root, name=excluded.name, \
             sha256=excluded.sha256, bytes=excluded.bytes, created_at=datetime('now')",
    )
    .bind(job_id)
    .bind(kind.as_str())
    .bind(lease.worker_id)
    .bind(lease.lease_epoch)
    .bind(&declaration.root)
    .bind(&declaration.name)
    .bind(declaration.sha256.to_ascii_lowercase())
    .bind(declaration.bytes)
    .execute(pool)
    .await?;
    Ok(())
}

/// Artefact déclaré pour ce job, s'il existe.
pub async fn get_artifact(
    pool: &SqlitePool,
    job_id: Uuid,
    kind: ArtifactKind,
) -> AppResult<Option<JobArtifact>> {
    let row: Option<(Uuid, i64, String, String, String, i64)> = sqlx::query_as(
        "SELECT holder_worker_id, lease_epoch, root, name, sha256, bytes \
         FROM job_artifacts WHERE job_id=?1 AND kind=?2",
    )
    .bind(job_id)
    .bind(kind.as_str())
    .fetch_optional(pool)
    .await?;
    let Some((holder_worker_id, lease_epoch, root, name, sha256, bytes)) = row else {
        return Ok(None);
    };
    Ok(Some(JobArtifact {
        holder_worker_id,
        lease_epoch,
        declaration: ArtifactDeclaration {
            root,
            name,
            sha256,
            bytes,
        },
    }))
}

/// Ce worker doit-il encore retenir son fichier de travail pour ce job (artefact déclaré à cette
/// époque de bail, tâche encore en attente ou en cours) ? Une fois la réponse `false` (réception
/// confirmée, tâche terminée ou échouée), il peut purger sa copie.
pub async fn artifact_retained(
    pool: &SqlitePool,
    job_id: Uuid,
    worker: Uuid,
    lease_epoch: i64,
) -> AppResult<bool> {
    let retained: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM job_artifacts a JOIN jobs j ON j.id=a.job_id \
         WHERE a.job_id=?1 AND a.kind=?2 AND a.holder_worker_id=?3 AND a.lease_epoch=?4 \
           AND j.status IN (?5, ?6))",
    )
    .bind(job_id)
    .bind(ArtifactKind::Working.as_str())
    .bind(worker)
    .bind(lease_epoch)
    .bind(JobStatus::Queued)
    .bind(JobStatus::Running)
    .fetch_one(pool)
    .await?;
    Ok(retained)
}

/// Le repreneur a tiré et vérifié le fichier de travail : l'artefact n'a plus d'objet et son
/// détenteur peut le purger. `false` si ce bail n'avait rien à confirmer (confirmation répétée).
pub async fn confirm_artifact_received(
    pool: &SqlitePool,
    job_id: Uuid,
    lease: &LeaseRequest,
) -> AppResult<bool> {
    super::active(pool, job_id, lease).await?;
    let input = super::relays::artifact_input(pool, job_id, lease.worker_id).await?;
    if input.is_none() {
        return Ok(false);
    }
    release_artifact(pool, job_id, ArtifactKind::Working).await
}

/// Oublie l'artefact (le détenteur peut alors purger son fichier) ; `false` s'il n'existait pas.
pub async fn release_artifact(
    pool: &SqlitePool,
    job_id: Uuid,
    kind: ArtifactKind,
) -> AppResult<bool> {
    let released = sqlx::query("DELETE FROM job_artifacts WHERE job_id=?1 AND kind=?2")
        .bind(job_id)
        .bind(kind.as_str())
        .execute(pool)
        .await?
        .rows_affected();
    Ok(released > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn declaration() -> ArtifactDeclaration {
        ArtifactDeclaration {
            root: "/work/retained".into(),
            name: "working.mkv".into(),
            sha256: "ab".repeat(32),
            bytes: 10,
        }
    }

    #[test]
    fn names_are_a_single_plain_path_component() {
        assert!(valid_artifact_name("working.mkv"));
        for bad in ["", "../x", "a/b", "/abs", ".", "..", "tab\tname"] {
            assert!(!valid_artifact_name(bad), "{bad:?}");
        }
        assert!(!valid_artifact_name(&"x".repeat(ARTIFACT_NAME_MAX_LEN + 1)));
    }

    #[test]
    fn declarations_are_checked_on_every_field() {
        assert!(valid_declaration(&declaration(), 10));
        let tweak = |change: fn(&mut ArtifactDeclaration)| {
            let mut candidate = declaration();
            change(&mut candidate);
            valid_declaration(&candidate, 10)
        };
        assert!(!tweak(|d| d.root.clear()));
        assert!(!tweak(|d| d.root = "x".repeat(ARTIFACT_ROOT_MAX_LEN + 1)));
        assert!(!tweak(|d| d.name = "../working.mkv".into()));
        assert!(!tweak(|d| d.sha256 = "zz".repeat(32)));
        assert!(!tweak(|d| d.bytes = 0));
        assert!(!tweak(|d| d.bytes = 11));
    }
}
