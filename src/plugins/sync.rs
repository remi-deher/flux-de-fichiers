//! Synchronisation d'un dépôt de plugins vers son dossier local : bornée, sans redirection,
//! refusant les adresses privées, et atomique (l'ancienne copie reste en place en cas d'échec).

use super::{CatalogItemType, PluginRepository, is_valid_repository_id, parse_index_tolerant};
use crate::tools_cache::ToolCacheManager;
use futures_util::{StreamExt, stream};
use reqwest::{Client, StatusCode, Url, redirect::Policy};
use std::{
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    path::{Path, PathBuf},
    time::Duration,
};

const INDEX_RELATIVE_PATH: &str = "catalogue/index.json";
const DISTRIBUTION_FILE_NAME: &str = "distribution.v1.json";
/// Variable d'environnement qui autorise les adresses privées (réseaux de laboratoire, tests).
pub const ALLOW_PRIVATE_HOSTS_ENV: &str = "PLUGIN_ALLOW_PRIVATE_HOSTS";
const SYNC_TIMEOUT: Duration = Duration::from_secs(15);
const MAX_FILE_BYTES: usize = 1024 * 1024;
const MAX_FILES: usize = 500;
const FETCH_CONCURRENCY: usize = 8;
const UNIQUE_LOCAL_PREFIX_MASK: u16 = 0xfe00;
const UNIQUE_LOCAL_PREFIX: u16 = 0xfc00;

/// Règles réseau d'une synchronisation.
#[derive(Clone, Copy, Debug, Default)]
pub struct SyncPolicy {
    pub allow_private_hosts: bool,
}

impl SyncPolicy {
    pub fn from_env() -> Self {
        Self {
            allow_private_hosts: std::env::var(ALLOW_PRIVATE_HOSTS_ENV).is_ok_and(|v| v == "1"),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SyncError {
    #[error("identifiant de dépôt invalide")]
    InvalidRepositoryId,
    #[error("URL d'index invalide : {0}")]
    InvalidUrl(String),
    #[error("hôte refusé : {0}")]
    ForbiddenHost(String),
    #[error("réponse HTTP {status} pour {url}")]
    Http { status: u16, url: String },
    #[error("fichier trop volumineux : {0}")]
    TooLarge(String),
    #[error("index invalide : {0}")]
    InvalidIndex(String),
    #[error("trop de fichiers déclarés par l'index")]
    TooManyFiles,
    #[error("réseau : {0}")]
    Network(String),
    #[error("stockage : {0}")]
    Storage(#[from] std::io::Error),
}

/// Bilan d'une synchronisation réussie.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncReport {
    pub entries: usize,
    pub files: usize,
    pub skipped: Vec<String>,
}

/// Adresse qui ne doit jamais être jointe depuis un dépôt distant (réseau interne, métadonnées cloud).
fn is_forbidden_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_forbidden_v4(v4),
        IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
            Some(mapped) => is_forbidden_v4(mapped),
            None => is_forbidden_v6(v6),
        },
    }
}

fn is_forbidden_v4(ip: Ipv4Addr) -> bool {
    ip.is_loopback()
        || ip.is_private()
        || ip.is_link_local()
        || ip.is_unspecified()
        || ip.is_broadcast()
        || ip.is_multicast()
}

fn is_forbidden_v6(ip: Ipv6Addr) -> bool {
    let first_segment = ip.segments()[0];
    ip.is_loopback()
        || ip.is_unspecified()
        || ip.is_multicast()
        || ip.is_unicast_link_local()
        || first_segment & UNIQUE_LOCAL_PREFIX_MASK == UNIQUE_LOCAL_PREFIX
}

