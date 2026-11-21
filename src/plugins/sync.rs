//! Synchronisation d'un dépôt de plugins vers son dossier local : bornée, sans redirection,
//! refusant les adresses privées, et atomique (l'ancienne copie reste en place en cas d'échec).

use super::{
    CatalogItemType, PluginRepository, icon::check_svg_icon, is_valid_repository_id,
    parse_index_tolerant,
};
use crate::{
    net_policy::{self, NetPolicyError, OutboundPolicy},
    tools_cache::ToolCacheManager,
};
use futures_util::{StreamExt, stream};
use reqwest::{Client, StatusCode, Url};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    time::Duration,
};
use subtle::ConstantTimeEq;

const INDEX_RELATIVE_PATH: &str = "index.json";
const DISTRIBUTION_FILE_NAME: &str = "distribution.v1.json";
/// Variable d'environnement qui autorise les adresses privées (réseaux de laboratoire, tests).
pub const ALLOW_PRIVATE_HOSTS_ENV: &str = net_policy::ALLOW_PRIVATE_HOSTS_ENV;
/// Variable d'environnement imposant des releases immuables (URL figée, `releaseVersion` et SHA-256 complets).
pub const REQUIRE_IMMUTABLE_RELEASES_ENV: &str = "PLUGIN_REQUIRE_IMMUTABLE_RELEASES";
const SYNC_TIMEOUT: Duration = Duration::from_secs(15);
const MAX_FILE_BYTES: usize = 1024 * 1024;
const MAX_FILES: usize = 500;
const FETCH_CONCURRENCY: usize = 8;

/// Règles réseau et d'immuabilité d'une synchronisation.
#[derive(Clone, Copy, Debug, Default)]
pub struct SyncPolicy {
    pub allow_private_hosts: bool,
    pub require_immutable_releases: bool,
}

