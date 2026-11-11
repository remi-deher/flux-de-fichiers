//! Gestionnaires HTTP pour l'API REST des flux de traitement (Flows).

use super::AppState;
use crate::{
    domain::{FlowRecord, UpdateFlowRequest},
    error::{AppError, AppResult},
};
use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
};

/// Liste l'ensemble des flux configurés en base de données.
pub async fn list_flows(
    State(s): State<AppState>,
    h: HeaderMap,
) -> AppResult<Json<Vec<FlowRecord>>> {
    crate::web_session::authorize_admin(&s.pool, &s.config, &h, &axum::http::Method::GET).await?;
    let flows =
        sqlx::query_as::<_, FlowRecord>("SELECT * FROM flows ORDER BY is_default DESC, name ASC")
            .fetch_all(&s.pool)
            .await?;
    Ok(Json(flows))
}

/// Récupère la définition détaillée d'un flux spécifique.
pub async fn get_flow(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> AppResult<Json<FlowRecord>> {
    crate::web_session::authorize_admin(&s.pool, &s.config, &h, &axum::http::Method::GET).await?;
    let flow = sqlx::query_as::<_, FlowRecord>("SELECT * FROM flows WHERE id = ?1")
        .bind(id)
        .fetch_optional(&s.pool)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(Json(flow))
}

/// Enregistre ou met à jour la définition d'un flux.
pub async fn save_flow(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
    Json(payload): Json<UpdateFlowRequest>,
) -> AppResult<(StatusCode, Json<FlowRecord>)> {
    crate::web_session::authorize_admin(&s.pool, &s.config, &h, &axum::http::Method::PUT).await?;

    if payload.name.trim().is_empty() {
        return Err(AppError::Invalid);
    }

    let mut tx = s.pool.begin_with("BEGIN IMMEDIATE").await?;

    if payload.is_default {
        // Un seul flux par défaut à la fois
        sqlx::query("UPDATE flows SET is_default = 0")
            .execute(&mut *tx)
            .await?;
    }

    let flow = sqlx::query_as::<_, FlowRecord>(
        "INSERT INTO flows (id, name, description, file_mask, is_default, definition, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, datetime('now'))
         ON CONFLICT(id) DO UPDATE SET
            name = excluded.name,
            description = excluded.description,
            file_mask = excluded.file_mask,
            is_default = excluded.is_default,
            definition = excluded.definition,
            updated_at = datetime('now')
         RETURNING *",
    )
    .bind(id)
    .bind(payload.name)
    .bind(payload.description)
    .bind(payload.file_mask)
    .bind(payload.is_default)
    .bind(payload.definition)
    .fetch_one(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok((StatusCode::OK, Json(flow)))
}

/// Supprime un flux personnalisé (interdit sur le flux actif par défaut).
pub async fn delete_flow(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> AppResult<StatusCode> {
    crate::web_session::authorize_admin(&s.pool, &s.config, &h, &axum::http::Method::DELETE)
        .await?;

    let is_default: Option<bool> = sqlx::query_scalar("SELECT is_default FROM flows WHERE id = ?1")
        .bind(&id)
        .fetch_optional(&s.pool)
        .await?;

    match is_default {
        Some(true) => Err(AppError::Conflict),
        Some(false) => {
            sqlx::query("DELETE FROM flows WHERE id = ?1")
                .bind(id)
                .execute(&s.pool)
                .await?;
            Ok(StatusCode::NO_CONTENT)
        }
        None => Err(AppError::NotFound),
    }
}
