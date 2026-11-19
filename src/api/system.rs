//! État du système pour la page Paramètres : version, clé de chiffrement, taille de la base.

use super::{AdminAuth, AppState, ViewOverviewAuth};
use crate::{
    config::Config,
    error::AppResult,
    runtime_settings::{self, EffectiveRuntimeSettings},
    secret_rotation::{self, RETIRED_KEY_GRACE},
    secrets::{self, SecretBox},
    telemetry::{TelemetryProbe, TelemetrySnapshot},
};
use axum::{Json, extract::State};
pub use forgearr_shared::system::{
    HardwareStatus, KeySource, NetworkStatus, StorageStatus, SystemStatus, UpdateCheckStatus,
    UpdateSystemSettingsRequest, VolumeSpace,
};
use std::{
    path::{Path, PathBuf},
    sync::LazyLock,
    time::{Duration, Instant},
};

const GITHUB_RELEASES_API: &str =
    "https://api.github.com/repos/remi-deher/flux-de-fichiers/releases/latest";
const UPDATE_CHECK_CACHE_TTL: Duration = Duration::from_secs(60 * 60);
const UPDATE_CHECK_TIMEOUT: Duration = Duration::from_secs(5);

struct CachedUpdateCheck {
    fetched_at: Instant,
    status: UpdateCheckStatus,
}

static UPDATE_CACHE: LazyLock<tokio::sync::Mutex<Option<CachedUpdateCheck>>> =
    LazyLock::new(|| tokio::sync::Mutex::new(None));

/// Compare deux étiquettes semver (`v0.41.0` et `0.42.0`) ; retourne `true` si `latest > current`.
pub fn is_newer_version(current: &str, latest: &str) -> bool {
    let parse = |raw: &str| -> Option<(u64, u64, u64)> {
        let clean = raw.trim().trim_start_matches('v').split('-').next()?;
        let mut parts = clean.split('.');
        let major = parts.next()?.parse().ok()?;
        let minor = parts.next()?.parse().ok()?;
        let patch = parts.next()?.parse().ok()?;
        Some((major, minor, patch))
    };
    match (parse(current), parse(latest)) {
        (Some(cur), Some(lat)) => lat > cur,
        _ => false,
    }
}

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

/// Relevé matériel du central ; un volume de capacité nulle est présenté comme illisible.
pub fn hardware_status(snapshot: TelemetrySnapshot) -> HardwareStatus {
    HardwareStatus {
        logical_cores: snapshot.logical_cores,
        cpu_usage_percent: snapshot.cpu_usage_percent,
        ram_total_bytes: snapshot.ram_total_bytes,
        ram_used_bytes: snapshot.ram_used_bytes,
        gpu_devices: snapshot.gpu_devices,
        volume: (snapshot.storage_total_bytes > 0).then_some(VolumeSpace {
            total_bytes: snapshot.storage_total_bytes,
            available_bytes: snapshot.storage_available_bytes,
        }),
    }
}

pub async fn hardware(
    State(state): State<AppState>,
    _auth: ViewOverviewAuth,
) -> Json<HardwareStatus> {
    let probe = TelemetryProbe::with_storage_path(&state.config.data_dir);
    Json(hardware_status(probe.sample()))
}

fn build_system_status(effective: &EffectiveRuntimeSettings, secrets: &SecretBox) -> SystemStatus {
    SystemStatus {
        version: crate::version::app_version().to_owned(),
        build_version: crate::version::BUILD_VERSION.to_owned(),
        key_id: secrets.key_id().to_owned(),
        retired_key_ids: secrets
            .retired_key_ids()
            .into_iter()
            .map(str::to_owned)
            .collect(),
        recovery_kit_key_id: secrets::recovery_kit::exported_key_id(&effective.config.config_dir)
            .ok()
            .flatten(),
        key_source: if secrets::external_key_file().is_some() {
            KeySource::External
        } else {
            KeySource::Local
        },
        database_bytes: database_size(&effective.config.database_url),
        network: network_status(&effective.config),
        storage: storage_status(&effective.config),
        smtp: effective.smtp_status.clone(),
        outbound: effective.outbound,
        check_updates_enabled: effective.check_updates_enabled,
    }
}

pub(super) async fn current_status(state: &AppState) -> AppResult<Json<SystemStatus>> {
    let secrets = SecretBox::open(&state.config.config_dir)?;
    let effective = runtime_settings::load_effective(&state.pool, &state.config).await;
    Ok(Json(build_system_status(&effective, &secrets)))
}

pub async fn status(
    State(state): State<AppState>,
    _auth: AdminAuth,
) -> AppResult<Json<SystemStatus>> {
    current_status(&state).await
}

/// Renouvelle la clé de chiffrement central en marche : la nouvelle clé est active dès la réponse,
/// l'ancienne est retirée en tâche de fond une fois tous les secrets rechiffrés. 409 si la clé vient
/// de `SECRET_KEY_FILE` ou si une rotation est déjà en cours.
pub async fn rotate_key(
    State(state): State<AppState>,
    _auth: AdminAuth,
) -> AppResult<Json<SystemStatus>> {
    secret_rotation::start_online_rotation(
        state.pool.clone(),
        state.config.config_dir.clone(),
        secrets::external_key_file(),
        RETIRED_KEY_GRACE,
    )?;
    current_status(&state).await
}

