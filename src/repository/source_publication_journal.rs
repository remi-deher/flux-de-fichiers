//! Journal durable des remplacements dont le résultat peut devenir incertain.

use crate::{
    domain::{JobErrorCode, JobStatus, LeaseRequest, LibraryOutputPolicy},
    error::{AppError, AppResult},
};
use serde::{Deserialize, Serialize};
use sqlx::{SqliteConnection, SqlitePool};
use uuid::Uuid;

#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum PublicationState {
    InProgress,
    Uncertain,
}

impl PendingSourcePublication {
    /// Le central garde-t-il le fichier de sortie ?
    pub fn has_central_copy(&self) -> bool {
        !self.output_name.is_empty()
    }
}

impl PublicationState {
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::InProgress => "in_progress",
            Self::Uncertain => "uncertain",
        }
    }
}

/// Instantané durable nécessaire à l'examen d'une publication interrompue.
#[derive(Clone, sqlx::FromRow)]
pub struct PendingSourcePublication {
    pub job_id: Uuid,
    pub lease_epoch: i64,
    /// Vide quand le résultat a été tiré et remplacé sans copie au central (ADR 0022).
    pub output_name: String,
    pub output_extension: String,
    pub output_sha256: String,
    pub output_bytes: i64,
    pub output_warning: bool,
    pub output_policy: LibraryOutputPolicy,
    /// Le worker qui traite applique lui-même le remplacement (et le confirme par `result/applied`).
    pub applied_locally: bool,
}

pub async fn pending_source_publications(
    pool: &SqlitePool,
) -> AppResult<Vec<PendingSourcePublication>> {
    Ok(sqlx::query_as("SELECT job_id,lease_epoch,output_name,output_extension,output_sha256,output_bytes,output_warning,output_policy,applied_locally FROM source_publication_journal WHERE state=?1 ORDER BY created_at,job_id")
        .bind(PublicationState::InProgress.as_str()).fetch_all(pool).await?)
}

/// Inscrit le résultat préparé (copie au central) uniquement si le bail reste actif.
pub async fn begin_source_publication(
    pool: &SqlitePool,
    job: Uuid,
    lease: &LeaseRequest,
    output: super::PublishedOutput<'_>,
    policy: LibraryOutputPolicy,
) -> AppResult<()> {
    let extension = std::path::Path::new(output.name)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    begin_publication(pool, job, lease, output, extension, policy, false).await
}

/// Comme [`begin_source_publication`], pour un résultat tiré par le détenteur : `output.name` est
/// vide, l'extension est déclarée à part.
pub async fn begin_pulled_publication(
    pool: &SqlitePool,
    job: Uuid,
    lease: &LeaseRequest,
    output: super::PublishedOutput<'_>,
    extension: &str,
    policy: LibraryOutputPolicy,
) -> AppResult<()> {
    begin_publication(pool, job, lease, output, extension, policy, false).await
}

/// Comme [`begin_pulled_publication`], pour un remplacement que le worker qui traite applique
/// lui-même sur son montage.
pub async fn begin_local_publication(
    pool: &SqlitePool,
    job: Uuid,
    lease: &LeaseRequest,
    output: super::PublishedOutput<'_>,
    extension: &str,
    policy: LibraryOutputPolicy,
) -> AppResult<()> {
    begin_publication(pool, job, lease, output, extension, policy, true).await
}

