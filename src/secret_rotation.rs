//! Rotation de la clé de chiffrement des secrets (ADR 0011).
//!
//! Tous les secrets stockés (réglages de plugins, en-têtes de webhook) sont rechiffrés avec une clé
//! neuve en une seule transaction : si une valeur ne se déchiffre pas, rien ne change. Avant d'y toucher,
//! l'ancienne clé, la nouvelle et une copie cohérente de la base sont rangées dans
//! `<config>/backups/secret-rotation-<date>-<ancienne clé>/`.

use crate::{
    error::{AppError, AppResult},
    flows, plugins, repository,
    secrets::SecretBox,
    worker_settings,
};
use chrono::Utc;
use sqlx::SqlitePool;
use std::path::{Path, PathBuf};

const BACKUPS_DIR: &str = "backups";
const BACKUP_PREFIX: &str = "secret-rotation";
const DATABASE_BACKUP_NAME: &str = "flux.db";
const OLD_KEY_BACKUP_NAME: &str = "secret_key";
const NEW_KEY_BACKUP_NAME: &str = "secret_key.next";
const BACKUP_TIMESTAMP_FORMAT: &str = "%Y%m%d-%H%M%S";

/// Valeurs rechiffrées, par origine.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct RotationReport {
    pub plugin_values: usize,
    pub flow_values: usize,
}

#[derive(Debug)]
pub struct RotationOutcome {
    pub report: RotationReport,
    pub backup_dir: PathBuf,
    pub old_key_id: String,
    pub new_key_id: String,
}

/// Rechiffre toutes les valeurs stockées de `old` vers `new`, dans une seule transaction.
pub async fn rotate_values(
    pool: &SqlitePool,
    old: &SecretBox,
    new: &SecretBox,
) -> AppResult<RotationReport> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let mut report = RotationReport::default();
    let settings: Vec<(String, String)> =
        sqlx::query_as("SELECT plugin_id, settings FROM plugin_settings")
            .fetch_all(&mut *tx)
            .await?;
    for (plugin_id, stored) in settings {
        if let Some((rewritten, count)) = plugins::rotate_settings(old, new, &plugin_id, &stored)? {
            sqlx::query("UPDATE plugin_settings SET settings = ?1 WHERE plugin_id = ?2")
                .bind(rewritten)
                .bind(&plugin_id)
                .execute(&mut *tx)
                .await?;
            report.plugin_values += count;
        }
    }
    for store in &repository::DEFINITION_STORES {
        let definitions: Vec<(String, i64, String)> =
            sqlx::query_as(store.select).fetch_all(&mut *tx).await?;
        for (flow_id, rowid, definition) in definitions {
            if let Some((rewritten, count)) =
                flows::rotate_definition(old, new, &flow_id, &definition)?
            {
                sqlx::query(store.update)
                    .bind(rewritten)
                    .bind(rowid)
                    .execute(&mut *tx)
                    .await?;
                report.flow_values += count;
            }
        }
    }
    tx.commit().await?;
    Ok(report)
}

fn storage_error(error: anyhow::Error) -> AppError {
    tracing::error!(error = %error, "écriture de la sauvegarde de rotation impossible");
    AppError::Internal
}

/// Copie cohérente de la base (`VACUUM INTO`), lisible même pendant que le central écrit.
async fn backup_database(pool: &SqlitePool, destination: &Path) -> AppResult<()> {
    let literal = destination.to_string_lossy().replace('\'', "''");
    sqlx::query(&format!("VACUUM INTO '{literal}'"))
        .execute(pool)
        .await?;
    Ok(())
}

/// Fait tourner la clé locale : sauvegarde, rechiffrement en transaction, puis remplacement du fichier.
/// La clé fournie par `SECRET_KEY_FILE` n'est pas gérée ici : elle appartient à l'orchestrateur.
pub async fn rotate_local_key(pool: &SqlitePool, config_dir: &Path) -> AppResult<RotationOutcome> {
    let key_path = SecretBox::local_key_path(config_dir);
    let old = SecretBox::open_with(config_dir, None)?;
    let new_hex = SecretBox::generate_key_hex();
    let new = SecretBox::from_key_hex(&new_hex)?;

    let backup_dir = config_dir.join(BACKUPS_DIR).join(format!(
        "{BACKUP_PREFIX}-{}-{}",
        Utc::now().format(BACKUP_TIMESTAMP_FORMAT),
        old.key_id()
    ));
    std::fs::create_dir_all(&backup_dir)?;
    let old_hex = std::fs::read_to_string(&key_path)?;
    worker_settings::write_private(&backup_dir.join(OLD_KEY_BACKUP_NAME), old_hex.trim())
        .map_err(storage_error)?;
    worker_settings::write_private(&backup_dir.join(NEW_KEY_BACKUP_NAME), &new_hex)
        .map_err(storage_error)?;
    backup_database(pool, &backup_dir.join(DATABASE_BACKUP_NAME)).await?;

    let report = rotate_values(pool, &old, &new).await?;
    worker_settings::write_private(&key_path, &new_hex).map_err(storage_error)?;
    Ok(RotationOutcome {
        report,
        backup_dir,
        old_key_id: old.key_id().to_owned(),
        new_key_id: new.key_id().to_owned(),
    })
}
