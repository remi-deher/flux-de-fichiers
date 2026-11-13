//! État du système pour la page Paramètres : version, clé de chiffrement, taille de la base.

use super::{AdminAuth, AppState};
use crate::{
    error::AppResult,
    secrets::{KEY_FILE_ENV, SecretBox},
};
use axum::{Json, extract::State};
pub use flux_shared::system::{KeySource, SystemStatus};
use std::path::PathBuf;

/// Chemin du fichier SQLite d'une URL de base (`sqlite:///config/flux.db?mode=rwc`) ; `None` en mémoire.
pub fn database_file(database_url: &str) -> Option<PathBuf> {
    let rest = database_url.strip_prefix("sqlite:").unwrap_or(database_url);
    let rest = rest.strip_prefix("//").unwrap_or(rest);
    let path = rest.split('?').next().unwrap_or_default();
    (!path.is_empty() && !path.contains(":memory:")).then(|| PathBuf::from(path))
}

/// Octets occupés par la base et son journal WAL.
fn database_size(database_url: &str) -> Option<u64> {
    let file = database_file(database_url)?;
    let size_of = |path: &std::path::Path| std::fs::metadata(path).map(|meta| meta.len()).ok();
    let main = size_of(&file)?;
    let mut journal = file.into_os_string();
    journal.push("-wal");
    Some(main + size_of(std::path::Path::new(&journal)).unwrap_or(0))
}

pub async fn status(
    State(state): State<AppState>,
    _auth: AdminAuth,
) -> AppResult<Json<SystemStatus>> {
    let secrets = SecretBox::open(&state.config.config_dir)?;
    Ok(Json(SystemStatus {
        version: env!("CARGO_PKG_VERSION").to_owned(),
        key_id: secrets.key_id().to_owned(),
        key_source: if std::env::var(KEY_FILE_ENV).is_ok() {
            KeySource::External
        } else {
            KeySource::Local
        },
        database_bytes: database_size(&state.config.database_url),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_database_file_is_read_from_sqlite_urls() {
        for (url, expected) in [
            ("sqlite:///config/flux.db", Some("/config/flux.db")),
            ("sqlite://var/config/flux.db", Some("var/config/flux.db")),
            ("sqlite:///config/flux.db?mode=rwc", Some("/config/flux.db")),
            ("/config/flux.db", Some("/config/flux.db")),
            ("sqlite::memory:", None),
            ("sqlite://:memory:", None),
            ("", None),
        ] {
            assert_eq!(database_file(url), expected.map(PathBuf::from), "{url}");
        }
    }

    #[test]
    fn test_size_adds_the_wal_journal_and_ignores_missing_files() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let db = dir.path().join("flux.db");
        let url = format!("sqlite://{}", db.display());
        assert_eq!(database_size(&url), None);
        std::fs::write(&db, vec![0u8; 100])?;
        assert_eq!(database_size(&url), Some(100));
        std::fs::write(dir.path().join("flux.db-wal"), vec![0u8; 40])?;
        assert_eq!(database_size(&url), Some(140));
        assert_eq!(database_size("sqlite::memory:"), None);
        Ok(())
    }
}