/// Base du dépôt : l'URL de l'index privée de `catalogue/index.json`.
fn base_url(index_url: &Url) -> Result<Url, SyncError> {
    let suffix = format!("/{INDEX_RELATIVE_PATH}");
    let path = index_url.path();
    let base_path = path
        .strip_suffix(&suffix)
        .ok_or_else(|| SyncError::InvalidUrl(format!("l'URL doit finir par {suffix}")))?;
    let mut base = index_url.clone();
    base.set_query(None);
    base.set_fragment(None);
    base.set_path(&format!("{base_path}/"));
    Ok(base)
}

/// Valide l'URL d'index d'un dépôt à l'ajout : schéma http(s) et suffixe `/catalogue/index.json`.
pub fn validate_index_url(raw: &str) -> Result<(), SyncError> {
    let url = Url::parse(raw).map_err(|error| SyncError::InvalidUrl(error.to_string()))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(SyncError::InvalidUrl("schéma http ou https attendu".into()));
    }
    base_url(&url).map(|_| ())
}

/// Client dont la résolution DNS est figée sur des adresses déjà contrôlées.
async fn pinned_client(url: &Url, policy: &SyncPolicy) -> Result<Client, SyncError> {
    if !matches!(url.scheme(), "http" | "https") {
        return Err(SyncError::InvalidUrl("schéma http ou https attendu".into()));
    }
    let port = url
        .port_or_known_default()
        .ok_or_else(|| SyncError::InvalidUrl("port inconnu".into()))?;
    let (host_label, addresses): (String, Vec<SocketAddr>) = match url.domain() {
        Some(domain) => {
            let resolved = tokio::net::lookup_host((domain, port))
                .await
                .map_err(|error| SyncError::Network(error.to_string()))?
                .collect();
            (domain.to_owned(), resolved)
        }
        None => {
            let literal = url.host_str().unwrap_or_default().trim_matches(['[', ']']);
            let ip: IpAddr = literal
                .parse()
                .map_err(|_| SyncError::InvalidUrl("hôte invalide".into()))?;
            (literal.to_owned(), vec![SocketAddr::new(ip, port)])
        }
    };
    if addresses.is_empty() {
        return Err(SyncError::Network(format!("{host_label} : aucune adresse")));
    }
    if !policy.allow_private_hosts
        && addresses
            .iter()
            .any(|address| is_forbidden_ip(address.ip()))
    {
        return Err(SyncError::ForbiddenHost(host_label));
    }
    let mut builder = Client::builder()
        .redirect(Policy::none())
        .timeout(SYNC_TIMEOUT);
    if let Some(domain) = url.domain() {
        builder = builder.resolve(domain, addresses[0]);
    }
    builder
        .build()
        .map_err(|error| SyncError::Network(error.to_string()))
}

/// Taille maximale d'une archive d'outil.
const MAX_ARCHIVE_BYTES: usize = 256 * 1024 * 1024;
const MAX_REDIRECTS: usize = 3;

/// Télécharge un fichier binaire borné. Chaque redirection est revalidée (hôte public, https exigé).
pub(super) async fn fetch_archive(url: &Url, policy: &SyncPolicy) -> Result<Vec<u8>, SyncError> {
    let mut current = url.clone();
    for _ in 0..=MAX_REDIRECTS {
        if current.scheme() != "https" && !policy.allow_private_hosts {
            return Err(SyncError::InvalidUrl(
                "https requis pour une archive".into(),
            ));
        }
        let client = pinned_client(&current, policy).await?;
        let response = client
            .get(current.clone())
            .send()
            .await
            .map_err(|error| SyncError::Network(error.to_string()))?;
        if response.status().is_redirection() {
            let location = response
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|value| value.to_str().ok())
                .ok_or_else(|| SyncError::Network("redirection sans destination".into()))?;
            current = current
                .join(location)
                .map_err(|error| SyncError::InvalidUrl(error.to_string()))?;
            continue;
        }
        if !response.status().is_success() {
            return Err(SyncError::Http {
                status: response.status().as_u16(),
                url: current.to_string(),
            });
        }
        let mut body: Vec<u8> = Vec::new();
        let mut chunks = response.bytes_stream();
        while let Some(chunk) = chunks.next().await {
            let chunk = chunk.map_err(|error| SyncError::Network(error.to_string()))?;
            if body.len() + chunk.len() > MAX_ARCHIVE_BYTES {
                return Err(SyncError::TooLarge(current.to_string()));
            }
            body.extend_from_slice(&chunk);
        }
        return Ok(body);
    }
    Err(SyncError::Network("trop de redirections".into()))
}

