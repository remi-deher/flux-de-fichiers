//! Billets de lecture d'une source ou d'un artefact détenu par un autre worker (ADR 0019 et 0022).

use crate::{
    domain::{Claim, JobStatus, LeaseRequest},
    error::{AppError, AppResult},
    opaque_token::{new_opaque_token, sha256_hex},
};
use forgearr_shared::{
    transfer::{ArtifactKind, SOURCE_TICKET_TTL_SECONDS, SourceTicket, TransferGrant},
    workers::WORKER_ONLINE_SECONDS,
};
use sqlx::SqlitePool;
use std::path::Path;
use uuid::Uuid;

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

/// Désigne (ou non) le worker qui sert les dossiers du central à la place du central : un seul à la
/// fois porte ce rôle.
pub async fn set_serves_central(pool: &SqlitePool, worker: Uuid, serves: bool) -> AppResult<()> {
    sqlx::query(
        "UPDATE workers SET serves_central=CASE WHEN id=?1 THEN ?2 ELSE 0 END \
         WHERE id=?1 OR serves_central=1",
    )
    .bind(worker)
    .bind(serves)
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
        && let Some((transfer_url, true)) = worker_reach(pool, owner).await?
    {
        holders.push(SourceHolder {
            worker_id: owner,
            root: source.source_root,
            transfer_url,
        });
    }
    Ok(holders)
}

/// Remet au worker traitant un billet pour lire la source chez un détenteur en ligne.
///
/// Sans effet si la source est déjà montée par le worker, ou si aucun détenteur (worker à accès direct
/// ou worker intégré pour le central) n'annonce d'adresse : la tâche échoue alors avec
/// `source_unreachable`, il n'y a plus de repli par le central.
pub async fn attach_source_ticket(
    pool: &SqlitePool,
    worker: Uuid,
    claim: &mut Claim,
) -> AppResult<()> {
    if claim.handoff_input.is_some() {
        // Remise par artefact (ADR 0022) : le billet désigne le fichier de travail chez son
        // détenteur, et non la source.
        let ticket = issue_artifact_ticket(
            pool,
            claim.job_id,
            ArtifactKind::Working,
            worker,
            claim.lease_epoch,
        )
        .await?;
        claim.source_ticket = ticket;
        return Ok(());
    }
    if claim.local_source_root.is_some() {
        return Ok(());
    }
    let ticket = issue_source_ticket(pool, worker, claim.job_id, claim.lease_epoch).await?;
    claim.source_ticket = ticket;
    Ok(())
}

/// Remet un billet frais au worker traitant dont le bail est actif : le billet précédent a expiré
/// ou la lecture a été interrompue et doit reprendre (`Range`). Le nouveau billet remplace les
/// précédents du même bail ; un transfert déjà en cours n'est pas coupé (le billet n'est validé
/// qu'au début de chaque requête).
pub async fn reissue_source_ticket(
    pool: &SqlitePool,
    job_id: Uuid,
    lease: &LeaseRequest,
) -> AppResult<Option<SourceTicket>> {
    super::active(pool, job_id, lease).await?;
    let input = super::relays::artifact_input(pool, job_id, lease.worker_id).await?;
    if input.is_some() {
        return issue_artifact_ticket(
            pool,
            job_id,
            ArtifactKind::Working,
            lease.worker_id,
            lease.lease_epoch,
        )
        .await;
    }
    issue_source_ticket(pool, lease.worker_id, job_id, lease.lease_epoch).await
}

/// Billet pour lire la source chez un détenteur en ligne ; `None` si la source est déjà lisible
/// (central, montage) ou si aucun détenteur n'annonce d'adresse.
async fn issue_source_ticket(
    pool: &SqlitePool,
    worker: Uuid,
    job_id: Uuid,
    lease_epoch: i64,
) -> AppResult<Option<SourceTicket>> {
    let source = super::job_source(pool, job_id).await?;
    // Source du central (aucun worker propriétaire) : le worker intégré la sert.
    let Some(source) = source.filter(|source| source.source_worker_id.is_some()) else {
        return issue_central_ticket(pool, worker, job_id, lease_epoch).await;
    };
    // Déjà montée par ce worker : rien à transférer.
    if source.source_worker_id == Some(worker) {
        return Ok(None);
    }
    let holder = source_holders(pool, job_id, worker)
        .await?
        .into_iter()
        .find(|holder| holder.transfer_url.is_some());
    let Some(SourceHolder {
        worker_id: holder,
        transfer_url: Some(url),
        ..
    }) = holder
    else {
        return Ok(None);
    };
    let ticket = store_ticket(pool, holder, url, job_id, worker, lease_epoch, None).await?;
    Ok(Some(ticket))
}

