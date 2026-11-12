use crate::{
    domain::{FlowRecord, UpdateFlowRequest},
    error::{AppError, AppResult},
};
use sqlx::SqlitePool;

pub async fn list_flows(pool: &SqlitePool) -> AppResult<Vec<FlowRecord>> {
    let flows =
        sqlx::query_as::<_, FlowRecord>("SELECT * FROM flows ORDER BY is_default DESC, name ASC")
            .fetch_all(pool)
            .await?;
    Ok(flows)
}

pub async fn get_flow(pool: &SqlitePool, id: &str) -> AppResult<FlowRecord> {
    sqlx::query_as::<_, FlowRecord>("SELECT * FROM flows WHERE id=?1")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or(AppError::NotFound)
}

pub async fn default_flow(pool: &SqlitePool) -> AppResult<Option<FlowRecord>> {
    let flow = sqlx::query_as::<_, FlowRecord>(
        "SELECT * FROM flows WHERE is_default=1 ORDER BY updated_at DESC LIMIT 1",
    )
    .fetch_optional(pool)
    .await?;
    Ok(flow)
}

pub async fn save_flow(
    pool: &SqlitePool,
    id: &str,
    payload: &UpdateFlowRequest,
) -> AppResult<FlowRecord> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    if payload.is_default {
        sqlx::query("UPDATE flows SET is_default=0")
            .execute(&mut *tx)
            .await?;
    }
    let flow = sqlx::query_as::<_, FlowRecord>(
        "INSERT INTO flows (id, name, description, file_mask, is_default, definition, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, datetime('now')) \
         ON CONFLICT(id) DO UPDATE SET \
            name=excluded.name, \
            description=excluded.description, \
            file_mask=excluded.file_mask, \
            is_default=excluded.is_default, \
            definition=excluded.definition, \
            updated_at=datetime('now') \
         RETURNING *",
    )
    .bind(id)
    .bind(&payload.name)
    .bind(&payload.description)
    .bind(&payload.file_mask)
    .bind(payload.is_default)
    .bind(&payload.definition)
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(flow)
}

pub async fn delete_flow(pool: &SqlitePool, id: &str) -> AppResult<()> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let is_default: Option<bool> = sqlx::query_scalar("SELECT is_default FROM flows WHERE id=?1")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;
    match is_default {
        Some(true) => Err(AppError::Conflict),
        Some(false) => {
            sqlx::query(
                "UPDATE libraries SET flow_id=NULL, updated_at=datetime('now') WHERE flow_id=?1",
            )
            .bind(id)
            .execute(&mut *tx)
            .await?;
            sqlx::query("DELETE FROM flows WHERE id=?1")
                .bind(id)
                .execute(&mut *tx)
                .await?;
            tx.commit().await?;
            Ok(())
        }
        None => Err(AppError::NotFound),
    }
}