async fn begin_publication(
    pool: &SqlitePool,
    job: Uuid,
    lease: &LeaseRequest,
    output: super::PublishedOutput<'_>,
    extension: &str,
    policy: LibraryOutputPolicy,
    applied_locally: bool,
) -> AppResult<()> {
    if policy == LibraryOutputPolicy::OutputDir {
        return Err(AppError::Invalid);
    }
    let timeout = crate::runtime_settings::load_unreachable_timeout(pool).await;
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    super::leases::ensure_held(&mut tx, job, lease).await?;
    super::irreversible_actions::ensure_no_failure_flow(&mut tx, job).await?;
    // Une échéance déjà passée ne doit plus conclure ce bail sous la publication qui commence.
    super::leases::refresh_contact(&mut tx, job, timeout).await?;
    if applied_locally && same_local_intent(&mut tx, job, lease.lease_epoch, output).await? {
        tx.commit().await?;
        return Ok(());
    }
    sqlx::query("INSERT INTO source_publication_journal(job_id,lease_epoch,output_name,output_extension,output_sha256,output_bytes,output_warning,output_policy,state,applied_locally) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)")
        .bind(job).bind(lease.lease_epoch).bind(output.name).bind(extension).bind(output.sha256).bind(output.bytes)
        .bind(output.warning).bind(policy).bind(PublicationState::InProgress.as_str()).bind(applied_locally)
        .execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(())
}

/// Le même remplacement local est-il déjà inscrit par ce bail (réponse perdue) ?
async fn same_local_intent(
    conn: &mut SqliteConnection,
    job: Uuid,
    epoch: i64,
    output: super::PublishedOutput<'_>,
) -> AppResult<bool> {
    Ok(sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM source_publication_journal WHERE job_id=?1 AND lease_epoch=?2 \
         AND state=?3 AND applied_locally=1 AND output_sha256=?4 AND output_bytes=?5)",
    )
    .bind(job)
    .bind(epoch)
    .bind(PublicationState::InProgress.as_str())
    .bind(output.sha256)
    .bind(output.bytes)
    .fetch_one(conn)
    .await?)
}

/// Remplacement en cours sous cette époque ; `Some(true)` s'il est appliqué par le worker qui traite.
pub(super) async fn publication_in_progress(
    conn: &mut SqliteConnection,
    job: Uuid,
    epoch: i64,
) -> AppResult<Option<bool>> {
    Ok(sqlx::query_scalar(
        "SELECT applied_locally FROM source_publication_journal \
         WHERE job_id=?1 AND lease_epoch=?2 AND state=?3",
    )
    .bind(job)
    .bind(epoch)
    .bind(PublicationState::InProgress.as_str())
    .fetch_optional(conn)
    .await?)
}

/// [`publication_in_progress`] hors transaction.
pub async fn source_publication_in_progress(
    pool: &SqlitePool,
    job: Uuid,
    epoch: i64,
) -> AppResult<Option<bool>> {
    publication_in_progress(&mut *pool.acquire().await?, job, epoch).await
}

/// Dernier accord avant le remplacement : bail vérifié et accord inscrit ensemble. Rend `false` si
/// aucun remplacement n'est inscrit pour ce bail (intention écartée).
pub async fn authorize_source_publication(
    pool: &SqlitePool,
    job: Uuid,
    lease: &LeaseRequest,
) -> AppResult<bool> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    super::leases::ensure_held(&mut tx, job, lease).await?;
    super::irreversible_actions::ensure_no_failure_flow(&mut tx, job).await?;
    let authorized = sqlx::query(
        "UPDATE source_publication_journal SET authorized_at=COALESCE(authorized_at, datetime('now')) \
         WHERE job_id=?1 AND lease_epoch=?2 AND state=?3",
    )
    .bind(job)
    .bind(lease.lease_epoch)
    .bind(PublicationState::InProgress.as_str())
    .execute(&mut *tx)
    .await?
    .rows_affected();
    tx.commit().await?;
    Ok(authorized > 0)
}

/// [`authorize_source_publication`] qui refuse (`Conflict`) sans intention inscrite : elle a pu être
/// écartée, et un remplacement sans journal ne serait jamais repris.
pub async fn require_publication_grant(
    pool: &SqlitePool,
    job: Uuid,
    lease: &LeaseRequest,
) -> AppResult<()> {
    if authorize_source_publication(pool, job, lease).await? {
        Ok(())
    } else {
        Err(AppError::Conflict)
    }
}

