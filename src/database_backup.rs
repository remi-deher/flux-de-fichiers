//! Copie de la base avant une migration sans retour arrière : une version antérieure de Forgearr
//! refuse ensuite de démarrer sur la même base (ADR 0029).

use anyhow::Context;
use sqlx::SqlitePool;
use std::path::{Path, PathBuf};

/// Migration `0050_installed_items`.
const STORE_MIGRATION: i64 = 50;
const STORE_BACKUP_NAME: &str = "flux-avant-0050.db";
/// Comme la base, la copie porte des empreintes de mots de passe et des secrets chiffrés.
const BACKUP_MODE: u32 = 0o600;

/// Copie la base dans `backups_dir` si la migration 0050 va s'appliquer à une base déjà en service.
/// Rend le chemin de la copie ; toute erreur doit arrêter le démarrage.
pub async fn backup_before_store_migration(
    pool: &SqlitePool,
    backups_dir: &Path,
) -> anyhow::Result<Option<PathBuf>> {
    if !store_migration_pending(pool).await? {
        return Ok(None);
    }
    let destination = backups_dir.join(STORE_BACKUP_NAME);
    copy_database(pool, backups_dir, &destination)
        .await
        .with_context(|| {
            format!(
                "copie de la base vers {} impossible : démarrage arrêté avant la migration",
                destination.display()
            )
        })?;
    tracing::info!(backup = %destination.display(), "base copiée avant la migration 0050");
    Ok(Some(destination))
}

/// Une base neuve n'a aucune migration inscrite : il n'y a rien à copier.
async fn store_migration_pending(pool: &SqlitePool) -> anyhow::Result<bool> {
    let known: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = '_sqlx_migrations'",
    )
    .fetch_one(pool)
    .await?;
    if known == 0 {
        return Ok(false);
    }
    let applied: Vec<i64> = sqlx::query_scalar("SELECT version FROM _sqlx_migrations")
        .fetch_all(pool)
        .await?;
    Ok(!applied.is_empty() && !applied.contains(&STORE_MIGRATION))
}

/// Écrit la copie à part, la synchronise sur disque, puis la renomme. Une copie plus ancienne est
/// remplacée : la base encore sans la migration est le meilleur point de retour.
async fn copy_database(
    pool: &SqlitePool,
    backups_dir: &Path,
    destination: &Path,
) -> anyhow::Result<()> {
    tokio::fs::create_dir_all(backups_dir).await?;
    let staging = backups_dir.join(format!(".{STORE_BACKUP_NAME}.tmp"));
    // Reste d'un démarrage interrompu pendant la copie.
    remove_if_present(&staging).await?;
    let published = match write_copy(pool, &staging).await {
        Ok(()) => tokio::fs::rename(&staging, destination)
            .await
            .map_err(anyhow::Error::from),
        Err(error) => Err(error),
    };
    if let Err(error) = published {
        let _ = remove_if_present(&staging).await;
        return Err(error);
    }
    tokio::fs::File::open(backups_dir).await?.sync_all().await?;
    Ok(())
}

async fn remove_if_present(path: &Path) -> std::io::Result<()> {
    match tokio::fs::remove_file(path).await {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}

async fn write_copy(pool: &SqlitePool, staging: &Path) -> anyhow::Result<()> {
    // SQLite accepte un fichier vide pour destination : il naît ainsi avec ses droits restreints.
    tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(BACKUP_MODE)
        .open(staging)
        .await?;
    let path = staging.to_str().context("chemin de sauvegarde non UTF-8")?;
    sqlx::query("VACUUM INTO ?1")
        .bind(path)
        .execute(pool)
        .await?;
    // `VACUUM INTO` ne synchronise pas son fichier sur disque.
    tokio::fs::File::open(staging).await?.sync_all().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
    use std::os::unix::fs::PermissionsExt;

    /// Base dont la dernière migration inscrite est `version`.
    async fn database(path: &Path, version: i64) -> anyhow::Result<SqlitePool> {
        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true);
        let pool = SqlitePoolOptions::new().connect_with(options).await?;
        sqlx::query("CREATE TABLE _sqlx_migrations (version BIGINT PRIMARY KEY)")
            .execute(&pool)
            .await?;
        sqlx::query("INSERT INTO _sqlx_migrations (version) VALUES (?1)")
            .bind(version)
            .execute(&pool)
            .await?;
        Ok(pool)
    }

    #[tokio::test]
    async fn a_database_in_service_is_copied_before_the_migration() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let backups = dir.path().join("backups");
        let pool = database(&dir.path().join("flux.db"), STORE_MIGRATION - 1).await?;
        // Deux démarrages interrompus avant la migration : la copie est refaite, sans reste.
        for _ in 0..2 {
            let interrupted = backups.join(format!(".{STORE_BACKUP_NAME}.tmp"));
            std::fs::create_dir_all(&backups)?;
            std::fs::write(interrupted, "reste")?;
            let copy = backup_before_store_migration(&pool, &backups)
                .await?
                .context("copie attendue")?;
            let mode = std::fs::metadata(&copy)?.permissions().mode();
            assert_eq!(mode & 0o777, BACKUP_MODE);
            let options = SqliteConnectOptions::new().filename(&copy);
            let restored = SqlitePoolOptions::new().connect_with(options).await?;
            let versions: Vec<i64> = sqlx::query_scalar("SELECT version FROM _sqlx_migrations")
                .fetch_all(&restored)
                .await?;
            restored.close().await;
            assert_eq!(versions, [STORE_MIGRATION - 1]);
        }
        assert_eq!(std::fs::read_dir(&backups)?.count(), 1);
        Ok(())
    }

    #[tokio::test]
    async fn a_new_or_already_migrated_database_is_not_copied() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let backups = dir.path().join("backups");
        let blank = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await?;
        assert!(
            backup_before_store_migration(&blank, &backups)
                .await?
                .is_none()
        );
        sqlx::migrate!().run(&blank).await?;
        assert!(
            backup_before_store_migration(&blank, &backups)
                .await?
                .is_none()
        );
        assert!(!backups.exists());
        Ok(())
    }

    #[tokio::test]
    async fn a_failed_copy_is_an_error_that_leaves_nothing_behind() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        // Un fichier à la place du dossier de sauvegarde.
        let backups = dir.path().join("backups");
        std::fs::write(&backups, "x")?;
        let pool = database(&dir.path().join("flux.db"), STORE_MIGRATION - 1).await?;
        assert!(
            backup_before_store_migration(&pool, &backups)
                .await
                .is_err()
        );
        Ok(())
    }
}