/// Met à jour les réglages réseau, session, SMTP et politique sortante (Lots B & C).
pub async fn update_settings(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Json(request): Json<UpdateSystemSettingsRequest>,
) -> AppResult<Json<SystemStatus>> {
    let secrets = SecretBox::open(&state.config.config_dir)?;
    let effective = runtime_settings::update(&state.pool, &state.config, request).await?;
    Ok(Json(build_system_status(&effective, &secrets)))
}

#[derive(serde::Deserialize)]
struct GithubLatestRelease {
    tag_name: String,
    html_url: Option<String>,
}

/// Vérifie la dernière version publiée sur GitHub si l'administrateur a activé l'option.
pub async fn updates(State(state): State<AppState>, _auth: AdminAuth) -> Json<UpdateCheckStatus> {
    let current = crate::version::app_version().to_owned();
    let effective = runtime_settings::load_effective(&state.pool, &state.config).await;
    if !effective.check_updates_enabled {
        return Json(UpdateCheckStatus {
            enabled: false,
            current_version: current,
            latest_version: None,
            update_available: false,
            release_url: None,
            checked_at: None,
            error: None,
        });
    }
    {
        let cache = UPDATE_CACHE.lock().await;
        if let Some(cached) = cache.as_ref()
            && cached.fetched_at.elapsed() < UPDATE_CHECK_CACHE_TTL
        {
            return Json(cached.status.clone());
        }
    }
    let client = reqwest::Client::builder()
        .timeout(UPDATE_CHECK_TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
        .build();
    let result = match client {
        Ok(client) => {
            let resp = client
                .get(GITHUB_RELEASES_API)
                .header(reqwest::header::USER_AGENT, format!("forgearr/{current}"))
                .send()
                .await;
            match resp {
                Ok(r) if r.status().is_success() => match r.json::<GithubLatestRelease>().await {
                    Ok(rel) => {
                        let latest = rel.tag_name.trim_start_matches('v').to_owned();
                        let update_available = is_newer_version(&current, &latest);
                        UpdateCheckStatus {
                            enabled: true,
                            current_version: current,
                            latest_version: Some(latest),
                            update_available,
                            release_url: rel.html_url,
                            checked_at: Some(chrono::Utc::now()),
                            error: None,
                        }
                    }
                    Err(_) => UpdateCheckStatus {
                        enabled: true,
                        current_version: current,
                        latest_version: None,
                        update_available: false,
                        release_url: None,
                        checked_at: Some(chrono::Utc::now()),
                        error: Some("Réponse GitHub illisible".to_owned()),
                    },
                },
                Ok(r) => UpdateCheckStatus {
                    enabled: true,
                    current_version: current,
                    latest_version: None,
                    update_available: false,
                    release_url: None,
                    checked_at: Some(chrono::Utc::now()),
                    error: Some(format!("HTTP {}", r.status().as_u16())),
                },
                Err(_) => UpdateCheckStatus {
                    enabled: true,
                    current_version: current,
                    latest_version: None,
                    update_available: false,
                    release_url: None,
                    checked_at: Some(chrono::Utc::now()),
                    error: Some("Dépôt GitHub injoignable".to_owned()),
                },
            }
        }
        Err(_) => UpdateCheckStatus {
            enabled: true,
            current_version: current,
            latest_version: None,
            update_available: false,
            release_url: None,
            checked_at: Some(chrono::Utc::now()),
            error: Some("Client HTTP indisponible".to_owned()),
        },
    };
    if result.error.is_none() {
        let mut cache = UPDATE_CACHE.lock().await;
        *cache = Some(CachedUpdateCheck {
            fetched_at: Instant::now(),
            status: result.clone(),
        });
    }
    Json(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_newer_version_compares_semver_triplets() {
        assert!(is_newer_version("0.41.0", "0.42.0"));
        assert!(is_newer_version("v0.41.0", "0.41.1"));
        assert!(!is_newer_version("0.41.0", "0.41.0"));
        assert!(!is_newer_version("0.42.0", "0.41.9"));
    }

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
            smtp: None,
        }
    }

    fn snapshot(storage_total_bytes: u64) -> TelemetrySnapshot {
        TelemetrySnapshot {
            timestamp: chrono::Utc::now(),
            logical_cores: 4,
            cpu_usage_percent: 25.0,
            load_average: [1.0, 0.5, 0.25],
            ram_total_bytes: 8192,
            ram_available_bytes: 2048,
            ram_used_bytes: 6144,
            gpu_detected: true,
            gpu_devices: vec!["/dev/dri/renderD128".into()],
            storage_total_bytes,
            storage_available_bytes: 100,
        }
    }

    #[test]
    fn test_hardware_status_keeps_the_load_and_hides_an_unreadable_volume() {
        let hardware = hardware_status(snapshot(1000));
        assert_eq!(hardware.logical_cores, 4);
        assert_eq!(hardware.ram_used_bytes, 6144);
        assert_eq!(hardware.gpu_devices, ["/dev/dri/renderD128"]);
        assert_eq!(
            hardware.volume,
            Some(VolumeSpace {
                total_bytes: 1000,
                available_bytes: 100
            })
        );
        assert_eq!(hardware_status(snapshot(0)).volume, None);
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
