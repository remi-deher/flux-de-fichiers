//! Contexte persistant d'une étape : noms d'identité et variables figées d'une notification différée.

use crate::error::{AppError, AppResult};
use serde_json::{Map, Value};
use sqlx::SqlitePool;
use uuid::Uuid;

/// Nom de la bibliothèque (variable `library.name`), absent sans bibliothèque.
pub async fn library_name(
    pool: &SqlitePool,
    library_id: Option<Uuid>,
) -> AppResult<Option<String>> {
    let Some(library) = library_id else {
        return Ok(None);
    };
    Ok(sqlx::query_scalar("SELECT name FROM libraries WHERE id=?1")
        .bind(library)
        .fetch_optional(pool)
        .await?)
}

/// Nom d'un worker (variable `worker.name`), absent s'il n'existe plus.
pub async fn worker_name(pool: &SqlitePool, worker_id: Uuid) -> AppResult<Option<String>> {
    Ok(sqlx::query_scalar("SELECT name FROM workers WHERE id=?1")
        .bind(worker_id)
        .fetch_optional(pool)
        .await?)
}

/// Variables personnalisées figées sur une étape (vide si l'étape n'en porte pas).
pub async fn step_variables(
    pool: &SqlitePool,
    job_id: Uuid,
    step_index: i32,
) -> AppResult<Map<String, Value>> {
    let stored: Option<Option<String>> = sqlx::query_scalar(
        "SELECT variables_json FROM job_steps WHERE job_id=?1 AND step_index=?2",
    )
    .bind(job_id)
    .bind(step_index)
    .fetch_optional(pool)
    .await?;
    match stored.flatten() {
        Some(json) => serde_json::from_str(&json).map_err(|_| AppError::Invalid),
        None => Ok(Map::new()),
    }
}
