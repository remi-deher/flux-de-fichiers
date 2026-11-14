//! État du système pour la page Paramètres : version, clé de chiffrement, taille de la base.

use super::{AdminAuth, AppState};
use crate::{
    config::Config,
    error::AppResult,
    secrets::{KEY_FILE_ENV, SecretBox},
    telemetry::TelemetryProbe,
};
use axum::{Json, extract::State};
pub use flux_shared::system::{KeySource, NetworkStatus, StorageStatus, SystemStatus, VolumeSpace};
use std::path::{Path, PathBuf};

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

fn display(path: &Path) -> String {
    path.display().to_string()
}

pub fn network_status(config: &Config) -> NetworkStatus {
    NetworkStatus {
        bind: config.bind.to_string(),
        tls: config.tls.is_some(),
        web_origin: config.web_origin.clone(),
        session_ttl_seconds: config.web_session_ttl_seconds,
    }
}

pub fn storage_status(config: &Config) -> StorageStatus {
    StorageStatus {
        config_dir: display(&config.config_dir),
        data_dir: display(&config.data_dir),
        input_dir: display(&config.input_dir),
        tools_cache_dir: display(&config.tools_cache_dir),
        plugins_dir: config.plugins_dir.as_deref().map(display),
        max_file_bytes: config.max_file_bytes,
        volume: TelemetryProbe::read_storage_info(&config.data_dir).map(
            |(total_bytes, available_bytes)| VolumeSpace {
                total_bytes,
                available_bytes,
            },
        ),
    }
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
        network: network_status(&state.config),
        storage: storage_status(&state.config),
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

    fn config(dir: &Path) -> Config {
        Config {
            bind: ([127, 0, 0, 1], 3000).into(),
            database_url: "sqlite::memory:".into(),
            data_dir: dir.to_path_buf(),
            input_dir: dir.join("input"),
            config_dir: dir.join("config"),
            tools_cache_dir: dir.join("tools"),
            plugins_dir: None,
            static_dir: "frontend/dist".into(),
            max_file_bytes: 2048,
            local_worker: false,
            web_origin: Some("https://flux.example".into()),
            web_session_ttl_seconds: 60,
            tls: None,
        }
    }

    #[test]
    fn test_network_status_reports_listening_address_tls_and_origin() {
        let mut config = config(Path::new("."));
        let network = network_status(&config);
        assert_eq!(network.bind, "127.0.0.1:3000");
        assert!(!network.tls);
        assert_eq!(network.web_origin.as_deref(), Some("https://flux.example"));
        assert_eq!(network.session_ttl_seconds, 60);
        config.tls = Some(("cert.pem".into(), "key.pem".into()));
        assert!(network_status(&config).tls);
    }

    #[test]
    fn test_storage_status_lists_folders_and_reads_the_volume_space() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let storage = storage_status(&config(dir.path()));
        assert_eq!(storage.max_file_bytes, 2048);
        assert_eq!(storage.plugins_dir, None);
        assert!(storage.input_dir.ends_with("input"));
        let volume = storage
            .volume
            .ok_or_else(|| anyhow::anyhow!("volume illisible"))?;
        assert!(volume.total_bytes >= volume.available_bytes);
        let missing = storage_status(&config(&dir.path().join("absent")));
        assert_eq!(missing.volume, None);
        Ok(())
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
