//! Réglages et dépôts de plugins. Toute écriture visible exige de quoi la signaler (ADR 0028).

use crate::{
    error::{AppError, AppResult},
    live::ChangeNotifier,
    plugins::PluginRepository,
};
use forgearr_shared::live::ChangeTopic;
use sqlx::SqlitePool;

pub async fn get_plugin_settings(pool: &SqlitePool, plugin_id: &str) -> AppResult<Option<String>> {
    let settings: Option<String> = sqlx::query_scalar!(
        "SELECT settings FROM plugin_settings WHERE plugin_id = ?1",
        plugin_id
    )
    .fetch_optional(pool)
    .await?;
    Ok(settings)
}

/// Retourne les couples `(plugin_id, settings_json)` enregistrés en base.
pub async fn list_all_plugin_settings(
    pool: &SqlitePool,
) -> AppResult<std::collections::HashMap<String, String>> {
    let rows: Vec<(String, String)> =
        sqlx::query_as("SELECT plugin_id, settings FROM plugin_settings")
            .fetch_all(pool)
            .await?;
    Ok(rows.into_iter().collect())
}

pub async fn set_plugin_settings(
    pool: &SqlitePool,
    changes: &ChangeNotifier,
    plugin_id: &str,
    settings_json: &str,
) -> AppResult<()> {
    sqlx::query!(
        "INSERT INTO plugin_settings (plugin_id, settings, updated_at) \
         VALUES (?1, ?2, datetime('now')) \
         ON CONFLICT(plugin_id) DO UPDATE SET settings = ?2, updated_at = datetime('now')",
        plugin_id,
        settings_json,
    )
    .execute(pool)
    .await?;
    changes.notify(ChangeTopic::Settings);
    // L'état « à configurer » des pièces du catalogue en dépend.
    changes.notify(ChangeTopic::Catalog);
    Ok(())
}

pub async fn list_plugin_repositories(pool: &SqlitePool) -> AppResult<Vec<PluginRepository>> {
    let repos = sqlx::query_as!(
        PluginRepository,
        r#"SELECT id AS "id!: String", name, index_url, is_official AS "is_official: bool", enabled AS "enabled: bool", last_synced_at, created_at FROM plugin_repositories ORDER BY is_official DESC, created_at ASC"#,
    )
    .fetch_all(pool)
    .await?;
    Ok(repos)
}

pub async fn get_plugin_repository(pool: &SqlitePool, id: &str) -> AppResult<PluginRepository> {
    sqlx::query_as!(
        PluginRepository,
        r#"SELECT id AS "id!: String", name, index_url, is_official AS "is_official: bool", enabled AS "enabled: bool", last_synced_at, created_at FROM plugin_repositories WHERE id = ?1"#,
        id,
    )
    .fetch_optional(pool)
    .await?
    .ok_or(AppError::NotFound)
}

pub async fn add_plugin_repository(
    pool: &SqlitePool,
    changes: &ChangeNotifier,
    id: &str,
    name: &str,
    index_url: &str,
) -> AppResult<PluginRepository> {
    let repo = sqlx::query_as!(
        PluginRepository,
        r#"INSERT INTO plugin_repositories (id, name, index_url, is_official, enabled) VALUES (?1, ?2, ?3, 0, 1) RETURNING id AS "id!: String", name, index_url, is_official AS "is_official: bool", enabled AS "enabled: bool", last_synced_at, created_at"#,
        id,
        name,
        index_url,
    )
    .fetch_one(pool)
    .await?;
    changes.notify(ChangeTopic::Repositories);
    Ok(repo)
}

pub async fn toggle_plugin_repository(
    pool: &SqlitePool,
    changes: &ChangeNotifier,
    id: &str,
) -> AppResult<bool> {
    let current: Option<bool> = sqlx::query_scalar!(
        r#"SELECT enabled AS "enabled: bool" FROM plugin_repositories WHERE id = ?1"#,
        id
    )
    .fetch_optional(pool)
    .await?;
    let new_state = match current {
        Some(val) => !val,
        None => return Err(AppError::NotFound),
    };
    sqlx::query!(
        "UPDATE plugin_repositories SET enabled = ?1 WHERE id = ?2",
        new_state,
        id
    )
    .execute(pool)
    .await?;
    changes.notify(ChangeTopic::Repositories);
    Ok(new_state)
}

pub async fn delete_plugin_repository(
    pool: &SqlitePool,
    changes: &ChangeNotifier,
    id: &str,
) -> AppResult<()> {
    let is_official: Option<bool> = sqlx::query_scalar!(
        r#"SELECT is_official AS "is_official: bool" FROM plugin_repositories WHERE id = ?1"#,
        id
    )
    .fetch_optional(pool)
    .await?;
    match is_official {
        Some(true) => Err(AppError::InvalidManifest(
            "Le catalogue officiel ne peut pas être supprimé.".to_string(),
        )),
        Some(false) => {
            let mut tx = pool.begin().await?;
            sqlx::query!("DELETE FROM plugin_repositories WHERE id = ?1", id)
                .execute(&mut *tx)
                .await?;
            let repo_scope = format!("repo:{id}");
            sqlx::query!(
                "DELETE FROM plugin_release_pins WHERE scope = ?1",
                repo_scope
            )
            .execute(&mut *tx)
            .await?;
            tx.commit().await?;
            changes.notify(ChangeTopic::Repositories);
            Ok(())
        }
        None => Err(AppError::NotFound),
    }
}

pub async fn touch_plugin_repository_sync(
    pool: &SqlitePool,
    changes: &ChangeNotifier,
    id: &str,
) -> AppResult<()> {
    let rows = sqlx::query!(
        "UPDATE plugin_repositories SET last_synced_at = datetime('now') WHERE id = ?1",
        id
    )
    .execute(pool)
    .await?
    .rows_affected();
    if rows == 0 {
        Err(AppError::NotFound)
    } else {
        changes.notify(ChangeTopic::Repositories);
        Ok(())
    }
}

/// Épingle une release `(scope, version)` à son SHA-256 à sa première observation.
/// Retourne `Ok(None)` si valide ou nouvellement épinglée, ou `Ok(Some(expected_sha256))` si la version a muté.
pub async fn verify_or_pin_release(
    pool: &SqlitePool,
    scope: &str,
    version: &str,
    actual_sha256: &str,
    source_url: &str,
) -> AppResult<Option<String>> {
    let normalized = actual_sha256.trim().to_ascii_lowercase();
    let existing: Option<String> = sqlx::query_scalar!(
        "SELECT sha256 FROM plugin_release_pins WHERE scope = ?1 AND version = ?2",
        scope,
        version
    )
    .fetch_optional(pool)
    .await?;

    if let Some(expected) = existing {
        if expected == normalized {
            return Ok(None);
        }
        return Ok(Some(expected));
    }

    sqlx::query!(
        "INSERT INTO plugin_release_pins (scope, version, sha256, source_url) \
         VALUES (?1, ?2, ?3, ?4) \
         ON CONFLICT(scope, version) DO NOTHING",
        scope,
        version,
        normalized,
        source_url,
    )
    .execute(pool)
    .await?;

    let persisted: Option<String> = sqlx::query_scalar!(
        "SELECT sha256 FROM plugin_release_pins WHERE scope = ?1 AND version = ?2",
        scope,
        version
    )
    .fetch_optional(pool)
    .await?;
    Ok(persisted.filter(|expected| expected != &normalized))
}
