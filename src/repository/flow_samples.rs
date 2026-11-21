//! Métadonnées du dernier vrai job de chaque flux (échantillon de simulation de l'éditeur).

use crate::{
    domain::{JobStatus, LeaseRequest},
    error::{AppError, AppResult},
};
use forgearr_shared::flow_analysis::FlowMediaSample;
use serde_json::Value;
use sqlx::SqlitePool;
use uuid::Uuid;

/// Retient les métadonnées finales d'un job pour son flux, sous le bail qui les rapporte.
///
/// Sans flux, avec des métadonnées vides ou trop volumineuses, rien n'est enregistré.
pub async fn save_flow_media_sample(
    pool: &SqlitePool,
    job_id: Uuid,
    lease: &LeaseRequest,
    media_info: &Value,
) -> AppResult<()> {
    let empty = media_info.is_null() || media_info.as_object().is_some_and(|map| map.is_empty());
    let json = serde_json::to_string(media_info).map_err(|_| AppError::Invalid)?;
    if empty || json.len() > FlowMediaSample::MAX_BYTES {
        return Ok(());
    }
    let active: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM jobs WHERE id=?1 AND worker_id=?2 AND lease_token=?3 \
         AND lease_epoch=?4 AND status=?5 AND leased_until > datetime('now'))",
    )
    .bind(job_id)
    .bind(lease.worker_id)
    .bind(lease.lease_token)
    .bind(lease.lease_epoch)
    .bind(JobStatus::Running)
    .fetch_one(pool)
    .await?;
    if !active {
        return Err(AppError::Conflict);
    }
    sqlx::query(
        "INSERT INTO flow_media_samples(flow_id, job_id, source_name, media_info_json) \
         SELECT flow_id, id, source_name, ?2 FROM jobs WHERE id=?1 AND flow_id IS NOT NULL \
         ON CONFLICT(flow_id) DO UPDATE SET job_id=excluded.job_id, \
             source_name=excluded.source_name, media_info_json=excluded.media_info_json, \
             recorded_at=datetime('now')",
    )
    .bind(job_id)
    .bind(json)
    .execute(pool)
    .await?;
    Ok(())
}

/// Métadonnées du dernier job du flux, s'il y en a.
pub async fn latest_flow_media_sample(
    pool: &SqlitePool,
    flow_id: &str,
) -> AppResult<Option<FlowMediaSample>> {
    let row: Option<(Uuid, String, String, String)> = sqlx::query_as(
        "SELECT job_id, source_name, media_info_json, recorded_at FROM flow_media_samples \
         WHERE flow_id=?1",
    )
    .bind(flow_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.and_then(|(job_id, source_name, json, recorded_at)| {
        Some(FlowMediaSample {
            job_id,
            source_name,
            media_info: serde_json::from_str(&json).ok()?,
            recorded_at,
        })
    }))
}
