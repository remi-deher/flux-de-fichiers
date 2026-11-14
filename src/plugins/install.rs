//! Installation d'un outil distribué dans le cache local, d'après son contrat de distribution.

use super::{SyncPolicy, sync::fetch_archive};
use crate::{
    error::{AppError, AppResult},
    tools_cache::{ToolArtifact, ToolCacheManager, ToolDistribution, host_arch},
};
use reqwest::Url;
use std::path::{Path, PathBuf};

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
struct InstallingGuard(String);

impl InstallingGuard {
    fn start(tool_id: &str) -> Self {
        if let Ok(mut installing) = INSTALLING.lock()
            && !installing.iter().any(|id| id == tool_id)
        {
            installing.push(tool_id.to_owned());
        }
        Self(tool_id.to_owned())
    }
}

impl Drop for InstallingGuard {
    fn drop(&mut self) {
        if let Ok(mut installing) = INSTALLING.lock() {
            installing.retain(|id| id != &self.0);
        }
    }
}

/// Une seule installation à la fois : deux téléchargements du même outil seraient inutiles.
static INSTALL_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

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
    let candidates = distribution_lookup_names(tool_id);
    if candidates.is_empty() {
        return Err(AppError::NotFound);
    }
    for root in roots {
        for name in &candidates {
            for path in [
                distribution_path(root, name),
                tool_distribution_path(root, name),
            ] {
                if path.is_file() {
                    let content = std::fs::read_to_string(&path)?;
                    return serde_json::from_str(&content)
                        .map_err(|error| AppError::InvalidManifest(error.to_string()));
                }
            }
        }
    }
    Err(AppError::NotFound)
}

/// Noms de dossier possibles pour un outil (`app.ffmpeg` → `ffmpeg`, `tool.ffmpeg` → `ffmpeg`).
fn distribution_lookup_names(tool_id: &str) -> Vec<String> {
    let mut names = Vec::new();
    for stripped in [
        tool_id.trim_start_matches("app."),
        tool_id.trim_start_matches("tool."),
        tool_id,
    ] {
        if super::is_valid_repository_id(stripped) && !names.iter().any(|name| name == stripped) {
            names.push(stripped.to_owned());
        }
    }
    names
}

