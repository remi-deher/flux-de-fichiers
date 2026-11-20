//! Journal durable des remplacements dont le résultat peut devenir incertain.

use crate::{
    domain::{JobErrorCode, JobStatus, LeaseRequest, LibraryOutputPolicy},
    error::{AppError, AppResult},
};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
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
}

pub async fn pending_source_publications(
    pool: &SqlitePool,
) -> AppResult<Vec<PendingSourcePublication>> {
    Ok(sqlx::query_as("SELECT job_id,lease_epoch,output_name,output_extension,output_sha256,output_bytes,output_warning,output_policy FROM source_publication_journal WHERE state=?1 ORDER BY created_at,job_id")
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
    begin_publication(pool, job, lease, output, extension, policy).await
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
    begin_publication(pool, job, lease, output, extension, policy).await
}

async fn begin_publication(
    pool: &SqlitePool,
    job: Uuid,
    lease: &LeaseRequest,
    output: super::PublishedOutput<'_>,
    extension: &str,
    policy: LibraryOutputPolicy,
) -> AppResult<()> {
    if policy == LibraryOutputPolicy::OutputDir {
        return Err(AppError::Invalid);
    }
    let rows = sqlx::query("INSERT INTO source_publication_journal(job_id,lease_epoch,output_name,output_extension,output_sha256,output_bytes,output_warning,output_policy,state) \
        SELECT id,lease_epoch,?5,?12,?6,?7,?8,?9,?10 FROM jobs WHERE id=?1 AND worker_id=?2 AND lease_token=?3 AND lease_epoch=?4 AND status=?11 AND leased_until > datetime('now')")
        .bind(job).bind(lease.worker_id).bind(lease.lease_token).bind(lease.lease_epoch).bind(output.name).bind(output.sha256).bind(output.bytes)
        .bind(output.warning).bind(policy).bind(PublicationState::InProgress.as_str()).bind(JobStatus::Running).bind(extension).execute(pool).await?.rows_affected();
    if rows != 1 {
        return Err(AppError::Conflict);
    }
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

/// Marque une issue inconnue ; une perte de bail reste distinguée de cette incertitude.
pub async fn mark_source_publication_uncertain(
    pool: &SqlitePool,
    job: Uuid,
    epoch: i64,
) -> AppResult<Option<JobErrorCode>> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let code: Option<JobErrorCode> = sqlx::query_scalar("UPDATE jobs SET status=?3, error_code=CASE WHEN leased_until <= datetime('now') THEN ?4 ELSE ?5 END, \
        output_name=NULLIF((SELECT output_name FROM source_publication_journal WHERE job_id=?1 AND lease_epoch=?2),''), \
        output_sha256=(SELECT output_sha256 FROM source_publication_journal WHERE job_id=?1 AND lease_epoch=?2), \
        output_bytes=(SELECT output_bytes FROM source_publication_journal WHERE job_id=?1 AND lease_epoch=?2), \
        warning=(SELECT output_warning FROM source_publication_journal WHERE job_id=?1 AND lease_epoch=?2), \
        lease_token=NULL,lease_epoch=lease_epoch+1,leased_until=NULL,updated_at=datetime('now') \
        WHERE id=?1 AND lease_epoch=?2 AND status=?6 AND EXISTS(SELECT 1 FROM source_publication_journal p WHERE p.job_id=jobs.id AND p.lease_epoch=?2 AND p.state=?7) RETURNING error_code")
        .bind(job).bind(epoch).bind(JobStatus::Failed).bind(JobErrorCode::LeaseExpired).bind(JobErrorCode::SourcePublicationUncertain)
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
pub(super) async fn reconcile_expired_publications(
    conn: &mut sqlx::SqliteConnection,
) -> AppResult<()> {
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