/// Écarte une intention dont l'exécutant a disparu avant tout accord : rien n'a été remplacé, et
/// l'accord ne peut plus être donné. `false` si l'accord a été donné.
pub async fn abandon_unauthorized_publication(
    pool: &SqlitePool,
    job: Uuid,
    epoch: i64,
) -> AppResult<bool> {
    let dropped = sqlx::query(
        "DELETE FROM source_publication_journal WHERE job_id=?1 AND lease_epoch=?2 AND state=?3 \
         AND authorized_at IS NULL",
    )
    .bind(job)
    .bind(epoch)
    .bind(PublicationState::InProgress.as_str())
    .execute(pool)
    .await?
    .rows_affected();
    Ok(dropped > 0)
}

/// La source avait changé : rien n'a été remplacé. Le journal est retiré et l'échec inscrit ensemble,
/// pour qu'une répétition du rapport retrouve l'issue.
pub async fn fail_changed_source(
    pool: &SqlitePool,
    job: Uuid,
    lease: &LeaseRequest,
) -> AppResult<()> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    super::leases::ensure_held(&mut tx, job, lease).await?;
    sqlx::query("DELETE FROM source_publication_journal WHERE job_id=?1 AND lease_epoch=?2")
        .bind(job)
        .bind(lease.lease_epoch)
        .execute(&mut *tx)
        .await?;
    super::leases::set_failed(&mut tx, job, JobErrorCode::SourceChanged).await?;
    tx.commit().await?;
    Ok(())
}

/// Retire une intention dont le remplacement a été refusé avant toute modification.
pub async fn clear_source_publication(pool: &SqlitePool, job: Uuid, epoch: i64) -> AppResult<()> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    sqlx::query("UPDATE jobs SET output_name=NULL,output_sha256=NULL,output_bytes=NULL WHERE id=?1 AND status=?3 AND error_code=?4 AND output_name=(SELECT output_name FROM source_publication_journal WHERE job_id=?1 AND lease_epoch=?2)")
        .bind(job).bind(epoch).bind(JobStatus::Failed).bind(JobErrorCode::LeaseExpired).execute(&mut *tx).await?;
    sqlx::query("DELETE FROM source_publication_journal WHERE job_id=?1 AND lease_epoch=?2")
        .bind(job)
        .bind(epoch)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

/// Marque une issue inconnue. Un bail encore inscrit n'est pas perdu, même échéance passée :
/// `lease_expired` ne vient que de la fin de bail.
pub async fn mark_source_publication_uncertain(
    pool: &SqlitePool,
    job: Uuid,
    epoch: i64,
) -> AppResult<Option<JobErrorCode>> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let code: Option<JobErrorCode> = sqlx::query_scalar("UPDATE jobs SET status=?3, error_code=?4, \
        output_name=NULLIF((SELECT output_name FROM source_publication_journal WHERE job_id=?1 AND lease_epoch=?2),''), \
        output_sha256=(SELECT output_sha256 FROM source_publication_journal WHERE job_id=?1 AND lease_epoch=?2), \
        output_bytes=(SELECT output_bytes FROM source_publication_journal WHERE job_id=?1 AND lease_epoch=?2), \
        warning=(SELECT output_warning FROM source_publication_journal WHERE job_id=?1 AND lease_epoch=?2), \
        lease_token=NULL,lease_epoch=lease_epoch+1,leased_until=NULL,updated_at=datetime('now') \
        WHERE id=?1 AND lease_epoch=?2 AND status=?5 AND EXISTS(SELECT 1 FROM source_publication_journal p WHERE p.job_id=jobs.id AND p.lease_epoch=?2 AND p.state=?6) RETURNING error_code")
        .bind(job).bind(epoch).bind(JobStatus::Failed).bind(JobErrorCode::SourcePublicationUncertain)
        .bind(JobStatus::Running).bind(PublicationState::InProgress.as_str()).fetch_optional(&mut *tx).await?;
    // Le balayage des baux peut avoir constaté l'expiration avant ce rapport.
    // Dans ce cas, conserver son code et rattacher le résultat au même epoch expiré.
    sqlx::query("UPDATE jobs SET \
        output_name=NULLIF((SELECT output_name FROM source_publication_journal WHERE job_id=?1 AND lease_epoch=?2),''), \
        output_sha256=(SELECT output_sha256 FROM source_publication_journal WHERE job_id=?1 AND lease_epoch=?2), \
        output_bytes=(SELECT output_bytes FROM source_publication_journal WHERE job_id=?1 AND lease_epoch=?2), \
        warning=(SELECT output_warning FROM source_publication_journal WHERE job_id=?1 AND lease_epoch=?2) \
        WHERE id=?1 AND lease_epoch=?2+1 AND status=?3 AND error_code=?4 \
        AND EXISTS(SELECT 1 FROM source_publication_journal WHERE job_id=?1 AND lease_epoch=?2 AND state=?5)")
        .bind(job).bind(epoch).bind(JobStatus::Failed).bind(JobErrorCode::LeaseExpired).bind(PublicationState::InProgress.as_str())
        .execute(&mut *tx).await?;
    sqlx::query("UPDATE source_publication_journal SET state=?3 WHERE job_id=?1 AND lease_epoch=?2 AND state=?4")
        .bind(job).bind(epoch).bind(PublicationState::Uncertain.as_str()).bind(PublicationState::InProgress.as_str()).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(code)
}