/// Télécharge, vérifie (SHA-256) et installe l'artefact de cet hôte. Sans effet si l'outil est déjà prêt.
pub async fn install_from_distribution(
    cache: &ToolCacheManager,
    distribution: &ToolDistribution,
    policy: &SyncPolicy,
) -> AppResult<PathBuf> {
    let artifact = artifact_for_host(distribution).ok_or_else(|| {
        AppError::ToolUnavailable(format!(
            "{} : aucun artefact pour {}",
            distribution.tool_id,
            host_arch()
        ))
    })?;
    let _installing = InstallingGuard::start(&distribution.tool_id);
    let _guard = INSTALL_LOCK.lock().await;
    let binary = cache
        .tool_dir(
            &distribution.tool_id,
            &distribution.tool_version,
            &artifact.arch,
        )
        .join("bin")
        .join(&artifact.binary_relative_path);
    if cache.is_ready(
        &distribution.tool_id,
        &distribution.tool_version,
        &artifact.arch,
    ) {
        return Ok(binary);
    }
    let url = Url::parse(&artifact.archive_url)
        .map_err(|error| AppError::InvalidManifest(error.to_string()))?;
    let archive = fetch_archive(&url, policy)
        .await
        .map_err(|error| AppError::RepositoryUnavailable(error.to_string()))?;
    let cache = cache.clone();
    let tool_id = distribution.tool_id.clone();
    let tool_version = distribution.tool_version.clone();
    let artifact = artifact.clone();
    tokio::task::spawn_blocking(move || {
        cache.extract_and_install(&tool_id, &tool_version, &artifact, &archive)
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
    let distribution = parse_distribution(&read_distribution(roots, tool_id)?.to_string())?;
    if distribution.tool_id != tool_id || distribution.tool_version != version {
        return Err(AppError::ToolUnavailable(format!(
            "{tool_id}@{version} : le contrat déclare {}@{}",
            distribution.tool_id, distribution.tool_version
        )));
    }
    install_from_distribution(cache, &distribution, policy).await
}

/// Installe les outils d'un job : la sonde du catalogue et l'outil de remuxage. Un échec est journalisé :
/// le job échouera de lui-même s'il manque réellement un outil.
pub async fn ensure_job_tools(
    roots: &[PathBuf],
    cache: &ToolCacheManager,
    probe: Option<&super::ProbeContract>,
    policy: &SyncPolicy,
) {
    let mut wanted = vec![
        (
            super::REMUX_TOOL_ID.to_owned(),
            super::REMUX_TOOL_VERSION.to_owned(),
        ),
        (
            super::FFMPEG_TOOL_ID.to_owned(),
            super::FFMPEG_TOOL_VERSION.to_owned(),
        ),
    ];
    if let Some(contract) = probe {
        wanted.push((contract.tool_id.clone(), contract.version.clone()));
    }
    for (tool_id, version) in wanted {
        if cache.is_ready(&tool_id, &version, host_arch()) {
            continue;
        }
        if let Err(error) = ensure_tool_installed(roots, cache, &tool_id, &version, policy).await {
            tracing::warn!(tool = %tool_id, error = %error, "installation de l'outil impossible");
        }
    }
}

/// Chemin du contrat de distribution d'une application dans un dossier de catalogue.
fn distribution_path(root: &Path, name: &str) -> PathBuf {
    root.join("apps").join(name).join(DISTRIBUTION_FILE_NAME)
}

/// Chemin du contrat sous `tools/<name>/` (catalogue plugins ADR 0004).
fn tool_distribution_path(root: &Path, name: &str) -> PathBuf {
    root.join("tools").join(name).join(DISTRIBUTION_FILE_NAME)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

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

    fn hex(bytes: &[u8]) -> String {
        Sha256::digest(bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
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
            tool_id: "app.demo".into(),
            tool_version: "1.0.0".into(),
            distribution_mode: "autonomous-binary".into(),
            artifacts: vec![ToolArtifact {
                arch: host_arch().into(),
                archive_url: format!("http://{address}/outil.tar.gz"),
                archive_format: "tar.gz".into(),
                archive_sha256,
                binary_relative_path: "demo".into(),
                binary_sha256: None,
                permissions: None,
            }],
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
        };
        let contract = distribution(address, hex(&archive));

        let binary = install_from_distribution(&cache, &contract, &policy).await?;
        assert!(binary.is_file());
        assert!(cache.is_ready("app.demo", "1.0.0", host_arch()));

        let again = install_from_distribution(&cache, &contract, &policy).await?;
        assert_eq!(binary, again);
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
        };
        let contract = distribution(address, "0".repeat(64));

        let result = install_from_distribution(&cache, &contract, &policy).await;
        assert!(matches!(result, Err(AppError::ChecksumMismatch { .. })));
        assert!(!cache.is_ready("app.demo", "1.0.0", host_arch()));
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
    fn test_read_distribution_refuses_unsafe_names_and_reads_synced_contracts() -> anyhow::Result<()>
    {
        let dir = tempfile::tempdir()?;
        let root = dir.path().to_path_buf();
        std::fs::create_dir_all(root.join("apps/demo"))?;
        std::fs::write(
            distribution_path(&root, "demo"),
            r#"{"toolId":"app.demo","toolVersion":"1.0.0","artifacts":[]}"#,
        )?;
        let roots = vec![root];
        assert_eq!(read_distribution(&roots, "app.demo")?["toolId"], "app.demo");
        assert!(matches!(
            read_distribution(&roots, "app.absent"),
            Err(AppError::NotFound)
        ));
        assert!(matches!(
            read_distribution(&roots, "app../../x"),
            Err(AppError::NotFound)
        ));

        let sibling = Path::new(env!("CARGO_MANIFEST_DIR")).join("../flux-de-fichiers-plugins");
        if sibling.join("tools/ffmpeg/distribution.v1.json").is_file() {
            let ffmpeg = read_distribution(std::slice::from_ref(&sibling), "tool.ffmpeg")?;
            assert_eq!(ffmpeg["toolId"], "tool.ffmpeg");
        }
        Ok(())
    }

    #[test]
    fn test_parse_distribution_reads_the_repository_format() -> AppResult<()> {
        let raw = r#"{"schemaVersion":"1.0.0","toolId":"app.mediainfo","toolVersion":"1.0.0",
            "distributionMode":"autonomous-binary","artifacts":[{"arch":"x86_64","os":"linux",
            "archiveUrl":"https://example.org/a.tar.gz","archiveFormat":"tar.gz",
            "archiveSha256":"abc","binaryRelativePath":"bin/mediainfo"}]}"#;
        let parsed = parse_distribution(raw)?;
        assert_eq!(parsed.tool_id, "app.mediainfo");
        assert_eq!(parsed.artifacts[0].binary_relative_path, "bin/mediainfo");
        Ok(())
    }

    #[test]
    fn test_installing_guard_marks_the_tool_until_it_is_dropped() {
        assert!(!is_installing("app.garde"));
        {
            let _first = InstallingGuard::start("app.garde");
            let _again = InstallingGuard::start("app.garde");
            assert!(is_installing("app.garde"));
        }
        assert!(!is_installing("app.garde"));
    }

    #[tokio::test]
    async fn test_a_running_install_is_reported_as_installing_then_ready() -> anyhow::Result<()> {
        let archive = archive_with("demo", b"#!/bin/sh\nexit 0\n")?;
        let address = serve_archive(archive.clone()).await?;
        let dir = tempfile::tempdir()?;
        let cache = ToolCacheManager::new(dir.path());
        let policy = SyncPolicy {
            allow_private_hosts: true,
        };
        let mut contract = distribution(address, hex(&archive));
        contract.tool_id = "app.suivi".into();

        let watcher = tokio::spawn(async {
            for _ in 0..200 {
                if is_installing("app.suivi") {
                    return true;
                }
                tokio::time::sleep(std::time::Duration::from_millis(1)).await;
            }
            false
        });
        install_from_distribution(&cache, &contract, &policy).await?;
        assert!(watcher.await?);
        assert!(!is_installing("app.suivi"));
        Ok(())
    }

    #[tokio::test]
    async fn test_ensure_job_tools_without_any_repository_is_a_harmless_no_op() -> anyhow::Result<()>
    {
        let dir = tempfile::tempdir()?;
        let cache = ToolCacheManager::new(dir.path());
        ensure_job_tools(&[], &cache, None, &SyncPolicy::default()).await;
        assert!(!cache.is_ready(
            super::super::REMUX_TOOL_ID,
            super::super::REMUX_TOOL_VERSION,
            host_arch()
        ));
        Ok(())
    }
}
