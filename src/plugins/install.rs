//! Installation d'un outil distribué dans le cache local, d'après son contrat de distribution.

use super::{CatalogItemType, SyncPolicy, is_mutable_branch_url, sync::fetch_archive};
use crate::{
    error::{AppError, AppResult},
    tools_cache::{ToolArtifact, ToolCacheManager, ToolDistribution, host_arch},
};
use reqwest::Url;
use sqlx::SqlitePool;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{Arc, PoisonError},
};

const DISTRIBUTION_FILE_NAME: &str = "distribution.v1.json";

/// Outils dont l'installation est en cours sur ce worker (en file d'attente ou en téléchargement).
static INSTALLING: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

/// L'outil est-il en cours d'installation sur ce worker ?
pub fn is_installing(tool_id: &str) -> bool {
    INSTALLING
        .lock()
        .is_ok_and(|installing| installing.iter().any(|id| id == tool_id))
}

/// Marque un outil « en cours » le temps de l'installation, même en cas d'échec ou d'annulation.
struct InstallingGuard {
    tool_id: String,
    cache: ToolCacheManager,
}

impl InstallingGuard {
    fn start(tool_id: &str, cache: &ToolCacheManager) -> Self {
        if let Ok(mut installing) = INSTALLING.lock()
            && !installing.iter().any(|id| id == tool_id)
        {
            installing.push(tool_id.to_owned());
        }
        cache.notify_changed();
        Self {
            tool_id: tool_id.to_owned(),
            cache: cache.clone(),
        }
    }
}

impl Drop for InstallingGuard {
    fn drop(&mut self) {
        if let Ok(mut installing) = INSTALLING.lock() {
            installing.retain(|id| id != &self.tool_id);
        }
        self.cache.notify_changed();
    }
}

/// Identifiant, version et architecture d'un outil.
type ToolKey = (String, String, String);
type ToolLocks = BTreeMap<ToolKey, Arc<tokio::sync::Mutex<()>>>;

/// Un verrou par outil : deux téléchargements du même outil seraient inutiles, mais un serveur lent ne
/// retient pas les autres outils.
static INSTALL_LOCKS: std::sync::Mutex<ToolLocks> = std::sync::Mutex::new(BTreeMap::new());

/// Chaque archive est gardée en mémoire : leur nombre est borné.
const MAX_CONCURRENT_DOWNLOADS: usize = 3;
static DOWNLOAD_SLOTS: tokio::sync::Semaphore =
    tokio::sync::Semaphore::const_new(MAX_CONCURRENT_DOWNLOADS);

fn install_lock(tool_id: &str, version: &str, arch: &str) -> Arc<tokio::sync::Mutex<()>> {
    INSTALL_LOCKS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .entry((tool_id.to_owned(), version.to_owned(), arch.to_owned()))
        .or_default()
        .clone()
}

pub fn parse_distribution(raw: &str) -> AppResult<ToolDistribution> {
    serde_json::from_str(raw).map_err(|error| AppError::InvalidManifest(error.to_string()))
}

/// Artefact prévu pour l'architecture de cet hôte.
pub fn artifact_for_host(distribution: &ToolDistribution) -> Option<&ToolArtifact> {
    distribution
        .artifacts
        .iter()
        .find(|artifact| artifact.arch == host_arch())
}

/// Contrat de distribution d'un outil, lu dans le premier dossier de catalogue qui le porte.
pub fn read_distribution(roots: &[PathBuf], tool_id: &str) -> AppResult<serde_json::Value> {
    for root in roots {
        for path in distribution_candidates(root, tool_id) {
            if path.is_file() {
                let content = std::fs::read_to_string(&path)?;
                return serde_json::from_str(&content)
                    .map_err(|error| AppError::InvalidManifest(error.to_string()));
            }
        }
    }
    Err(AppError::NotFound)
}

/// Emplacements possibles du contrat : à côté du manifeste indexé (dossier libre, dépôts
/// tiers compris), puis `apps/<identifiant>` pour un dossier sans index.
fn distribution_candidates(root: &Path, tool_id: &str) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(catalog) = super::load_catalog(root)
        && let Some(entry) = catalog
            .entries
            .iter()
            .find(|entry| entry.entry_type == CatalogItemType::App && entry.id == tool_id)
        && ToolCacheManager::is_safe_relative_path(Path::new(&entry.manifest_path))
        && let Some(folder) = Path::new(&entry.manifest_path).parent()
    {
        candidates.push(root.join(folder).join(DISTRIBUTION_FILE_NAME));
    }
    if super::is_valid_repository_id(tool_id) {
        candidates.push(distribution_path(root, tool_id));
    }
    candidates
}