/// Billet pour lire une source du central chez le worker intégré, s'il annonce une adresse ;
/// `None` sinon, ou si le preneur est ce worker.
async fn issue_central_ticket(
    pool: &SqlitePool,
    taker: Uuid,
    job_id: Uuid,
    lease_epoch: i64,
) -> AppResult<Option<SourceTicket>> {
    let holder: Option<(Uuid, String)> = sqlx::query_as(
        "SELECT id, transfer_url FROM workers \
         WHERE serves_central=1 AND id != ?1 AND transfer_url IS NOT NULL \
           AND last_seen > datetime('now', '-' || ?2 || ' seconds')",
    )
    .bind(taker)
    .bind(WORKER_ONLINE_SECONDS)
    .fetch_optional(pool)
    .await?;
    let Some((holder, url)) = holder else {
        return Ok(None);
    };
    let ticket = store_ticket(pool, holder, url, job_id, taker, lease_epoch, None).await?;
    Ok(Some(ticket))
}

/// Billet pour tirer un artefact retenu chez son détenteur : `None` si l'artefact n'existe pas, si
/// le preneur est le détenteur, ou si le détenteur n'est pas en ligne avec une adresse annoncée.
pub async fn issue_artifact_ticket(
    pool: &SqlitePool,
    job_id: Uuid,
    kind: ArtifactKind,
    taker: Uuid,
    lease_epoch: i64,
) -> AppResult<Option<SourceTicket>> {
    let Some(artifact) = super::get_artifact(pool, job_id, kind).await? else {
        return Ok(None);
    };
    if artifact.holder_worker_id == taker {
        return Ok(None);
    }
    let Some((Some(url), true)) = worker_reach(pool, artifact.holder_worker_id).await? else {
        return Ok(None);
    };
    let ticket = store_ticket(
        pool,
        artifact.holder_worker_id,
        url,
        job_id,
        taker,
        lease_epoch,
        Some(kind),
    )
    .await?;
    Ok(Some(ticket))
}

/// Le worker annonce-t-il une adresse de transfert, donc peut-il servir des fichiers ?
pub async fn can_serve(pool: &SqlitePool, worker: Uuid) -> AppResult<bool> {
    Ok(matches!(
        worker_reach(pool, worker).await?,
        Some((Some(_), _))
    ))
}

/// Adresse de transfert annoncée par un worker et sa présence en ligne ; `None` s'il est inconnu.
async fn worker_reach(
    pool: &SqlitePool,
    worker: Uuid,
) -> AppResult<Option<(Option<String>, bool)>> {
    let reach: Option<(Option<String>, bool)> = sqlx::query_as(
        "SELECT transfer_url, last_seen > datetime('now', '-' || ?2 || ' seconds') \
         FROM workers WHERE id=?1",
    )
    .bind(worker)
    .bind(WORKER_ONLINE_SECONDS)
    .fetch_optional(pool)
    .await?;
    Ok(reach)
}

