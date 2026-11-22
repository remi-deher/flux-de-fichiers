//! Dossiers de catalogue : un dossier par dépôt synchronisé, plus un dossier local de développement.

use super::{CatalogCache, PluginRepository, SyncPolicy, SyncReport};
use crate::{
    config::Config,
    error::{AppError, AppResult},
    repository,
};
use sqlx::SqlitePool;
use std::path::{Path, PathBuf};

const MAX_REPOSITORY_ID_LEN: usize = 64;
const INDEX_RELATIVE_PATH: &str = "index.json";

/// Un identifiant de dépôt sert de nom de dossier : lettres, chiffres, `.`, `-` et `_` seulement.
pub fn is_valid_repository_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAX_REPOSITORY_ID_LEN
        && !id.starts_with('.')
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
}

/// Dossier local d'un dépôt, ou `None` si son identifiant n'est pas sûr.
pub fn repository_dir(repos_dir: &Path, repository_id: &str) -> Option<PathBuf> {
    is_valid_repository_id(repository_id).then(|| repos_dir.join(repository_id))
}

fn has_index(dir: &Path) -> bool {
    dir.join(INDEX_RELATIVE_PATH).is_file()
}

/// Dossier du catalogue local géré depuis l'interface (sans dépôt GitHub).
pub fn local_catalog_dir(config: &Config) -> PathBuf {
    config.plugin_repos_dir().join("local")
}

/// Dossiers de catalogue à lire, dans l'ordre de priorité : le dossier local de développement,
/// le catalogue local utilisateur (`/config/plugins/local`), puis les dépôts activés déjà synchronisés.
pub async fn catalog_roots(pool: &SqlitePool, config: &Config) -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = config.plugins_dir.iter().cloned().collect();
    let local_dir = local_catalog_dir(config);
    if has_index(&local_dir) {
        roots.push(local_dir);
    }
    let repos_dir = config.plugin_repos_dir();
    if let Ok(repositories) = repository::list_plugin_repositories(pool).await {
        roots.extend(
            repositories
                .into_iter()
                .filter(|repo| repo.enabled)
                .filter_map(|repo| repository_dir(&repos_dir, &repo.id))
                .filter(|dir| has_index(dir)),
        );
    }
    roots
}

/// Synchronise un dépôt, vide le cache du catalogue, puis date la synchronisation : seul chemin qui
/// publie une copie de dépôt, il signale donc chaque changement.
pub async fn sync_and_signal(
    pool: &SqlitePool,
    cache: &CatalogCache,
    repo: &PluginRepository,
    repos_dir: &Path,
    policy: &SyncPolicy,
) -> AppResult<SyncReport> {
    let report = super::sync_repository_verified(pool, repo, repos_dir, policy)
        .await
        .map_err(|error| AppError::RepositoryUnavailable(error.to_string()))?;
    cache.invalidate().await;
    repository::touch_plugin_repository_sync(pool, cache.changes(), &repo.id).await?;
    Ok(report)
}

/// Synchronise chaque dépôt activé ; un échec est journalisé sans interrompre les autres.
pub async fn sync_enabled_repositories(pool: &SqlitePool, config: &Config, cache: &CatalogCache) {
    let Ok(repositories) = repository::list_plugin_repositories(pool).await else {
        return;
    };
    let repos_dir = config.plugin_repos_dir();
    let policy = crate::runtime_settings::load_effective(pool, config)
        .await
        .sync_policy();
    for repo in repositories.into_iter().filter(|repo| repo.enabled) {
        match sync_and_signal(pool, cache, &repo, &repos_dir, &policy).await {
            Ok(report) => {
                tracing::info!(repository = %repo.id, entries = report.entries, files = report.files, "dépôt de plugins synchronisé");
            }
            Err(error) => {
                tracing::warn!(repository = %repo.id, ?error, "synchronisation du dépôt impossible");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_repository_ids_are_safe_directory_names() {
        for valid in ["repo.official", "repo.custom_1", "mon-depot"] {
            assert!(is_valid_repository_id(valid), "{valid}");
        }
        for invalid in ["", ".cache", "../evil", "a/b", "a b", "é", &"x".repeat(65)] {
            assert!(!is_valid_repository_id(invalid), "{invalid}");
        }
    }

    #[test]
    fn test_repository_dir_refuses_unsafe_ids() {
        let base = Path::new("/config/plugins");
        assert_eq!(
            repository_dir(base, "repo.official"),
            Some(base.join("repo.official"))
        );
        assert_eq!(repository_dir(base, "../x"), None);
    }

    #[test]
    fn test_has_index_requires_the_catalogue_file() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        assert!(!has_index(dir.path()));
        std::fs::write(dir.path().join("index.json"), "{}")?;
        assert!(has_index(dir.path()));
        Ok(())
    }
}