/// Télécharge, vérifie (SHA-256) et installe l'artefact de cet hôte. Sans effet si l'outil est déjà prêt.
pub async fn install_from_distribution(
    cache: &ToolCacheManager,
    distribution: &ToolDistribution,
    policy: &SyncPolicy,
) -> AppResult<PathBuf> {
    if distribution.release_status.as_deref() == Some("contract-only") {
        return Err(AppError::ToolUnavailable(format!(
            "{} : contrat en attente (contract-only)",
            distribution.tool_id
        )));
    }
    if policy.require_immutable_releases
        && distribution.release_status.as_deref() != Some("released")
    {
        return Err(AppError::ToolUnavailable(format!(
            "{} : releaseStatus 'released' requis en mode immuable",
            distribution.tool_id
        )));
    }
    let artifact = artifact_for_host(distribution).ok_or_else(|| {
        AppError::ToolUnavailable(format!(
            "{} : aucun artefact pour {}",
            distribution.tool_id,
            host_arch()
        ))
    })?;
    let url = Url::parse(&artifact.archive_url)
        .map_err(|error| AppError::InvalidManifest(error.to_string()))?;
    if policy.require_immutable_releases && is_mutable_branch_url(&url) {
        return Err(AppError::InvalidManifest(format!(
            "{} : archiveUrl pointe vers une branche mouvante ({url})",
            distribution.tool_id
        )));
    }
    let ready_binary = || {
        let (id, version) = (&distribution.tool_id, &distribution.tool_version);
        cache.is_ready(id, version, &artifact.arch).then(|| {
            ToolCacheManager::resolve_bin_candidate(
                &cache.tool_dir(id, version, &artifact.arch).join("bin"),
                &artifact.binary_relative_path,
            )
        })
    };
    if let Some(binary) = ready_binary() {
        return Ok(binary);
    }
    let _installing = InstallingGuard::start(&distribution.tool_id, cache);
    let lock = install_lock(
        &distribution.tool_id,
        &distribution.tool_version,
        &artifact.arch,
    );
    let _guard = lock.lock().await;
    // Une autre installation a pu l'achever pendant l'attente.
    if let Some(binary) = ready_binary() {
        return Ok(binary);
    }
    let _slot = DOWNLOAD_SLOTS
        .acquire()
        .await
        .map_err(|_| AppError::Internal)?;
    let archive = fetch_archive(&url, policy)
        .await
        .map_err(|error| AppError::RepositoryUnavailable(error.to_string()))?;
    let cache = cache.clone();
    let tool_id = distribution.tool_id.clone();
    let tool_version = distribution.tool_version.clone();
    let artifact = artifact.clone();
    let allowlist = distribution
        .install_policy
        .as_ref()
        .map(|policy| policy.executable_allowlist.clone())
        .unwrap_or_default();
    tokio::task::spawn_blocking(move || {
        cache.extract_and_install_with_allowlist(
            &tool_id,
            &tool_version,
            &artifact,
            &archive,
            &allowlist,
        )
    })
    .await
    .map_err(|_| AppError::Internal)?
}

/// Installe un outil du catalogue à la version attendue, d'après le contrat des dépôts synchronisés.
pub async fn ensure_tool_installed(
    roots: &[PathBuf],
    cache: &ToolCacheManager,
    tool_id: &str,
    version: &str,
    policy: &SyncPolicy,
) -> AppResult<PathBuf> {
    ensure_tool_installed_with_optional_pool(None, roots, cache, tool_id, version, policy).await
}

/// Variante vérifiant en plus l'immuabilité TOFU de l'archive (`plugin_release_pins`).
pub async fn ensure_tool_installed_verified(
    pool: &SqlitePool,
    roots: &[PathBuf],
    cache: &ToolCacheManager,
    tool_id: &str,
    version: &str,
    policy: &SyncPolicy,
) -> AppResult<PathBuf> {
    ensure_tool_installed_with_optional_pool(Some(pool), roots, cache, tool_id, version, policy)
        .await
}

