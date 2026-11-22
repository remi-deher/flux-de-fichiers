//! Demandes d'enrôlement de workers par approbation et fenêtre d'acceptation (ADR 0016).

use crate::{
    api_keys,
    error::{AppError, AppResult},
};
use chrono::{DateTime, Duration, Utc};
use forgearr_shared::workers::{
    EnrollmentRequestStatus, EnrollmentWindowStatus, WorkerEnrollmentCreated, WorkerEnrollmentList,
    WorkerEnrollmentPollResponse, WorkerEnrollmentSummary,
};
use sqlx::SqlitePool;
use subtle::ConstantTimeEq;
use uuid::Uuid;

use super::identity::{get_setting, set_setting};

const ENROLLMENT_WINDOW_KEY: &str = "worker_enrollment_window_until";
/// Durée d'ouverture de la fenêtre d'acceptation (ADR 0016).
pub const ENROLLMENT_WINDOW_SECONDS: i64 = 15 * 60;
/// Nombre maximal de demandes en attente simultanées.
pub const MAX_PENDING_ENROLLMENT_REQUESTS: i64 = 20;

struct EnrollmentRow {
    id: Uuid,
    worker_name: String,
    worker_version: Option<String>,
    source_addr: Option<String>,
    secret_hash: String,
    status: String,
    created_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
}

impl EnrollmentRow {
    fn into_summary(self) -> AppResult<WorkerEnrollmentSummary> {
        let status = EnrollmentRequestStatus::parse(&self.status).ok_or(AppError::Internal)?;
        Ok(WorkerEnrollmentSummary {
            id: self.id,
            worker_name: self.worker_name,
            worker_version: self.worker_version,
            source_addr: self.source_addr,
            status,
            created_at: self.created_at,
            expires_at: self.expires_at,
        })
    }
}