/// Enregistre un billet (seule l'empreinte du secret est conservée). Il remplace les billets
/// précédents du même preneur, du même bail et de la même cible (source ou nature d'artefact).
async fn store_ticket(
    pool: &SqlitePool,
    holder: Uuid,
    url: String,
    job_id: Uuid,
    taker: Uuid,
    lease_epoch: i64,
    artifact: Option<ArtifactKind>,
) -> AppResult<SourceTicket> {
    let token = new_opaque_token();
    let artifact_kind = artifact.map(ArtifactKind::as_str);
    sqlx::query("DELETE FROM source_tickets WHERE expires_at <= datetime('now')")
        .execute(pool)
        .await?;
    sqlx::query(
        "DELETE FROM source_tickets \
         WHERE job_id=?1 AND target_worker_id=?2 AND lease_epoch=?3 AND artifact_kind IS ?4",
    )
    .bind(job_id)
    .bind(taker)
    .bind(lease_epoch)
    .bind(artifact_kind)
    .execute(pool)
    .await?;
    sqlx::query(
        "INSERT INTO source_tickets(token_hash, job_id, source_worker_id, target_worker_id, \
             lease_epoch, expires_at, artifact_kind) \
         VALUES(?1, ?2, ?3, ?4, ?5, datetime('now', '+' || ?6 || ' seconds'), ?7)",
    )
    .bind(sha256_hex(&token))
    .bind(job_id)
    .bind(holder)
    .bind(taker)
    .bind(lease_epoch)
    .bind(SOURCE_TICKET_TTL_SECONDS)
    .bind(artifact_kind)
    .execute(pool)
    .await?;
    Ok(SourceTicket { url, token })
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
    redeem(pool, token, source_worker, None).await
}

/// Variante du worker intégré : il sert aussi les dossiers du central, dont le dépôt d'entrée
/// `input_dir` pour les tâches sans dossier configuré.
pub async fn redeem_central_ticket(
    pool: &SqlitePool,
    token: &str,
    source_worker: Uuid,
    input_dir: &Path,
) -> AppResult<TransferGrant> {
    redeem(pool, token, source_worker, Some(input_dir)).await
}

async fn redeem(
    pool: &SqlitePool,
    token: &str,
    source_worker: Uuid,
    input_dir: Option<&Path>,
) -> AppResult<TransferGrant> {
    let row: Option<(Uuid, i64, Option<String>)> = sqlx::query_as(
        "SELECT job_id, lease_epoch, artifact_kind FROM source_tickets \
         WHERE token_hash=?1 AND source_worker_id=?2 AND expires_at > datetime('now')",
    )
    .bind(sha256_hex(token))
    .bind(source_worker)
    .fetch_optional(pool)
    .await?;
    let (job_id, lease_epoch, artifact_kind) = row.ok_or(AppError::Unauthorized)?;
    let job = super::get_job(pool, job_id).await?;
    if job.status != JobStatus::Running || job.lease_epoch != lease_epoch {
        return Err(AppError::Conflict);
    }
    if let Some(raw_kind) = artifact_kind {
        let kind = ArtifactKind::parse(&raw_kind).ok_or(AppError::Internal)?;
        let artifact = super::get_artifact(pool, job_id, kind)
            .await?
            .filter(|artifact| artifact.holder_worker_id == source_worker)
            .ok_or(AppError::NotFound)?;
        return Ok(TransferGrant {
            root: artifact.declaration.root,
            source_name: artifact.declaration.name,
            sha256: artifact.declaration.sha256,
            bytes: artifact.declaration.bytes,
        });
    }
    let root = source_root(pool, job_id, source_worker, input_dir).await?;
    Ok(TransferGrant {
        root,
        source_name: job.source_name,
        sha256: job.source_sha256.ok_or(AppError::Internal)?,
        bytes: job.source_bytes.ok_or(AppError::Internal)?,
    })
}

/// Racine de la source chez ce détenteur : son montage vérifié, sinon la racine enregistrée sur la
/// tâche ; pour le worker intégré (`input_dir` fourni), celle du central.
async fn source_root(
    pool: &SqlitePool,
    job_id: Uuid,
    worker: Uuid,
    input_dir: Option<&Path>,
) -> AppResult<String> {
    let mount: Option<String> = sqlx::query_scalar(
        "SELECT local_path FROM verified_job_source_mounts WHERE job_id=?1 AND worker_id=?2",
    )
    .bind(job_id)
    .bind(worker)
    .fetch_optional(pool)
    .await?;
    if let Some(root) = mount {
        return Ok(root);
    }
    match (super::job_source(pool, job_id).await?, input_dir) {
        (Some(source), _) if source.source_worker_id == Some(worker) => Ok(source.source_root),
        (Some(source), Some(_)) if source.source_worker_id.is_none() => Ok(source.source_root),
        (None, Some(dir)) => Ok(dir.to_string_lossy().into_owned()),
        _ => Err(AppError::NotFound),
    }
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
}