impl SyncPolicy {
    pub fn from_env() -> Self {
        Self {
            allow_private_hosts: std::env::var(ALLOW_PRIVATE_HOSTS_ENV).is_ok_and(|v| v == "1"),
            require_immutable_releases: std::env::var(REQUIRE_IMMUTABLE_RELEASES_ENV)
                .is_ok_and(|v| v == "1"),
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
    #[error("intégrité SHA-256 refusée : {0}")]
    IntegrityMismatch(String),
    #[error("release immuable violée : {0}")]
    ImmutableViolation(String),
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
    /// Sous-catégories déclarées mais non retenues (ADR 0023) ; le plugin reste dans sa catégorie.
    pub warnings: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release_version: Option<String>,
    #[serde(default)]
    pub index_sha256: String,
}

/// Indique si une URL pointe vers une branche Git mouvante (`dev`, `main`, `master`, `refs/heads/`).
pub fn is_mutable_branch_url(url: &Url) -> bool {
    let path = url.path();
    if path.contains("/refs/heads/")
        || path.ends_with("/dev/index.json")
        || path.ends_with("/main/index.json")
        || path.ends_with("/master/index.json")
    {
        return true;
    }
    if url.host_str() == Some("raw.githubusercontent.com") {
        let mut segments = path.trim_start_matches('/').split('/');
        if let (Some(_owner), Some(_repo), Some(branch)) =
            (segments.next(), segments.next(), segments.next())
        {
            return matches!(branch, "dev" | "main" | "master");
        }
    }
    false
}

/// Adresse qui ne doit jamais être jointe depuis un dépôt distant (réseau interne, métadonnées cloud).
#[cfg(test)]
fn is_forbidden_ip(ip: std::net::IpAddr) -> bool {
    net_policy::is_forbidden_ip(ip)
}

impl From<NetPolicyError> for SyncError {
    fn from(error: NetPolicyError) -> Self {
        match error {
            NetPolicyError::InvalidUrl(msg) => Self::InvalidUrl(msg),
            NetPolicyError::ForbiddenHost(host) => Self::ForbiddenHost(host),
            NetPolicyError::Network(msg) => Self::Network(msg),
        }
    }
}

/// Base du dépôt : l'URL de l'index privée de `index.json`.
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

/// Valide l'URL d'index d'un dépôt à l'ajout : schéma http(s) et suffixe `/index.json`.
pub fn validate_index_url(raw: &str) -> Result<(), SyncError> {
    let url = Url::parse(raw).map_err(|error| SyncError::InvalidUrl(error.to_string()))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(SyncError::InvalidUrl("schéma http ou https attendu".into()));
    }
    base_url(&url).map(|_| ())
}

/// Client dont la résolution DNS est figée sur toutes les adresses IP publiques déjà contrôlées.
async fn pinned_client(url: &Url, policy: &SyncPolicy) -> Result<Client, SyncError> {
    let outbound = OutboundPolicy {
        allow_private_hosts: policy.allow_private_hosts,
        require_https: false,
    };
    let (_, addresses) = net_policy::resolve_checked(url, outbound).await?;
    Ok(net_policy::build_pinned_client(
        url,
        &addresses,
        SYNC_TIMEOUT,
    )?)
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

fn resolve_manifest_relative(manifest_rel: &str, declared_rel: &str) -> Option<String> {
    let trimmed = declared_rel.trim().trim_start_matches("./");
    if trimmed.is_empty() || !ToolCacheManager::is_safe_relative_path(Path::new(trimmed)) {
        return None;
    }
    let joined = match Path::new(manifest_rel).parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.join(trimmed),
        _ => PathBuf::from(trimmed),
    };
    if !ToolCacheManager::is_safe_relative_path(&joined) {
        return None;
    }
    Some(joined.to_string_lossy().replace('\\', "/"))
}

/// Déduit les fichiers JSON annexes déclarés dans les manifestes (`files[]` et `pack.installation`).
fn extra_files_from_manifests(
    already_queued: &HashSet<String>,
    fetched_files: &[(String, String)],
) -> Result<Vec<String>, SyncError> {
    let mut extra: Vec<String> = Vec::new();
    let mut seen = already_queued.clone();
    for (manifest_rel, content) in fetched_files {
        let Ok(doc) = serde_json::from_str::<serde_json::Value>(content) else {
            continue;
        };
        if let Some(declared) = doc.get("files").and_then(|v| v.as_array()) {
            for item in declared {
                let Some(path) = item.get("path").and_then(|v| v.as_str()) else {
                    continue;
                };
                let is_json = path.ends_with(".json")
                    || item.get("mediaType").and_then(|v| v.as_str()) == Some("application/json");
                if is_json
                    && let Some(resolved) = resolve_manifest_relative(manifest_rel, path)
                    && seen.insert(resolved.clone())
                {
                    extra.push(resolved);
                }
            }
        }
        if let Some(install_path) = doc
            .get("pack")
            .and_then(|pack| pack.get("installation"))
            .and_then(|v| v.as_str())
            && let Some(resolved) = resolve_manifest_relative(manifest_rel, install_path)
            && seen.insert(resolved.clone())
        {
            extra.push(resolved);
        }
    }
    if already_queued.len() + extra.len() > MAX_FILES {
        return Err(SyncError::TooManyFiles);
    }
    extra.sort();
    Ok(extra)
}

fn verify_optional_sha256(
    relative: &str,
    content: &str,
    expected_by_path: &HashMap<String, String>,
    require_digest: bool,
) -> Result<(), SyncError> {
    let Some(expected) = expected_by_path.get(relative) else {
        if require_digest {
            return Err(SyncError::IntegrityMismatch(format!(
                "{relative} : empreinte SHA-256 manquante dans l'index"
            )));
        }
        return Ok(());
    };
    let actual = ToolCacheManager::compute_sha256(content.as_bytes());
    let expected_norm = expected.to_lowercase();
    if bool::from(actual.as_bytes().ct_eq(expected_norm.as_bytes())) {
        Ok(())
    } else {
        Err(SyncError::IntegrityMismatch(format!(
            "{relative} : empreinte sha256 invalide (attendu {expected_norm}, reçu {actual})"
        )))
    }
}

async fn fetch_json_batch(
    client: &Client,
    base: &Url,
    paths: Vec<String>,
) -> Vec<(String, Result<Option<String>, SyncError>)> {
    stream::iter(paths)
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
        .await
}

/// Chemins (relatifs à la racine du dépôt) des icônes SVG déclarées par les manifestes.
fn icon_paths_from_manifests(manifests: &[(String, String)]) -> Vec<String> {
    let mut paths: Vec<String> = manifests
        .iter()
        .filter_map(|(manifest_rel, content)| {
            let document: serde_json::Value = serde_json::from_str(content).ok()?;
            let declared = document.get("icon")?.as_str()?;
            if !declared.trim().ends_with(".svg") {
                return None;
            }
            resolve_manifest_relative(manifest_rel, declared)
        })
        .collect();
    paths.sort();
    paths.dedup();
    paths
}

/// Retire des fichiers à copier les contrats d'opération dont les variables, expositions ou
/// dépendances sont invalides ; chacun est signalé dans le bilan (ADR 0025). Le central ne se fie
/// pas à la CI du dépôt : il revalide lui-même.
fn reject_invalid_operation_declarations(
    files: &mut Vec<(String, String)>,
    skipped: &mut Vec<String>,
) {
    files.retain(
        |(relative, content)| match super::operation_declaration_issue(content) {
            Some(issue) => {
                skipped.push(format!("{relative} : déclarations refusées ({issue})"));
                false
            }
            None => true,
        },
    );
}

/// Garde les icônes qui passent la validation par liste blanche ; les autres sont signalées.
fn collect_icons(
    fetched: Vec<(String, Result<Option<String>, SyncError>)>,
    expected_sha256: &HashMap<String, String>,
    require_digest: bool,
    files: &mut Vec<(String, String)>,
    skipped: &mut Vec<String>,
) -> Result<(), SyncError> {
    for (relative, result) in fetched {
        match result {
            Ok(Some(content)) => match check_svg_icon(&content) {
                Ok(()) => {
                    verify_optional_sha256(&relative, &content, expected_sha256, require_digest)?;
                    files.push((relative, content));
                }
                Err(issue) => skipped.push(format!("{relative} : icône refusée ({issue})")),
            },
            Ok(None) => skipped.push(format!("{relative} : icône introuvable")),
            Err(error) => skipped.push(format!("{relative} : {error}")),
        }
    }
    Ok(())
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

struct PreparedSync {
    files: Vec<(String, String)>,
    report: SyncReport,
}

async fn prepare_sync(
    repository: &PluginRepository,
    policy: &SyncPolicy,
) -> Result<PreparedSync, SyncError> {
    if !is_valid_repository_id(&repository.id) {
        return Err(SyncError::InvalidRepositoryId);
    }
    let index_url = Url::parse(&repository.index_url)
        .map_err(|error| SyncError::InvalidUrl(error.to_string()))?;
    if policy.require_immutable_releases && is_mutable_branch_url(&index_url) {
        return Err(SyncError::ImmutableViolation(format!(
            "l'URL pointe vers une branche mouvante ({index_url}) ; utilisez un tag ou une release immuable"
        )));
    }
    let base = base_url(&index_url)?;
    let client = pinned_client(&index_url, policy).await?;

    let index_text = fetch_text(&client, &index_url)
        .await?
        .ok_or_else(|| SyncError::Http {
            status: StatusCode::NOT_FOUND.as_u16(),
            url: index_url.to_string(),
        })?;
    let index_sha256 = ToolCacheManager::compute_sha256(index_text.as_bytes());
    let parsed = parse_index_tolerant(&index_text)
        .map_err(|error| SyncError::InvalidIndex(error.to_string()))?;
    let release_version = parsed.index.release_version.clone();
    if policy.require_immutable_releases && release_version.is_none() {
        return Err(SyncError::ImmutableViolation(
            "index.json doit déclarer 'releaseVersion'".into(),
        ));
    }
    let mut skipped = parsed.skipped;
    let mut expected_sha256: HashMap<String, String> = parsed.index.files_sha256.clone();
    for entry in &parsed.index.entries {
        if let Some(digest) = &entry.sha256 {
            expected_sha256.insert(entry.manifest_path.clone(), digest.clone());
        } else if policy.require_immutable_releases {
            return Err(SyncError::ImmutableViolation(format!(
                "{} : empreinte sha256 manquante dans l'index",
                entry.id
            )));
        }
    }
    let wanted = wanted_files(&parsed.index.entries)?;
    let mut already_queued: HashSet<String> = wanted.iter().cloned().collect();
    already_queued.insert(INDEX_RELATIVE_PATH.to_owned());

    let fetched = fetch_json_batch(&client, &base, wanted).await;
    let mut files: Vec<(String, String)> = vec![(INDEX_RELATIVE_PATH.to_owned(), index_text)];
    let mut manifest_docs: Vec<(String, String)> = Vec::new();
    collect_fetched_json(
        fetched,
        &expected_sha256,
        policy.require_immutable_releases,
        &mut files,
        &mut manifest_docs,
        &mut skipped,
    )?;

    let extra = extra_files_from_manifests(&already_queued, &manifest_docs)?;
    if !extra.is_empty() {
        let extra_fetched = fetch_json_batch(&client, &base, extra).await;
        collect_fetched_json(
            extra_fetched,
            &expected_sha256,
            policy.require_immutable_releases,
            &mut files,
            &mut Vec::new(),
            &mut skipped,
        )?;
    }
    reject_invalid_operation_declarations(&mut files, &mut skipped);
    let icons = icon_paths_from_manifests(&manifest_docs);
    if files.len() + icons.len() > MAX_FILES {
        return Err(SyncError::TooManyFiles);
    }
    if !icons.is_empty() {
        let fetched_icons = fetch_json_batch(&client, &base, icons).await;
        collect_icons(
            fetched_icons,
            &expected_sha256,
            policy.require_immutable_releases,
            &mut files,
            &mut skipped,
        )?;
    }
    let warnings = super::subcategory::repository_warnings(&parsed.index.entries, |entry| {
        manifest_docs
            .iter()
            .find(|(relative, _)| *relative == entry.manifest_path)
            .map(|(_, content)| content.clone())
    });
    for warning in &warnings {
        tracing::warn!(repository = %repository.id, %warning, "sous-catégorie non retenue");
    }
    let report = SyncReport {
        entries: parsed.index.entries.len(),
        files: files.len(),
        skipped,
        warnings,
        release_version,
        index_sha256,
    };
    Ok(PreparedSync { files, report })
}

/// Synchronise un dépôt : index, manifestes et contrats de distribution, copiés atomiquement.
pub async fn sync_repository(
    repository: &PluginRepository,
    repos_dir: &Path,
    policy: &SyncPolicy,
) -> Result<SyncReport, SyncError> {
    let prepared = prepare_sync(repository, policy).await?;
    publish(repos_dir, &repository.id, &prepared.files).await?;
    Ok(prepared.report)
}

/// Synchronise un dépôt en imposant l'immuabilité TOFU en base (`plugin_release_pins`) avant publication.
pub async fn sync_repository_verified(
    pool: &sqlx::SqlitePool,
    repository: &PluginRepository,
    repos_dir: &Path,
    policy: &SyncPolicy,
) -> Result<SyncReport, SyncError> {
    let prepared = prepare_sync(repository, policy).await?;
    if let Some(release_version) = &prepared.report.release_version {
        let scope = format!("repo:{}", repository.id);
        match crate::repository::verify_or_pin_release(
            pool,
            &scope,
            release_version,
            &prepared.report.index_sha256,
            &repository.index_url,
        )
        .await
        {
            Ok(None) => {}
            Ok(Some(expected)) => {
                return Err(SyncError::ImmutableViolation(format!(
                    "la release {release_version} du dépôt {} a changé d'empreinte (attendu {expected}, reçu {})",
                    repository.id, prepared.report.index_sha256
                )));
            }
            Err(error) => return Err(SyncError::Network(error.to_string())),
        }
    }
    publish(repos_dir, &repository.id, &prepared.files).await?;
    Ok(prepared.report)
}

fn collect_fetched_json(
    fetched: Vec<(String, Result<Option<String>, SyncError>)>,
    expected_sha256: &HashMap<String, String>,
    require_digest: bool,
    files: &mut Vec<(String, String)>,
    manifests: &mut Vec<(String, String)>,
    skipped: &mut Vec<String>,
) -> Result<(), SyncError> {
    for (relative, result) in fetched {
        match result {
            Ok(Some(content)) if serde_json::from_str::<serde_json::Value>(&content).is_ok() => {
                verify_optional_sha256(&relative, &content, expected_sha256, require_digest)?;
                manifests.push((relative.clone(), content.clone()));
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
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::CatalogEntry;

    use std::net::{IpAddr, Ipv4Addr, SocketAddr};

    fn entry(id: &str, kind: CatalogItemType, manifest: &str) -> CatalogEntry {
        CatalogEntry {
            id: id.to_owned(),
            entry_type: kind,
            version: "1.0.0".to_owned(),
            name: id.to_owned(),
            manifest_path: manifest.to_owned(),
            description: None,
            capability: None,
            current_choice: None,
            distribution: None,
            category: None,
            subcategory: None,
            has_icon: false,
            tags: Vec::new(),
            owner: None,
            sha256: None,
            updated_at: None,
            runtime: None,
        }
    }

    #[test]
    fn test_extra_files_extracts_pack_installation_and_json_files_safely() -> Result<(), SyncError>
    {
        let already = HashSet::from([
            "index.json".to_owned(),
            "packs/media/manifest.json".to_owned(),
        ]);
        let manifests = vec![(
            "packs/media/manifest.json".to_owned(),
            r#"{
                "files": [
                    {"path": "README.md", "role": "docs", "mediaType": "text/markdown"},
                    {"path": "blocks/extra.json", "mediaType": "application/json"},
                    {"path": "../../etc/passwd.json", "mediaType": "application/json"}
                ],
                "pack": {"installation": "installation.json"}
            }"#
            .to_owned(),
        )];
        let extra = extra_files_from_manifests(&already, &manifests)?;
        assert_eq!(
            extra,
            vec![
                "packs/media/blocks/extra.json".to_owned(),
                "packs/media/installation.json".to_owned(),
            ]
        );
        Ok(())
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
        let url = Url::parse("https://example.org/depot/main/index.json?x=1")
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
                "mediainfo",
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
            &[("index.json".into(), "{\"v\":1}".into())],
        )
        .await?;
        publish(
            dir.path(),
            "repo.test",
            &[("index.json".into(), "{\"v\":2}".into())],
        )
        .await?;
        let content = std::fs::read_to_string(dir.path().join("repo.test/index.json"))?;
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
            index_url: "http://127.0.0.1:9/index.json".into(),
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
            index_url: "https://example.org/index.json".into(),
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
                "/depot/index.json",
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
            index_url: format!("http://{address}/depot/index.json"),
            is_official: false,
            enabled: true,
            last_synced_at: None,
            created_at: String::new(),
        };
        let policy = SyncPolicy {
            allow_private_hosts: true,
            ..SyncPolicy::default()
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
        assert!(root.join("index.json").is_file());
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
            index_url: format!("http://{address}/depot/index.json"),
            is_official: false,
            enabled: true,
            last_synced_at: None,
            created_at: String::new(),
        };
        let policy = SyncPolicy {
            allow_private_hosts: true,
            ..SyncPolicy::default()
        };
        sync_repository(&repository, dir.path(), &policy).await?;

        repository.index_url = format!("http://{address}/absent/index.json");
        let failure = sync_repository(&repository, dir.path(), &policy).await;
        assert!(matches!(failure, Err(SyncError::Http { status: 404, .. })));
        assert!(dir.path().join("repo.fixture/index.json").is_file());
        Ok(())
    }

    #[test]
    fn test_validate_index_url_checks_scheme_and_suffix() {
        assert!(validate_index_url("https://example.org/d/index.json").is_ok());
        assert!(validate_index_url("ftp://example.org/d/index.json").is_err());
        assert!(validate_index_url("https://example.org/d/catalogue.json").is_err());
        assert!(validate_index_url("https://example.org/d/mon-index.json").is_err());
        assert!(validate_index_url("pas une url").is_err());
    }

    #[test]
    fn test_mutable_branch_url_detection() -> anyhow::Result<()> {
        for mutable in [
            "https://raw.githubusercontent.com/org/repo/dev/index.json",
            "https://raw.githubusercontent.com/org/repo/main/index.json",
            "https://raw.githubusercontent.com/org/repo/refs/heads/main/index.json",
            "https://example.org/depot/master/index.json",
        ] {
            assert!(is_mutable_branch_url(&Url::parse(mutable)?), "{mutable}");
        }
        for immutable in [
            "https://raw.githubusercontent.com/org/repo/catalogue-v1.0.0/index.json",
            "https://github.com/org/repo/releases/download/tools-1.0.0/ffmpeg.tar.gz",
            "https://example.org/releases/v2.1.0/index.json",
        ] {
            assert!(
                !is_mutable_branch_url(&Url::parse(immutable)?),
                "{immutable}"
            );
        }
        Ok(())
    }

    #[tokio::test]
    async fn test_integrity_mismatch_and_immutable_pin_abort_sync_without_overwriting()
    -> anyhow::Result<()> {
        use axum::{Router, routing::get};
        use sqlx::sqlite::SqlitePoolOptions;
        use std::sync::Arc;

        let manifest_v1 = r#"{"schemaVersion":"1.1.0","id":"plugin.a","type":"plugin","version":"1.0.0","name":"A","description":"A","author":"a","license":"MIT"}"#.to_owned();
        let manifest_sha = ToolCacheManager::compute_sha256(manifest_v1.as_bytes());
        let index_v1 = serde_json::json!({
            "schemaVersion": "1.0.0",
            "generatedFor": "demo",
            "releaseVersion": "1.0.0",
            "filesSha256": { "plugins/a/manifest.json": manifest_sha },
            "entries": [{
                "id": "plugin.a",
                "type": "plugin",
                "version": "1.0.0",
                "name": "A",
                "manifestPath": "plugins/a/manifest.json",
                "sha256": manifest_sha
            }]
        })
        .to_string();
        let state = Arc::new(tokio::sync::Mutex::new((
            index_v1.clone(),
            manifest_v1.clone(),
        )));
        let st_idx = state.clone();
        let st_man = state.clone();
        let router = Router::new()
            .route(
                "/v1.0.0/index.json",
                get(move || {
                    let st = st_idx.clone();
                    async move { st.lock().await.0.clone() }
                }),
            )
            .route(
                "/v1.0.0/plugins/a/manifest.json",
                get(move || {
                    let st = st_man.clone();
                    async move { st.lock().await.1.clone() }
                }),
            );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let address = listener.local_addr()?;
        tokio::spawn(async move {
            let _ = axum::serve(listener, router).await;
        });

        let pool = SqlitePoolOptions::new().connect("sqlite::memory:").await?;
        sqlx::migrate!().run(&pool).await?;
        let dir = tempfile::tempdir()?;
        let repo = PluginRepository {
            id: "repo.immutable".into(),
            name: "Immutable".into(),
            index_url: format!("http://{address}/v1.0.0/index.json"),
            is_official: false,
            enabled: true,
            last_synced_at: None,
            created_at: String::new(),
        };
        let strict_policy = SyncPolicy {
            allow_private_hosts: true,
            require_immutable_releases: true,
        };

        let ok_report = sync_repository_verified(&pool, &repo, dir.path(), &strict_policy).await?;
        assert_eq!(ok_report.release_version.as_deref(), Some("1.0.0"));

        // Mutation du manifeste sans recalcul du SHA : IntegrityMismatch et copie précédente préservée
        state.lock().await.1 = manifest_v1.replace("\"A\"", "\"Tampered\"");
        let mismatch = sync_repository_verified(&pool, &repo, dir.path(), &strict_policy).await;
        assert!(matches!(mismatch, Err(SyncError::IntegrityMismatch(_))));

        // Mutation de l'index à version identique 1.0.0 : ImmutableViolation (TOFU)
        let mutated_manifest = manifest_v1.replace("\"A\"", "\"Mutated\"");
        let mutated_sha = ToolCacheManager::compute_sha256(mutated_manifest.as_bytes());
        let mutated_index = serde_json::json!({
            "schemaVersion": "1.0.0",
            "generatedFor": "demo",
            "releaseVersion": "1.0.0",
            "filesSha256": { "plugins/a/manifest.json": mutated_sha },
            "entries": [{
                "id": "plugin.a",
                "type": "plugin",
                "version": "1.0.0",
                "name": "Mutated",
                "manifestPath": "plugins/a/manifest.json",
                "sha256": mutated_sha
            }]
        })
        .to_string();
        *state.lock().await = (mutated_index, mutated_manifest);
        let pin_err = sync_repository_verified(&pool, &repo, dir.path(), &strict_policy).await;
        assert!(matches!(pin_err, Err(SyncError::ImmutableViolation(_))));

        let on_disk =
            std::fs::read_to_string(dir.path().join("repo.immutable/plugins/a/manifest.json"))?;
        assert!(on_disk.contains("\"A\""));
        Ok(())
    }

    const ICON: &str = r#"<svg xmlns="http://www.w3.org/2000/svg"/>"#;

    type Fetched = (String, Result<Option<String>, SyncError>);

    fn fetched(path: &str, body: Option<&str>) -> Fetched {
        (path.to_owned(), Ok(body.map(str::to_owned)))
    }

    fn failed(path: &str) -> Fetched {
        (path.to_owned(), Err(SyncError::Network("coupé".into())))
    }

    #[test]
    fn icon_paths_are_resolved_beside_the_manifest_and_unsafe_ones_are_ignored() {
        let manifest = |dir: &str, icon: &str| {
            (
                format!("plugins/{dir}/manifest.json"),
                format!(r#"{{"icon": "{icon}"}}"#),
            )
        };
        let manifests = vec![
            manifest("plex", "icon.svg"),
            manifest("jelly", "./img/icon.svg"),
            manifest("twin", "icon.svg"),
            manifest("evil", "../../secret.svg"),
            manifest("png", "icon.png"),
            ("plugins/none/manifest.json".to_owned(), "{}".to_owned()),
            ("plugins/b/manifest.json".to_owned(), "{".to_owned()),
        ];
        let found = icon_paths_from_manifests(&manifests);
        assert_eq!(found.len(), 3);
        assert!(found.contains(&"plugins/plex/icon.svg".to_owned()));
        assert!(found.contains(&"plugins/jelly/img/icon.svg".to_owned()));
        assert!(found.contains(&"plugins/twin/icon.svg".to_owned()));
    }

    #[test]
    fn collect_icons_keeps_safe_svg_and_reports_the_rest() -> anyhow::Result<()> {
        let results = vec![
            fetched("plugins/a/icon.svg", Some(ICON)),
            fetched("plugins/b/icon.svg", Some("<svg onload=\"x\"/>")),
            fetched("plugins/c/icon.svg", None),
            failed("plugins/d/icon.svg"),
        ];
        let mut files = Vec::new();
        let mut skipped = Vec::new();
        collect_icons(results, &HashMap::new(), false, &mut files, &mut skipped)?;
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].0, "plugins/a/icon.svg");
        assert_eq!(skipped.len(), 3);
        assert!(skipped[0].contains("icône refusée"));
        assert!(skipped[1].contains("introuvable"));
        assert!(skipped[2].contains("coupé"));
        Ok(())
    }

    #[test]
    fn immutable_mode_requires_and_checks_the_icon_digest() -> anyhow::Result<()> {
        let path = "plugins/a/icon.svg";
        let digest = ToolCacheManager::compute_sha256(ICON.as_bytes());
        let valid = HashMap::from([(path.to_owned(), digest)]);
        let mut files = Vec::new();
        let mut skipped = Vec::new();
        collect_icons(
            vec![fetched(path, Some(ICON))],
            &valid,
            true,
            &mut files,
            &mut skipped,
        )?;
        assert_eq!(files.len(), 1);

        let missing = collect_icons(
            vec![fetched(path, Some(ICON))],
            &HashMap::new(),
            true,
            &mut files,
            &mut skipped,
        );
        assert!(matches!(missing, Err(SyncError::IntegrityMismatch(_))));

        let wrong = HashMap::from([(path.to_owned(), "0".repeat(64))]);
        let mismatch = collect_icons(
            vec![fetched(path, Some(ICON))],
            &wrong,
            true,
            &mut files,
            &mut skipped,
        );
        assert!(matches!(mismatch, Err(SyncError::IntegrityMismatch(_))));
        Ok(())
    }

    #[test]
    fn invalid_operation_declarations_are_dropped_and_reported_but_other_files_are_kept() {
        let invalid = r#"{"providerId":"p","runtime":{},"exposes":{"namespace":"nodes","contractVersion":"1.0.0","variables":[]}}"#;
        let valid = r#"{"providerId":"p","runtime":{"requires":[{"variable":"mediainfo.video.codec","provider":"app.mediainfo"}]}}"#;
        let mut files = vec![
            ("index.json".to_owned(), r#"{"entries":[]}"#.to_owned()),
            ("a/operations/bad.json".to_owned(), invalid.to_owned()),
            ("a/operations/ok.json".to_owned(), valid.to_owned()),
        ];
        let mut skipped = Vec::new();
        reject_invalid_operation_declarations(&mut files, &mut skipped);
        let kept: Vec<_> = files.iter().map(|(path, _)| path.as_str()).collect();
        assert_eq!(kept, ["index.json", "a/operations/ok.json"]);
        assert_eq!(skipped.len(), 1);
        assert!(skipped[0].contains("a/operations/bad.json") && skipped[0].contains("réservé"));
    }
}