/// Purge les demandes expirées pour garder la file propre.
pub async fn purge_expired_enrollment_requests(pool: &SqlitePool) -> AppResult<()> {
    let now = Utc::now();
    sqlx::query!(
        "DELETE FROM worker_enrollment_requests WHERE expires_at <= ?1",
        now
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// État courant de la fenêtre d'acceptation des demandes d'enrôlement.
pub async fn enrollment_window_status(pool: &SqlitePool) -> AppResult<EnrollmentWindowStatus> {
    let now = Utc::now();
    let Some(raw) = get_setting(pool, ENROLLMENT_WINDOW_KEY).await? else {
        return Ok(EnrollmentWindowStatus {
            open: false,
            expires_at: None,
        });
    };
    let parsed = DateTime::parse_from_rfc3339(raw.trim())
        .ok()
        .map(|dt| dt.with_timezone(&Utc));
    match parsed {
        Some(expires_at) if expires_at > now => Ok(EnrollmentWindowStatus {
            open: true,
            expires_at: Some(expires_at),
        }),
        _ => Ok(EnrollmentWindowStatus {
            open: false,
            expires_at: None,
        }),
    }
}

/// Ouvre (pour 15 minutes) ou ferme la fenêtre d'acceptation des demandes d'enrôlement.
pub async fn set_enrollment_window(
    pool: &SqlitePool,
    open: bool,
) -> AppResult<EnrollmentWindowStatus> {
    if open {
        let expires_at = Utc::now() + Duration::seconds(ENROLLMENT_WINDOW_SECONDS);
        set_setting(pool, ENROLLMENT_WINDOW_KEY, &expires_at.to_rfc3339()).await?;
        Ok(EnrollmentWindowStatus {
            open: true,
            expires_at: Some(expires_at),
        })
    } else {
        sqlx::query!(
            "DELETE FROM instance_settings WHERE key = ?1",
            ENROLLMENT_WINDOW_KEY
        )
        .execute(pool)
        .await?;
        Ok(EnrollmentWindowStatus {
            open: false,
            expires_at: None,
        })
    }
}

/// Liste l'état de la fenêtre et les demandes actives (en attente).
pub async fn list_enrollment_requests(pool: &SqlitePool) -> AppResult<WorkerEnrollmentList> {
    purge_expired_enrollment_requests(pool).await?;
    let window = enrollment_window_status(pool).await?;
    let now = Utc::now();
    let rows = sqlx::query_as!(
        EnrollmentRow,
        r#"SELECT id AS "id!: Uuid", worker_name, worker_version, source_addr, secret_hash, status, created_at AS "created_at: DateTime<Utc>", expires_at AS "expires_at: DateTime<Utc>" FROM worker_enrollment_requests WHERE status = 'pending' AND expires_at > ?1 ORDER BY created_at DESC"#,
        now,
    )
    .fetch_all(pool)
    .await?;
    let requests = rows
        .into_iter()
        .map(EnrollmentRow::into_summary)
        .collect::<AppResult<Vec<_>>>()?;
    Ok(WorkerEnrollmentList { window, requests })
}

/// Enregistre une nouvelle demande d'enrôlement si la fenêtre est ouverte et le plafond respecté.
pub async fn create_enrollment_request(
    pool: &SqlitePool,
    worker_name: &str,
    worker_version: Option<&str>,
    source_addr: Option<&str>,
    secret_hash: &str,
) -> AppResult<WorkerEnrollmentCreated> {
    purge_expired_enrollment_requests(pool).await?;
    let window = enrollment_window_status(pool).await?;
    if !window.open {
        return Err(AppError::Forbidden);
    }
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let now = Utc::now();
    let pending: i64 = sqlx::query_scalar!(
        r#"SELECT COUNT(*) AS "count!: i64" FROM worker_enrollment_requests WHERE status = 'pending' AND expires_at > ?1"#,
        now,
    )
    .fetch_one(&mut *tx)
    .await?;
    if pending >= MAX_PENDING_ENROLLMENT_REQUESTS {
        return Err(AppError::RateLimited);
    }
    let id = Uuid::new_v4();
    let expires_at = now + Duration::seconds(ENROLLMENT_WINDOW_SECONDS);
    sqlx::query!(
        "INSERT INTO worker_enrollment_requests \
         (id, worker_name, worker_version, source_addr, secret_hash, status, created_at, expires_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, 'pending', ?6, ?7)",
        id,
        worker_name,
        worker_version,
        source_addr,
        secret_hash,
        now,
        expires_at,
    )
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(WorkerEnrollmentCreated {
        id,
        status: EnrollmentRequestStatus::Pending,
        expires_at,
    })
}

/// Approuve ou refuse une demande en attente.
pub async fn decide_enrollment_request(
    pool: &SqlitePool,
    id: Uuid,
    approve: bool,
) -> AppResult<WorkerEnrollmentSummary> {
    purge_expired_enrollment_requests(pool).await?;
    let next_status = if approve {
        EnrollmentRequestStatus::Approved
    } else {
        EnrollmentRequestStatus::Rejected
    };
    let next_status_str = next_status.as_str();
    let now = Utc::now();
    let row = sqlx::query_as!(
        EnrollmentRow,
        r#"UPDATE worker_enrollment_requests SET status = ?1 WHERE id = ?2 AND status = 'pending' AND expires_at > ?3 RETURNING id AS "id!: Uuid", worker_name, worker_version, source_addr, secret_hash, status, created_at AS "created_at: DateTime<Utc>", expires_at AS "expires_at: DateTime<Utc>""#,
        next_status_str,
        id,
        now,
    )
    .fetch_optional(pool)
    .await?
    .ok_or(AppError::NotFound)?;
    row.into_summary()
}

/// Consulte une demande par le worker et lui remet sa clé une seule fois si approuvée.
pub async fn poll_enrollment_request(
    pool: &SqlitePool,
    id: Uuid,
    secret: &str,
) -> AppResult<WorkerEnrollmentPollResponse> {
    let presented_hash = crate::opaque_token::sha256_hex(secret);
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let Some(row) = sqlx::query_as!(
        EnrollmentRow,
        r#"SELECT id AS "id!: Uuid", worker_name, worker_version, source_addr, secret_hash, status, created_at AS "created_at: DateTime<Utc>", expires_at AS "expires_at: DateTime<Utc>" FROM worker_enrollment_requests WHERE id = ?1"#,
        id,
    )
    .fetch_optional(&mut *tx)
    .await?
    else {
        return Err(AppError::NotFound);
    };
    if row
        .secret_hash
        .as_bytes()
        .ct_eq(presented_hash.as_bytes())
        .unwrap_u8()
        != 1
    {
        return Err(AppError::Unauthorized);
    }
    let now = Utc::now();
    if row.expires_at <= now {
        sqlx::query!("DELETE FROM worker_enrollment_requests WHERE id = ?1", id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        return Ok(WorkerEnrollmentPollResponse {
            status: EnrollmentRequestStatus::Expired,
            key: None,
        });
    }
    let status = EnrollmentRequestStatus::parse(&row.status).ok_or(AppError::Internal)?;
    match status {
        EnrollmentRequestStatus::Pending => Ok(WorkerEnrollmentPollResponse {
            status: EnrollmentRequestStatus::Pending,
            key: None,
        }),
        EnrollmentRequestStatus::Rejected => {
            sqlx::query!("DELETE FROM worker_enrollment_requests WHERE id = ?1", id)
                .execute(&mut *tx)
                .await?;
            tx.commit().await?;
            Ok(WorkerEnrollmentPollResponse {
                status: EnrollmentRequestStatus::Rejected,
                key: None,
            })
        }
        EnrollmentRequestStatus::Approved => {
            let deleted = sqlx::query!(
                "DELETE FROM worker_enrollment_requests WHERE id = ?1 AND status = 'approved'",
                id,
            )
            .execute(&mut *tx)
            .await?
            .rows_affected();
            if deleted == 0 {
                return Err(AppError::NotFound);
            }
            let issued = api_keys::issue_for_worker(&mut tx, &row.worker_name).await?;
            tx.commit().await?;
            Ok(WorkerEnrollmentPollResponse {
                status: EnrollmentRequestStatus::Approved,
                key: Some(issued.raw),
            })
        }
        EnrollmentRequestStatus::Expired => Ok(WorkerEnrollmentPollResponse {
            status: EnrollmentRequestStatus::Expired,
            key: None,
        }),
    }
}