async fn ensure_tool_installed_with_optional_pool(
    pool: Option<&SqlitePool>,
    roots: &[PathBuf],
    cache: &ToolCacheManager,
    tool_id: &str,
    version: &str,
    policy: &SyncPolicy,
) -> AppResult<PathBuf> {
    let distribution = parse_distribution(&read_distribution(roots, tool_id)?.to_string())?;
    if distribution.tool_id != tool_id || distribution.tool_version != version {
        return Err(AppError::ToolUnavailable(format!(
            "{tool_id}@{version} : le contrat déclare {}@{}",
            distribution.tool_id, distribution.tool_version
        )));
    }
    if let (Some(pool), Some(artifact)) = (pool, artifact_for_host(&distribution)) {
        let scope = format!("tool:{tool_id}:{}", artifact.arch);
        if let Some(expected) = crate::repository::verify_or_pin_release(
            pool,
            &scope,
            version,
            &artifact.archive_sha256,
            &artifact.archive_url,
        )
        .await?
        {
            return Err(AppError::InvalidManifest(format!(
                "{tool_id}@{version} ({}) : archiveSha256 muté (attendu {expected}, reçu {})",
                artifact.arch, artifact.archive_sha256
            )));
        }
    }
    install_from_distribution(cache, &distribution, policy).await
}

/// Installe uniquement la liste explicite d'outils requis par le graphe ou la sonde du job.
pub async fn ensure_tools_for_requirements(
    roots: &[PathBuf],
    cache: &ToolCacheManager,
    requirements: &[crate::domain::ToolRequirement],
    policy: &SyncPolicy,
) {
    for req in requirements {
        if cache.is_ready(&req.tool_id, &req.version, host_arch()) {
            continue;
        }
        if let Err(error) =
            ensure_tool_installed(roots, cache, &req.tool_id, &req.version, policy).await
        {
            tracing::warn!(
                tool = %req.tool_id,
                version = %req.version,
                error = %error,
                "installation de l'outil impossible"
            );
        }
    }
}

