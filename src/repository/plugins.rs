use crate::{
    error::{AppError, AppResult},
    plugins::PluginRepository,
};
use sqlx::SqlitePool;

pub async fn get_plugin_settings(pool: &SqlitePool, plugin_id: &str) -> AppResult<Option<String>> {
    let settings: Option<String> =
        sqlx::query_scalar("SELECT settings FROM plugin_settings WHERE plugin_id = ?1")
            .bind(plugin_id)
            .fetch_optional(pool)
            .await?;
    Ok(settings)
}

pub async fn set_plugin_settings(
    pool: &SqlitePool,
    plugin_id: &str,
    settings_json: &str,
) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO plugin_settings (plugin_id, settings, updated_at) \
         VALUES (?1, ?2, datetime('now')) \
         ON CONFLICT(plugin_id) DO UPDATE SET settings = ?2, updated_at = datetime('now')",
    )
    .bind(plugin_id)
    .bind(settings_json)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn list_plugin_repositories(pool: &SqlitePool) -> AppResult<Vec<PluginRepository>> {
    let repos = sqlx::query_as::<_, PluginRepository>(
        "SELECT id, name, index_url, is_official, enabled, last_synced_at, created_at \
         FROM plugin_repositories ORDER BY is_official DESC, created_at ASC",
    )
    .fetch_all(pool)
    .await?;
    Ok(repos)
}

pub async fn add_plugin_repository(
    pool: &SqlitePool,
    id: &str,
    name: &str,
    index_url: &str,
) -> AppResult<PluginRepository> {
    let repo = sqlx::query_as::<_, PluginRepository>(
        "INSERT INTO plugin_repositories (id, name, index_url, is_official, enabled) \
         VALUES (?1, ?2, ?3, 0, 1) \
         RETURNING id, name, index_url, is_official, enabled, last_synced_at, created_at",
    )
    .bind(id)
    .bind(name)
    .bind(index_url)
    .fetch_one(pool)
    .await?;
    Ok(repo)
}

pub async fn toggle_plugin_repository(pool: &SqlitePool, id: &str) -> AppResult<bool> {
    let current: Option<bool> =
        sqlx::query_scalar("SELECT enabled FROM plugin_repositories WHERE id = ?1")
            .bind(id)
            .fetch_optional(pool)
            .await?;
    let new_state = match current {
        Some(val) => !val,
        None => return Err(AppError::NotFound),
    };
    sqlx::query("UPDATE plugin_repositories SET enabled = ?1 WHERE id = ?2")
        .bind(new_state)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(new_state)
}

pub async fn delete_plugin_repository(pool: &SqlitePool, id: &str) -> AppResult<()> {
    let is_official: Option<bool> =
        sqlx::query_scalar("SELECT is_official FROM plugin_repositories WHERE id = ?1")
            .bind(id)
            .fetch_optional(pool)
            .await?;
    match is_official {
        Some(true) => Err(AppError::InvalidManifest(
            "Le catalogue officiel ne peut pas être supprimé.".to_string(),
        )),
        Some(false) => {
            sqlx::query("DELETE FROM plugin_repositories WHERE id = ?1")
                .bind(id)
                .execute(pool)
                .await?;
            Ok(())
        }
        None => Err(AppError::NotFound),
    }
}

pub async fn touch_plugin_repository_sync(pool: &SqlitePool, id: &str) -> AppResult<()> {
    let rows = sqlx::query(
        "UPDATE plugin_repositories SET last_synced_at = datetime('now') WHERE id = ?1",
    )
    .bind(id)
    .execute(pool)
    .await?
    .rows_affected();
    if rows == 0 {
        Err(AppError::NotFound)
    } else {
        Ok(())
    }
}