/// Télécharge un fichier texte borné ; `None` si le serveur répond 404.
async fn fetch_text(client: &Client, url: &Url) -> Result<Option<String>, SyncError> {
    let response = client
        .get(url.clone())
        .send()
        .await
        .map_err(|error| SyncError::Network(error.to_string()))?;
    if response.status() == StatusCode::NOT_FOUND {
        return Ok(None);
    }
    if !response.status().is_success() {
        return Err(SyncError::Http {
            status: response.status().as_u16(),
            url: url.to_string(),
        });
    }
    let mut body: Vec<u8> = Vec::new();
    let mut chunks = response.bytes_stream();
    while let Some(chunk) = chunks.next().await {
        let chunk = chunk.map_err(|error| SyncError::Network(error.to_string()))?;
        if body.len() + chunk.len() > MAX_FILE_BYTES {
            return Err(SyncError::TooLarge(url.to_string()));
        }
        body.extend_from_slice(&chunk);
    }
    String::from_utf8(body)
        .map(Some)
        .map_err(|_| SyncError::InvalidIndex(format!("{url} n'est pas du texte UTF-8")))
}

/// Fichiers à copier en plus de l'index : le manifeste de chaque fiche, et le contrat de distribution d'une application.
fn wanted_files(entries: &[super::CatalogEntry]) -> Result<Vec<String>, SyncError> {
    let mut wanted: Vec<String> = Vec::new();
    for entry in entries {
        let manifest = Path::new(&entry.manifest_path);
        if !ToolCacheManager::is_safe_relative_path(manifest) || manifest.is_absolute() {
            continue;
        }
        wanted.push(entry.manifest_path.clone());
        if entry.entry_type == CatalogItemType::App
            && let Some(dir) = manifest.parent()
        {
            wanted.push(
                dir.join(DISTRIBUTION_FILE_NAME)
                    .to_string_lossy()
                    .into_owned(),
            );
        }
    }
    wanted.sort();
    wanted.dedup();
    if wanted.len() > MAX_FILES {
        return Err(SyncError::TooManyFiles);
    }
    Ok(wanted)
}

/// Écrit les fichiers dans un dossier temporaire, puis le substitue d'un seul renommage.
async fn publish(
    repos_dir: &Path,
    repository_id: &str,
    files: &[(String, String)],
) -> Result<(), SyncError> {
    tokio::fs::create_dir_all(repos_dir).await?;
    let unique = uuid::Uuid::new_v4().simple();
    let staging = repos_dir.join(format!(".{repository_id}.tmp-{unique}"));
    for (relative, content) in files {
        let target = staging.join(relative);
        if let Some(parent) = target.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(&target, content).await?;
    }
    let destination = repos_dir.join(repository_id);
    let previous: PathBuf = repos_dir.join(format!(".{repository_id}.old-{unique}"));
    let had_previous = tokio::fs::try_exists(&destination).await?;
    if had_previous {
        tokio::fs::rename(&destination, &previous).await?;
    }
    if let Err(error) = tokio::fs::rename(&staging, &destination).await {
        if had_previous {
            let _ = tokio::fs::rename(&previous, &destination).await;
        }
        let _ = tokio::fs::remove_dir_all(&staging).await;
        return Err(SyncError::Storage(error));
    }
    if had_previous {
        let _ = tokio::fs::remove_dir_all(&previous).await;
    }
    Ok(())
}

