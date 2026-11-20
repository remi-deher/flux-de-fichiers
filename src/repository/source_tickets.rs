//! Billets de lecture d'une source détenue par un autre worker (ADR 0019).

use crate::{
    domain::{Claim, JobStatus},
    error::{AppError, AppResult},
};
use forgearr_shared::{
    transfer::{SOURCE_TICKET_TTL_SECONDS, SourceTicket, TransferGrant},
    workers::WORKER_ONLINE_SECONDS,
};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use uuid::Uuid;

/// Empreinte stockée d'un secret de billet.
fn token_hash(token: &str) -> String {
    Sha256::digest(token.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Secret opaque de 244 bits tirés de l'aléa du système.
fn new_token() -> String {
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}

/// Adresse de transfert annoncée par un worker, acceptée seulement en `http` ou `https`.
pub fn valid_transfer_url(raw: &str) -> Option<String> {
    let url = reqwest::Url::parse(raw.trim()).ok()?;
    let acceptable = matches!(url.scheme(), "http" | "https")
        && url.host().is_some()
        && url.username().is_empty()
        && url.password().is_none()
        && url.query().is_none()
        && url.fragment().is_none();
    acceptable.then(|| url.as_str().trim_end_matches('/').to_owned())
}

/// Enregistre (ou efface) l'adresse de transfert d'un worker.
pub async fn set_transfer_url(
    pool: &SqlitePool,
    worker: Uuid,
    transfer_url: Option<&str>,
) -> AppResult<()> {
    sqlx::query("UPDATE workers SET transfer_url=?2 WHERE id=?1")
        .bind(worker)
        .bind(transfer_url)
        .execute(pool)
        .await?;
    Ok(())
}

/// Worker à accès direct qui détient la source d'une tâche (ADR 0021).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceHolder {
    pub worker_id: Uuid,
    /// Racine du dossier de la source telle que montée sur ce worker.
    pub root: String,
    pub transfer_url: Option<String>,
}

/// Détenteurs en ligne de la source, hors `exclude`, ceux qui annoncent une adresse de transfert
/// d'abord. Tout worker à accès direct qui partage la `source_key` convient, pas seulement celui
/// qui a scanné ; à défaut, le worker propriétaire enregistré sur la tâche.
pub async fn source_holders(
    pool: &SqlitePool,
    job_id: Uuid,
    exclude: Uuid,
) -> AppResult<Vec<SourceHolder>> {
    source_holders_matching(pool, job_id, exclude, false).await
}

/// Détenteurs en ligne dont le montage autorise l'écriture pour cette source.
pub async fn writable_source_holders(
    pool: &SqlitePool,
    job_id: Uuid,
    exclude: Uuid,
) -> AppResult<Vec<SourceHolder>> {
    source_holders_matching(pool, job_id, exclude, true).await
}

async fn source_holders_matching(
    pool: &SqlitePool,
    job_id: Uuid,
    exclude: Uuid,
    writable: bool,
) -> AppResult<Vec<SourceHolder>> {
    let rows: Vec<(Uuid, String, Option<String>)> = sqlx::query_as(
        "SELECT m.worker_id, m.local_path, w.transfer_url \
         FROM verified_job_source_mounts m JOIN workers w ON w.id = m.worker_id \
         WHERE m.job_id=?1 AND m.worker_id != ?2 \
           AND (?4 = 0 OR m.writable = 1) \
           AND w.last_seen > datetime('now', '-' || ?3 || ' seconds') \
         ORDER BY (w.transfer_url IS NULL), w.last_seen DESC",
    )
    .bind(job_id)
    .bind(exclude)
    .bind(WORKER_ONLINE_SECONDS)
    .bind(writable)
    .fetch_all(pool)
    .await?;
    let mut holders: Vec<SourceHolder> = rows
        .into_iter()
        .map(|(worker_id, root, transfer_url)| SourceHolder {
            worker_id,
            root,
            transfer_url,
        })
        .collect();
    if holders.is_empty()
        && let Some(source) = super::job_source(pool, job_id).await?
        && (!writable || (source.source_writable && source.source_key.is_none()))
        && let Some(owner) = source.source_worker_id.filter(|owner| *owner != exclude)
    {
        let row: Option<(Option<String>, bool)> = sqlx::query_as(
            "SELECT transfer_url, last_seen > datetime('now', '-' || ?2 || ' seconds') \
             FROM workers WHERE id=?1",
        )
        .bind(owner)
        .bind(WORKER_ONLINE_SECONDS)
        .fetch_optional(pool)
        .await?;
        if let Some((transfer_url, true)) = row {
            holders.push(SourceHolder {
                worker_id: owner,
                root: source.source_root,
                transfer_url,
            });
        }
    }
    Ok(holders)
}

/// Remet au worker traitant un billet pour lire la source chez un détenteur en ligne.
///
/// Sans effet si la source est déjà lisible (montage, central) ou si aucun détenteur n'annonce
/// d'adresse : le worker traitant utilisera alors le repli par le central.
pub async fn attach_source_ticket(
    pool: &SqlitePool,
    worker: Uuid,
    claim: &mut Claim,
) -> AppResult<()> {
    if claim.local_source_root.is_some() {
        return Ok(());
    }
    let Some(source) = super::job_source(pool, claim.job_id).await? else {
        return Ok(());
    };
    // Source du central (aucun worker propriétaire) ou déjà montée par ce worker : rien à transférer.
    if source.source_worker_id.is_none_or(|owner| owner == worker) {
        return Ok(());
    }
    let holder = source_holders(pool, claim.job_id, worker)
        .await?
        .into_iter()
        .find(|holder| holder.transfer_url.is_some());
    let Some(SourceHolder {
        worker_id: holder,
        transfer_url: Some(url),
        ..
    }) = holder
    else {
        return Ok(());
    };
    let token = new_token();
    sqlx::query("DELETE FROM source_tickets WHERE expires_at <= datetime('now')")
        .execute(pool)
        .await?;
    sqlx::query(
        "INSERT INTO source_tickets(token_hash, job_id, source_worker_id, target_worker_id, \
             lease_epoch, expires_at) \
         VALUES(?1, ?2, ?3, ?4, ?5, datetime('now', '+' || ?6 || ' seconds'))",
    )
    .bind(token_hash(&token))
    .bind(claim.job_id)
    .bind(holder)
    .bind(worker)
    .bind(claim.lease_epoch)
    .bind(SOURCE_TICKET_TTL_SECONDS)
    .execute(pool)
    .await?;
    claim.source_ticket = Some(SourceTicket { url, token });
    Ok(())
}

/// Valide un billet présenté au worker source et dit ce qu'il doit servir.
///
/// Le billet doit être non expiré, destiné à ce worker source, et le bail du worker traitant doit
/// toujours être en cours à la même époque.
pub async fn redeem_ticket(
    pool: &SqlitePool,
    token: &str,
    source_worker: Uuid,
) -> AppResult<TransferGrant> {
    let row: Option<(Uuid, i64)> = sqlx::query_as(
        "SELECT job_id, lease_epoch FROM source_tickets \
         WHERE token_hash=?1 AND source_worker_id=?2 AND expires_at > datetime('now')",
    )
    .bind(token_hash(token))
    .bind(source_worker)
    .fetch_optional(pool)
    .await?;
    let (job_id, lease_epoch) = row.ok_or(AppError::Unauthorized)?;
    let job = super::get_job(pool, job_id).await?;
    if job.status != JobStatus::Running || job.lease_epoch != lease_epoch {
        return Err(AppError::Conflict);
    }
    // Racine chez ce détenteur : son montage vérifié, sinon la racine enregistrée sur la tâche.
    let mount: Option<String> = sqlx::query_scalar(
        "SELECT local_path FROM verified_job_source_mounts WHERE job_id=?1 AND worker_id=?2",
    )
    .bind(job_id)
    .bind(source_worker)
    .fetch_optional(pool)
    .await?;
    let root = match mount {
        Some(root) => root,
        None => {
            let source = super::job_source(pool, job_id)
                .await?
                .filter(|source| source.source_worker_id == Some(source_worker))
                .ok_or(AppError::NotFound)?;
            source.source_root
        }
    };
    Ok(TransferGrant {
        root,
        source_name: job.source_name,
        sha256: job.source_sha256.ok_or(AppError::Internal)?,
        bytes: job.source_bytes.ok_or(AppError::Internal)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transfer_url_accepts_plain_http_and_https_only() {
        assert_eq!(
            valid_transfer_url(" http://nas.lan:9000/ ").as_deref(),
            Some("http://nas.lan:9000")
        );
        assert!(valid_transfer_url("https://nas.example/transfer").is_some());
        assert!(valid_transfer_url("ftp://nas").is_none());
        assert!(valid_transfer_url("http://user:pass@nas").is_none());
        assert!(valid_transfer_url("http://nas?x=1").is_none());
        assert!(valid_transfer_url("pas une url").is_none());
    }

    #[test]
    fn tokens_are_long_and_hashed_deterministically() {
        let token = new_token();
        assert_eq!(token.len(), 64);
        assert_ne!(token, new_token());
        assert_eq!(token_hash(&token), token_hash(&token));
        assert_eq!(token_hash(&token).len(), 64);
    }
}