/// Au redémarrage, aucune requête en mémoire ne peut confirmer une publication interrompue.
pub async fn recover_source_publications(pool: &SqlitePool) -> AppResult<u64> {
    let pending: Vec<(Uuid, i64)> =
        sqlx::query_as("SELECT job_id,lease_epoch FROM source_publication_journal WHERE state=?1")
            .bind(PublicationState::InProgress.as_str())
            .fetch_all(pool)
            .await?;
    let mut recovered = 0;
    for (job, epoch) in pending {
        if mark_source_publication_uncertain(pool, job, epoch)
            .await?
            .is_some()
        {
            recovered += 1;
        }
    }
    Ok(recovered)
}

/// Rattache les résultats dont le bail vient d'être expiré par le balayage des jobs.
pub(super) async fn reconcile_expired_publications(conn: &mut SqliteConnection) -> AppResult<()> {
    sqlx::query("UPDATE jobs SET \
        output_name=NULLIF((SELECT p.output_name FROM source_publication_journal p WHERE p.job_id=jobs.id AND p.lease_epoch+1=jobs.lease_epoch AND p.state=?1),''), \
        output_sha256=(SELECT p.output_sha256 FROM source_publication_journal p WHERE p.job_id=jobs.id AND p.lease_epoch+1=jobs.lease_epoch AND p.state=?1), \
        output_bytes=(SELECT p.output_bytes FROM source_publication_journal p WHERE p.job_id=jobs.id AND p.lease_epoch+1=jobs.lease_epoch AND p.state=?1), \
        warning=(SELECT p.output_warning FROM source_publication_journal p WHERE p.job_id=jobs.id AND p.lease_epoch+1=jobs.lease_epoch AND p.state=?1) \
        WHERE status=?2 AND error_code=?3 AND EXISTS(SELECT 1 FROM source_publication_journal p WHERE p.job_id=jobs.id AND p.lease_epoch+1=jobs.lease_epoch AND p.state=?1)")
        .bind(PublicationState::InProgress.as_str()).bind(JobStatus::Failed).bind(JobErrorCode::LeaseExpired).execute(&mut *conn).await?;
    sqlx::query("UPDATE source_publication_journal SET state=?1 WHERE state=?2 AND EXISTS(SELECT 1 FROM jobs j WHERE j.id=source_publication_journal.job_id AND j.lease_epoch=source_publication_journal.lease_epoch+1 AND j.status=?3 AND j.error_code=?4)")
        .bind(PublicationState::Uncertain.as_str()).bind(PublicationState::InProgress.as_str())
        .bind(JobStatus::Failed).bind(JobErrorCode::LeaseExpired).execute(&mut *conn).await?;
    Ok(())
}