/// Synchronise un dépôt : index, manifestes et contrats de distribution, copiés atomiquement.
pub async fn sync_repository(
    repository: &PluginRepository,
    repos_dir: &Path,
    policy: &SyncPolicy,
) -> Result<SyncReport, SyncError> {
    if !is_valid_repository_id(&repository.id) {
        return Err(SyncError::InvalidRepositoryId);
    }
    let index_url = Url::parse(&repository.index_url)
        .map_err(|error| SyncError::InvalidUrl(error.to_string()))?;
    let base = base_url(&index_url)?;
    let client = pinned_client(&index_url, policy).await?;

    let index_text = fetch_text(&client, &index_url)
        .await?
        .ok_or_else(|| SyncError::Http {
            status: StatusCode::NOT_FOUND.as_u16(),
            url: index_url.to_string(),
        })?;
    let parsed = parse_index_tolerant(&index_text)
        .map_err(|error| SyncError::InvalidIndex(error.to_string()))?;
    let mut skipped = parsed.skipped;
    let wanted = wanted_files(&parsed.index.entries)?;

    let fetched: Vec<(String, Result<Option<String>, SyncError>)> = stream::iter(wanted)
        .map(|relative| {
            let client = client.clone();
            let base = base.clone();
            async move {
                let result = match base.join(&relative) {
                    Ok(url) => fetch_text(&client, &url).await,
                    Err(error) => Err(SyncError::InvalidUrl(error.to_string())),
                };
                (relative, result)
            }
        })
        .buffer_unordered(FETCH_CONCURRENCY)
        .collect()
        .await;

    let mut files: Vec<(String, String)> = vec![(INDEX_RELATIVE_PATH.to_owned(), index_text)];
    for (relative, result) in fetched {
        match result {
            Ok(Some(content)) if serde_json::from_str::<serde_json::Value>(&content).is_ok() => {
                files.push((relative, content));
            }
            Ok(Some(_)) => skipped.push(format!("{relative} : JSON invalide")),
            Ok(None) => {
                if !relative.ends_with(DISTRIBUTION_FILE_NAME) {
                    skipped.push(format!("{relative} : introuvable"));
                }
            }
            Err(error) => skipped.push(format!("{relative} : {error}")),
        }
    }
    let report = SyncReport {
        entries: parsed.index.entries.len(),
        files: files.len(),
        skipped,
    };
    publish(repos_dir, &repository.id, &files).await?;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::CatalogEntry;

    fn entry(id: &str, kind: CatalogItemType, manifest: &str) -> CatalogEntry {
        CatalogEntry {
            id: id.to_owned(),
            entry_type: kind,
            version: "1.0.0".to_owned(),
            name: id.to_owned(),
            manifest_path: manifest.to_owned(),
            capability: None,
            current_choice: None,
            distribution: None,
            category: None,
            tags: Vec::new(),
        }
    }

    #[test]
    fn test_forbidden_addresses_cover_internal_ranges() {
        for forbidden in [
            "127.0.0.1",
            "10.1.2.3",
            "172.16.0.1",
            "192.168.1.50",
            "169.254.169.254",
            "0.0.0.0",
            "::1",
            "fe80::1",
            "fd00::1",
            "::ffff:192.168.0.1",
        ] {
            let ip: IpAddr = forbidden.parse().unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST));
            assert!(is_forbidden_ip(ip), "{forbidden}");
        }
        for allowed in ["8.8.8.8", "140.82.121.4", "2606:4700::1111"] {
            let ip: IpAddr = allowed.parse().unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST));
            assert!(!is_forbidden_ip(ip), "{allowed}");
        }
    }

    #[test]
    fn test_base_url_strips_the_index_suffix() -> Result<(), SyncError> {
        let url = Url::parse("https://example.org/depot/main/catalogue/index.json?x=1")
            .map_err(|error| SyncError::InvalidUrl(error.to_string()))?;
        assert_eq!(base_url(&url)?.as_str(), "https://example.org/depot/main/");
        let wrong = Url::parse("https://example.org/depot/liste.json")
            .map_err(|error| SyncError::InvalidUrl(error.to_string()))?;
        assert!(matches!(base_url(&wrong), Err(SyncError::InvalidUrl(_))));
        Ok(())
    }

    #[test]
    fn test_wanted_files_add_distribution_for_apps_and_skip_unsafe_paths() -> Result<(), SyncError>
    {
        let entries = vec![
            entry(
                "app.mediainfo",
                CatalogItemType::App,
                "apps/mediainfo/manifest.json",
            ),
            entry(
                "plugin.x",
                CatalogItemType::Plugin,
                "plugins/x/manifest.json",
            ),
            entry("plugin.evil", CatalogItemType::Plugin, "../../etc/passwd"),
            entry("plugin.abs", CatalogItemType::Plugin, "/etc/passwd"),
        ];
        let wanted = wanted_files(&entries)?;
        assert_eq!(
            wanted,
            vec![
                "apps/mediainfo/distribution.v1.json",
                "apps/mediainfo/manifest.json",
                "plugins/x/manifest.json",
            ]
        );
        Ok(())
    }

    #[test]
    fn test_too_many_declared_files_are_refused() {
        let entries: Vec<_> = (0..=MAX_FILES)
            .map(|index| {
                entry(
                    &format!("p{index}"),
                    CatalogItemType::Plugin,
                    &format!("p/{index}.json"),
                )
            })
            .collect();
        assert!(matches!(
            wanted_files(&entries),
            Err(SyncError::TooManyFiles)
        ));
    }

    #[tokio::test]
    async fn test_publish_replaces_the_previous_copy_atomically() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        publish(
            dir.path(),
            "repo.test",
            &[("catalogue/index.json".into(), "{\"v\":1}".into())],
        )
        .await?;
        publish(
            dir.path(),
            "repo.test",
            &[("catalogue/index.json".into(), "{\"v\":2}".into())],
        )
        .await?;
        let content = std::fs::read_to_string(dir.path().join("repo.test/catalogue/index.json"))?;
        assert_eq!(content, "{\"v\":2}");
        let leftovers: Vec<_> = std::fs::read_dir(dir.path())?
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().starts_with('.'))
            .collect();
        assert!(leftovers.is_empty());
        Ok(())
    }

    #[tokio::test]
    async fn test_sync_refuses_private_hosts_by_default() {
        let repository = PluginRepository {
            id: "repo.local".into(),
            name: "Local".into(),
            index_url: "http://127.0.0.1:9/catalogue/index.json".into(),
            is_official: false,
            enabled: true,
            last_synced_at: None,
            created_at: String::new(),
        };
        let dir = std::env::temp_dir();
        let result = sync_repository(&repository, &dir, &SyncPolicy::default()).await;
        assert!(matches!(result, Err(SyncError::ForbiddenHost(_))));
    }

    #[tokio::test]
    async fn test_sync_refuses_unsafe_repository_ids() {
        let repository = PluginRepository {
            id: "../evil".into(),
            name: "Mal".into(),
            index_url: "https://example.org/catalogue/index.json".into(),
            is_official: false,
            enabled: true,
            last_synced_at: None,
            created_at: String::new(),
        };
        let dir = std::env::temp_dir();
        let result = sync_repository(&repository, &dir, &SyncPolicy::default()).await;
        assert!(matches!(result, Err(SyncError::InvalidRepositoryId)));
    }

    async fn serve_fixture() -> anyhow::Result<SocketAddr> {
        use axum::{Router, http::StatusCode, routing::get};
        let router = Router::new()
            .route(
                "/depot/catalogue/index.json",
                get(|| async {
                    r#"{"schemaVersion":"1.0.0","entries":[
                        {"id":"app.demo","type":"app","version":"1.0.0","name":"Démo","manifestPath":"apps/demo/manifest.json","capability":"probe","currentChoice":true},
                        {"id":"plugin.x","type":"plugin","version":"1.0.0","name":"X","manifestPath":"plugins/x/manifest.json"},
                        {"id":"tool.ancien","type":"tool","version":"1.0.0","name":"Ancien","manifestPath":"tools/ancien/manifest.json"}
                    ]}"#
                }),
            )
            .route(
                "/depot/apps/demo/manifest.json",
                get(|| async { r#"{"schemaVersion":"1.0.0","id":"app.demo","type":"app","version":"1.0.0","name":"Démo"}"# }),
            )
            .route(
                "/depot/apps/demo/distribution.v1.json",
                get(|| async { r#"{"toolId":"app.demo","toolVersion":"1.0.0","artifacts":[]}"# }),
            )
            .route("/depot/plugins/x/manifest.json", get(|| async { StatusCode::NOT_FOUND }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let address = listener.local_addr()?;
        tokio::spawn(async move {
            let _ = axum::serve(listener, router).await;
        });
        Ok(address)
    }

    #[tokio::test]
    async fn test_sync_copies_index_manifests_and_distribution_and_skips_unknown_entries()
    -> anyhow::Result<()> {
        let address = serve_fixture().await?;
        let dir = tempfile::tempdir()?;
        let repository = PluginRepository {
            id: "repo.fixture".into(),
            name: "Fixture".into(),
            index_url: format!("http://{address}/depot/catalogue/index.json"),
            is_official: false,
            enabled: true,
            last_synced_at: None,
            created_at: String::new(),
        };
        let policy = SyncPolicy {
            allow_private_hosts: true,
        };

        let report = sync_repository(&repository, dir.path(), &policy).await?;
        assert_eq!(report.entries, 2);
        assert!(
            report
                .skipped
                .iter()
                .any(|reason| reason.contains("tool.ancien"))
        );
        assert!(
            report
                .skipped
                .iter()
                .any(|reason| reason.contains("plugins/x/manifest.json"))
        );

        let root = dir.path().join("repo.fixture");
        assert!(root.join("catalogue/index.json").is_file());
        assert!(root.join("apps/demo/manifest.json").is_file());
        assert!(root.join("apps/demo/distribution.v1.json").is_file());
        let entries = crate::plugins::list_catalog(&[root]);
        assert_eq!(entries.len(), 2);

        let again = sync_repository(&repository, dir.path(), &policy).await?;
        assert_eq!(again.entries, 2);
        Ok(())
    }

    #[tokio::test]
    async fn test_failed_sync_keeps_the_previous_copy() -> anyhow::Result<()> {
        let address = serve_fixture().await?;
        let dir = tempfile::tempdir()?;
        let mut repository = PluginRepository {
            id: "repo.fixture".into(),
            name: "Fixture".into(),
            index_url: format!("http://{address}/depot/catalogue/index.json"),
            is_official: false,
            enabled: true,
            last_synced_at: None,
            created_at: String::new(),
        };
        let policy = SyncPolicy {
            allow_private_hosts: true,
        };
        sync_repository(&repository, dir.path(), &policy).await?;

        repository.index_url = format!("http://{address}/absent/catalogue/index.json");
        let failure = sync_repository(&repository, dir.path(), &policy).await;
        assert!(matches!(failure, Err(SyncError::Http { status: 404, .. })));
        assert!(
            dir.path()
                .join("repo.fixture/catalogue/index.json")
                .is_file()
        );
        Ok(())
    }

    #[test]
    fn test_validate_index_url_checks_scheme_and_suffix() {
        assert!(validate_index_url("https://example.org/d/catalogue/index.json").is_ok());
        assert!(validate_index_url("ftp://example.org/d/catalogue/index.json").is_err());
        assert!(validate_index_url("https://example.org/d/index.json").is_err());
        assert!(validate_index_url("pas une url").is_err());
    }
}