/// Chemin du contrat de distribution d'une application dans un dossier de catalogue.
fn distribution_path(root: &Path, name: &str) -> PathBuf {
    root.join("apps").join(name).join(DISTRIBUTION_FILE_NAME)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::opaque_token::sha256_hex;

    fn archive_with(binary_name: &str, content: &[u8]) -> anyhow::Result<Vec<u8>> {
        let mut builder = tar::Builder::new(Vec::new());
        let mut header = tar::Header::new_gnu();
        header.set_size(content.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        builder.append_data(&mut header, binary_name, content)?;
        let tar_bytes = builder.into_inner()?;
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        std::io::Write::write_all(&mut encoder, &tar_bytes)?;
        Ok(encoder.finish()?)
    }

    async fn serve_archive(bytes: Vec<u8>) -> anyhow::Result<std::net::SocketAddr> {
        use axum::{Router, routing::get};
        let router = Router::new().route(
            "/outil.tar.gz",
            get(move || {
                let body = bytes.clone();
                async move { body }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let address = listener.local_addr()?;
        tokio::spawn(async move {
            let _ = axum::serve(listener, router).await;
        });
        Ok(address)
    }

    fn distribution(address: std::net::SocketAddr, archive_sha256: String) -> ToolDistribution {
        ToolDistribution {
            schema_version: Some("1.0.0".into()),
            tool_id: "demo".into(),
            tool_version: "1.0.0".into(),
            release_status: None,
            distribution_mode: "autonomous-binary".into(),
            install_policy: None,
            artifacts: vec![ToolArtifact {
                arch: host_arch().into(),
                archive_url: format!("http://{address}/outil.tar.gz"),
                archive_format: "tar.gz".into(),
                archive_sha256,
                binary_relative_path: "demo".into(),
                binary_sha256: None,
                binaries: std::collections::HashMap::new(),
                permissions: None,
            }],
            released_at: None,
        }
    }

    #[tokio::test]
    async fn test_install_downloads_verifies_and_marks_the_tool_ready() -> anyhow::Result<()> {
        let archive = archive_with("demo", b"#!/bin/sh\nexit 0\n")?;
        let address = serve_archive(archive.clone()).await?;
        let dir = tempfile::tempdir()?;
        let cache = ToolCacheManager::new(dir.path());
        let policy = SyncPolicy {
            allow_private_hosts: true,
            ..SyncPolicy::default()
        };
        let contract = distribution(address, sha256_hex(&archive));

        let binary = install_from_distribution(&cache, &contract, &policy).await?;
        assert!(binary.is_file());
        assert!(cache.is_ready("demo", "1.0.0", host_arch()));

        let again = install_from_distribution(&cache, &contract, &policy).await?;
        assert_eq!(binary, again);
        Ok(())
    }

    #[tokio::test]
    async fn an_installation_signals_its_end_even_when_it_fails() -> anyhow::Result<()> {
        use crate::live::{CHANGE_WINDOW, ChangeNotifier, EventHistory, LiveEvent};
        use forgearr_shared::live::ChangeTopic;
        let archive = archive_with("demo", b"#!/bin/sh\nexit 0\n")?;
        let address = serve_archive(archive.clone()).await?;
        let dir = tempfile::tempdir()?;
        let history = EventHistory::new();
        let cache =
            ToolCacheManager::new(dir.path()).with_changes(ChangeNotifier::new(history.clone()));
        let policy = SyncPolicy {
            allow_private_hosts: true,
            ..SyncPolicy::default()
        };
        for digest in ["00".repeat(32), sha256_hex(&archive)] {
            let mut rx = history.subscribe_from(None).await.rx;
            let contract = distribution(address, digest);
            let _ = install_from_distribution(&cache, &contract, &policy).await;
            tokio::time::sleep(CHANGE_WINDOW * 3).await;
            assert!(matches!(
                rx.try_recv(),
                Ok((_, LiveEvent::Changed(ChangeTopic::Tools)))
            ));
        }
        assert!(cache.is_ready("demo", "1.0.0", host_arch()));
        // Déjà prêt : rien n'a changé.
        let mut rx = history.subscribe_from(None).await.rx;
        let contract = distribution(address, sha256_hex(&archive));
        install_from_distribution(&cache, &contract, &policy).await?;
        tokio::time::sleep(CHANGE_WINDOW * 2).await;
        assert!(rx.try_recv().is_err());
        Ok(())
    }

    #[tokio::test]
    async fn test_install_refuses_an_archive_with_a_wrong_digest() -> anyhow::Result<()> {
        let archive = archive_with("demo", b"contenu")?;
        let address = serve_archive(archive).await?;
        let dir = tempfile::tempdir()?;
        let cache = ToolCacheManager::new(dir.path());
        let policy = SyncPolicy {
            allow_private_hosts: true,
            ..SyncPolicy::default()
        };
        let contract = distribution(address, "0".repeat(64));

        let result = install_from_distribution(&cache, &contract, &policy).await;
        assert!(matches!(result, Err(AppError::ChecksumMismatch { .. })));
        assert!(!cache.is_ready("demo", "1.0.0", host_arch()));
        Ok(())
    }

    #[tokio::test]
    async fn test_install_refuses_plain_http_to_a_private_host_by_default() {
        let cache = ToolCacheManager::new(std::env::temp_dir().join("flux-install-refus"));
        let contract = distribution(
            "127.0.0.1:9"
                .parse()
                .unwrap_or_else(|_| std::net::SocketAddr::from(([127, 0, 0, 1], 9))),
            "0".repeat(64),
        );
        let result = install_from_distribution(&cache, &contract, &SyncPolicy::default()).await;
        assert!(matches!(result, Err(AppError::RepositoryUnavailable(_))));
    }

    #[test]
    fn test_distribution_without_artifact_for_this_host_is_unavailable() {
        let mut contract = distribution(
            std::net::SocketAddr::from(([127, 0, 0, 1], 9)),
            "0".repeat(64),
        );
        contract.artifacts[0].arch = "mips".into();
        assert!(artifact_for_host(&contract).is_none());
    }

    #[test]
    fn read_distribution_follows_the_indexed_manifest_folder() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let root = dir.path().to_path_buf();
        std::fs::create_dir_all(root.join("apps/handbrake"))?;
        std::fs::write(
            root.join("index.json"),
            r#"{"schemaVersion":"1.0.0","entries":[{"id":"com.alice.handbrake","type":"app",
                "version":"1.0.0","name":"HandBrake","manifestPath":"apps/handbrake/manifest.json"}]}"#,
        )?;
        std::fs::write(
            root.join("apps/handbrake").join(DISTRIBUTION_FILE_NAME),
            r#"{"toolId":"com.alice.handbrake","toolVersion":"1.0.0","artifacts":[]}"#,
        )?;
        let roots = vec![root];
        assert_eq!(
            read_distribution(&roots, "com.alice.handbrake")?["toolId"],
            "com.alice.handbrake"
        );
        Ok(())
    }

    #[test]
    fn test_read_distribution_refuses_unsafe_names_and_reads_synced_contracts() -> anyhow::Result<()>
    {
        let dir = tempfile::tempdir()?;
        let root = dir.path().to_path_buf();
        std::fs::create_dir_all(root.join("apps/demo"))?;
        std::fs::write(
            distribution_path(&root, "demo"),
            r#"{"toolId":"demo","toolVersion":"1.0.0","artifacts":[]}"#,
        )?;
        let roots = vec![root];
        assert_eq!(read_distribution(&roots, "demo")?["toolId"], "demo");
        assert!(matches!(
            read_distribution(&roots, "absent"),
            Err(AppError::NotFound)
        ));
        assert!(matches!(
            read_distribution(&roots, "../../x"),
            Err(AppError::NotFound)
        ));

        let sibling = Path::new(env!("CARGO_MANIFEST_DIR")).join("../flux-de-fichiers-plugins");
        if sibling.join("apps/ffmpeg/distribution.v1.json").is_file() {
            let ffmpeg = read_distribution(std::slice::from_ref(&sibling), "ffmpeg")?;
            assert_eq!(ffmpeg["toolId"], "ffmpeg");
        }
        Ok(())
    }

    #[test]
    fn test_parse_distribution_reads_the_repository_format() -> AppResult<()> {
        let raw = r#"{"schemaVersion":"1.0.0","toolId":"mediainfo","toolVersion":"1.0.0",
            "distributionMode":"autonomous-binary","artifacts":[{"arch":"x86_64","os":"linux",
            "archiveUrl":"https://example.org/a.tar.gz","archiveFormat":"tar.gz",
            "archiveSha256":"abc","binaryRelativePath":"bin/mediainfo"}]}"#;
        let parsed = parse_distribution(raw)?;
        assert_eq!(parsed.tool_id, "mediainfo");
        assert_eq!(parsed.artifacts[0].binary_relative_path, "bin/mediainfo");
        Ok(())
    }

    #[test]
    fn test_installing_guard_marks_the_tool_until_it_is_dropped() {
        let cache = ToolCacheManager::new(std::env::temp_dir().join("flux-install-garde"));
        assert!(!is_installing("garde"));
        {
            let _first = InstallingGuard::start("garde", &cache);
            let _again = InstallingGuard::start("garde", &cache);
            assert!(is_installing("garde"));
        }
        assert!(!is_installing("garde"));
    }

    #[tokio::test]
    async fn test_a_running_install_is_reported_as_installing_then_ready() -> anyhow::Result<()> {
        let archive = archive_with("demo", b"#!/bin/sh\nexit 0\n")?;
        let address = serve_archive(archive.clone()).await?;
        let dir = tempfile::tempdir()?;
        let cache = ToolCacheManager::new(dir.path());
        let policy = SyncPolicy {
            allow_private_hosts: true,
            ..SyncPolicy::default()
        };
        let mut contract = distribution(address, sha256_hex(&archive));
        contract.tool_id = "suivi".into();

        let watcher = tokio::spawn(async {
            for _ in 0..200 {
                if is_installing("suivi") {
                    return true;
                }
                tokio::time::sleep(std::time::Duration::from_millis(1)).await;
            }
            false
        });
        install_from_distribution(&cache, &contract, &policy).await?;
        assert!(watcher.await?);
        assert!(!is_installing("suivi"));
        Ok(())
    }

    #[tokio::test]
    async fn a_silent_server_does_not_hold_back_another_tool() -> anyhow::Result<()> {
        use axum::{Router, routing::get};
        use std::time::Duration;
        const TEST_LIMIT: Duration = Duration::from_secs(20);

        let reached = Arc::new(tokio::sync::Notify::new());
        let server_reached = reached.clone();
        let router = Router::new().route(
            "/outil.tar.gz",
            get(move || async move {
                server_reached.notify_one();
                std::future::pending::<&'static str>().await
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let silent = listener.local_addr()?;
        tokio::spawn(async move {
            let _ = axum::serve(listener, router).await;
        });
        let archive = archive_with("demo", b"#!/bin/sh\nexit 0\n")?;
        let address = serve_archive(archive.clone()).await?;
        let dir = tempfile::tempdir()?;
        let cache = ToolCacheManager::new(dir.path());
        let policy = SyncPolicy {
            allow_private_hosts: true,
            ..SyncPolicy::default()
        };
        let mut stalled = distribution(silent, "0".repeat(64));
        stalled.tool_id = "muet".into();
        let stalled_cache = cache.clone();
        let waiting = tokio::spawn(async move {
            install_from_distribution(&stalled_cache, &stalled, &policy)
                .await
                .is_ok()
        });
        tokio::time::timeout(TEST_LIMIT, reached.notified()).await?;

        let mut other = distribution(address, sha256_hex(&archive));
        other.tool_id = "autre".into();
        tokio::time::timeout(
            TEST_LIMIT,
            install_from_distribution(&cache, &other, &policy),
        )
        .await??;
        assert!(cache.is_ready("autre", "1.0.0", host_arch()));
        assert!(is_installing("muet"));
        waiting.abort();
        Ok(())
    }

    #[tokio::test]
    async fn test_contract_only_distribution_is_rejected_cleanly() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let cache = ToolCacheManager::new(dir.path());
        let mut contract = distribution("127.0.0.1:9".parse()?, "0".into());
        contract.release_status = Some("contract-only".into());
        let result = install_from_distribution(&cache, &contract, &SyncPolicy::default()).await;
        assert!(matches!(result, Err(AppError::ToolUnavailable(_))));
        Ok(())
    }

    #[tokio::test]
    async fn test_ensure_tools_for_requirements_without_repo_is_a_harmless_no_op()
    -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let cache = ToolCacheManager::new(dir.path());
        let wanted = [crate::domain::ToolRequirement {
            tool_id: "example".into(),
            version: "1.0.0".into(),
        }];
        ensure_tools_for_requirements(&[], &cache, &wanted, &SyncPolicy::default()).await;
        assert!(!cache.is_ready("example", "1.0.0", host_arch()));
        Ok(())
    }

    #[tokio::test]
    async fn test_tool_pin_refuses_mutated_archive_sha256_for_same_version() -> anyhow::Result<()> {
        use sqlx::sqlite::SqlitePoolOptions;

        let pool = SqlitePoolOptions::new().connect("sqlite::memory:").await?;
        sqlx::migrate!().run(&pool).await?;

        let archive = archive_with("demo", b"#!/bin/sh\nexit 0\n")?;
        let address = serve_archive(archive.clone()).await?;
        let dir = tempfile::tempdir()?;
        let root = dir.path().join("repo");
        std::fs::create_dir_all(root.join("apps/demo"))?;
        let sha = sha256_hex(&archive);
        std::fs::write(
            distribution_path(&root, "demo"),
            serde_json::json!({
                "schemaVersion": "1.0.0",
                "toolId": "demo",
                "toolVersion": "1.0.0",
                "releaseStatus": "released",
                "distributionMode": "autonomous-binary",
                "artifacts": [{
                    "arch": host_arch(),
                    "os": "linux",
                    "archiveUrl": format!("http://{address}/outil.tar.gz"),
                    "archiveFormat": "tar.gz",
                    "archiveSha256": sha,
                    "binaryRelativePath": "demo"
                }]
            })
            .to_string(),
        )?;

        let cache = ToolCacheManager::new(dir.path().join("cache"));
        let policy = SyncPolicy {
            allow_private_hosts: true,
            require_immutable_releases: true,
        };
        ensure_tool_installed_verified(
            &pool,
            std::slice::from_ref(&root),
            &cache,
            "demo",
            "1.0.0",
            &policy,
        )
        .await?;

        // Une archive différente sous la même version 1.0.0 est refusée par l'épinglage TOFU.
        std::fs::write(
            distribution_path(&root, "demo"),
            serde_json::json!({
                "schemaVersion": "1.0.0",
                "toolId": "demo",
                "toolVersion": "1.0.0",
                "releaseStatus": "released",
                "distributionMode": "autonomous-binary",
                "artifacts": [{
                    "arch": host_arch(),
                    "os": "linux",
                    "archiveUrl": format!("http://{address}/outil.tar.gz"),
                    "archiveFormat": "tar.gz",
                    "archiveSha256": "a".repeat(64),
                    "binaryRelativePath": "demo"
                }]
            })
            .to_string(),
        )?;
        let mutated = ensure_tool_installed_verified(
            &pool,
            std::slice::from_ref(&root),
            &cache,
            "demo",
            "1.0.0",
            &policy,
        )
        .await;
        assert!(matches!(mutated, Err(AppError::InvalidManifest(_))));
        Ok(())
    }
}
