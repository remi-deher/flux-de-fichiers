//! Reprise des dépôts que le moteur lisait avant l'ADR 0029 : ceux dont les éléments voulus ne sont pas
//! encore tous figés. Le moteur lit encore leur copie.

use super::set_setting;
use crate::error::{AppError, AppResult};
use sqlx::{SqliteConnection, SqlitePool};

/// Réglage d'instance : identifiants des dépôts à reprendre, en JSON.
const STORE_MIGRATION_KEY: &str = "store_migration";

/// Dépôts à reprendre ; `None` tant que la reprise n'a pas commencé, liste vide une fois finie.
pub async fn pending_store_repositories(pool: &SqlitePool) -> AppResult<Option<Vec<String>>> {
    let mut conn = pool.acquire().await?;
    read_pending(&mut conn).await
}

async fn read_pending(conn: &mut SqliteConnection) -> AppResult<Option<Vec<String>>> {
    let raw: Option<String> =
        sqlx::query_scalar("SELECT value FROM instance_settings WHERE key = ?1")
            .bind(STORE_MIGRATION_KEY)
            .fetch_optional(conn)
            .await?;
    raw.map(|raw| {
        serde_json::from_str(&raw).map_err(|error| {
            tracing::error!(%error, "état de la reprise des dépôts illisible");
            AppError::Internal
        })
    })
    .transpose()
}

/// Reste-t-il des dépôts à reprendre, ou la reprise à commencer ?
pub async fn store_migration_is_done(pool: &SqlitePool) -> AppResult<bool> {
    Ok(pending_store_repositories(pool)
        .await?
        .is_some_and(|pending| pending.is_empty()))
}

pub async fn set_pending_store_repositories(
    pool: &SqlitePool,
    pending: &[String],
) -> AppResult<()> {
    let raw = serde_json::to_string(pending).map_err(|_| AppError::Internal)?;
    set_setting(pool, STORE_MIGRATION_KEY, &raw).await
}

/// Un dépôt retiré n'est plus à reprendre : rajouté sous le même identifiant, il peut venir d'ailleurs.
pub(super) async fn forget_pending_repository(
    conn: &mut SqliteConnection,
    repository_id: &str,
) -> AppResult<()> {
    let Some(mut pending) = read_pending(conn).await? else {
        return Ok(());
    };
    pending.retain(|id| id != repository_id);
    let raw = serde_json::to_string(&pending).map_err(|_| AppError::Internal)?;
    sqlx::query("UPDATE instance_settings SET value = ?2 WHERE key = ?1")
        .bind(STORE_MIGRATION_KEY)
        .bind(raw)
        .execute(conn)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    #[tokio::test]
    async fn not_started_started_and_done_are_told_apart() -> anyhow::Result<()> {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await?;
        sqlx::migrate!().run(&pool).await?;
        assert_eq!(pending_store_repositories(&pool).await?, None);
        assert!(!store_migration_is_done(&pool).await?);

        let pending = ["repo.official".to_owned()];
        set_pending_store_repositories(&pool, &pending).await?;
        let listed = pending_store_repositories(&pool).await?;
        assert_eq!(listed.as_deref(), Some(pending.as_slice()));
        assert!(!store_migration_is_done(&pool).await?);

        let mut conn = pool.acquire().await?;
        forget_pending_repository(&mut conn, "repo.autre").await?;
        assert_eq!(read_pending(&mut conn).await?.as_deref(), listed.as_deref());
        forget_pending_repository(&mut conn, "repo.official").await?;
        drop(conn);
        assert!(store_migration_is_done(&pool).await?);

        // Un état illisible n'est ni fini, ni à recommencer.
        set_setting(&pool, STORE_MIGRATION_KEY, "?").await?;
        assert!(pending_store_repositories(&pool).await.is_err());
        Ok(())
    }
}
