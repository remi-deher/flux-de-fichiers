use crate::{
    domain::{Claim, Job, LEASE_SECONDS, LeaseRequest, MAX_ATTEMPTS, Worker},
    error::{AppError, AppResult},
};
use sqlx::SqlitePool;
use uuid::Uuid;

pub async fn register(pool: &SqlitePool, name: &str) -> AppResult<Worker> {
    Ok(sqlx::query_as("INSERT INTO workers(id,name) VALUES(?1,?2) ON CONFLICT(name) DO UPDATE SET last_seen=datetime('now') RETURNING *")
        .bind(Uuid::new_v4()).bind(name).fetch_one(pool).await?)
}

pub async fn claim(pool: &SqlitePool, worker: Uuid) -> AppResult<Option<Claim>> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    if sqlx::query("UPDATE workers SET last_seen=datetime('now') WHERE id=?1")
        .bind(worker)
        .execute(&mut *tx)
        .await?
        .rows_affected()
        != 1
    {
        return Err(AppError::NotFound);
    }
    sqlx::query("UPDATE jobs SET status=CASE WHEN attempts >= ?1 THEN 'failed' ELSE 'queued' END, error_code='lease_expired', lease_token=NULL, leased_until=NULL, updated_at=datetime('now') WHERE status='running' AND leased_until <= datetime('now')")
        .bind(MAX_ATTEMPTS).execute(&mut *tx).await?;
    let next_id: Option<Uuid> = sqlx::query_scalar("SELECT id FROM jobs WHERE status='queued' AND attempts < ?1 AND (target_worker_id IS NULL OR target_worker_id=?2) ORDER BY created_at,id LIMIT 1")
        .bind(MAX_ATTEMPTS).bind(worker).fetch_optional(&mut *tx).await?;
    let job: Option<Job> = if let Some(id) = next_id {
        sqlx::query_as("UPDATE jobs SET status='running', worker_id=?2, lease_token=?3, leased_until=datetime('now', '+' || ?4 || ' seconds'), attempts=attempts+1, error_code=NULL, updated_at=datetime('now') WHERE id=?1 RETURNING *")
            .bind(id).bind(worker).bind(Uuid::new_v4()).bind(LEASE_SECONDS).fetch_optional(&mut *tx).await?
    } else {
        None
    };
    tx.commit().await?;
    job.map(|j| {
        Ok(Claim {
            job_id: j.id,
            lease_token: j.lease_token.ok_or(AppError::Internal)?,
            source_name: j.source_name,
            source_sha256: j.source_sha256.ok_or(AppError::Internal)?,
            source_bytes: j.source_bytes.ok_or(AppError::Internal)?,
        })
    })
    .transpose()
}

pub async fn active(pool: &SqlitePool, id: Uuid, lease: &LeaseRequest) -> AppResult<Job> {
    sqlx::query_as("SELECT * FROM jobs WHERE id=?1 AND worker_id=?2 AND lease_token=?3 AND status='running' AND leased_until > datetime('now')")
        .bind(id).bind(lease.worker_id).bind(lease.lease_token).fetch_optional(pool).await?.ok_or(AppError::Conflict)
}

pub async fn heartbeat(pool: &SqlitePool, id: Uuid, lease: &LeaseRequest) -> AppResult<()> {
    let count = sqlx::query("UPDATE jobs SET leased_until=datetime('now', '+' || ?4 || ' seconds'),updated_at=datetime('now') WHERE id=?1 AND worker_id=?2 AND lease_token=?3 AND status='running' AND leased_until > datetime('now')")
        .bind(id).bind(lease.worker_id).bind(lease.lease_token).bind(LEASE_SECONDS).execute(pool).await?.rows_affected();
    if count != 1 {
        return Err(AppError::Conflict);
    }
    Ok(())
}

pub async fn fail(pool: &SqlitePool, id: Uuid, lease: &LeaseRequest, code: &str) -> AppResult<()> {
    let count = sqlx::query("UPDATE jobs SET status='failed',error_code=?4,lease_token=NULL,leased_until=NULL,updated_at=datetime('now') WHERE id=?1 AND worker_id=?2 AND lease_token=?3 AND status='running' AND leased_until > datetime('now')")
        .bind(id).bind(lease.worker_id).bind(lease.lease_token).bind(code).execute(pool).await?.rows_affected();
    if count != 1 {
        return Err(AppError::Conflict);
    }
    Ok(())
}

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
    sqlx::query("INSERT INTO plugin_settings (plugin_id, settings, updated_at) VALUES (?1, ?2, datetime('now')) ON CONFLICT(plugin_id) DO UPDATE SET settings = ?2, updated_at = datetime('now')")
        .bind(plugin_id)
        .bind(settings_json)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn list_plugin_repositories(
    pool: &SqlitePool,
) -> AppResult<Vec<crate::plugins::PluginRepository>> {
    let repos = sqlx::query_as::<_, crate::plugins::PluginRepository>(
        "SELECT id, name, index_url, is_official, enabled, last_synced_at, created_at FROM plugin_repositories ORDER BY is_official DESC, created_at ASC"
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
) -> AppResult<crate::plugins::PluginRepository> {
    let repo = sqlx::query_as::<_, crate::plugins::PluginRepository>(
        "INSERT INTO plugin_repositories (id, name, index_url, is_official, enabled) VALUES (?1, ?2, ?3, 0, 1) RETURNING id, name, index_url, is_official, enabled, last_synced_at, created_at"
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
