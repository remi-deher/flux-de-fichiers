//! Dossiers de catalogue : un dossier par dépôt synchronisé, plus un dossier local de développement.

use crate::{config::Config, repository};
use sqlx::SqlitePool;
use std::path::{Path, PathBuf};

const MAX_REPOSITORY_ID_LEN: usize = 64;
const INDEX_RELATIVE_PATH: [&str; 2] = ["catalogue", "index.json"];

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
    INDEX_RELATIVE_PATH
        .iter()
        .fold(dir.to_path_buf(), |path, part| path.join(part))
        .is_file()
}

/// Dossiers de catalogue à lire, dans l'ordre de priorité : le dossier local de développement,
/// puis les dépôts activés déjà synchronisés (officiel en premier).
pub async fn catalog_roots(pool: &SqlitePool, config: &Config) -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = config.plugins_dir.iter().cloned().collect();
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

/// Synchronise chaque dépôt activé ; un échec est journalisé sans interrompre les autres.
pub async fn sync_enabled_repositories(pool: &SqlitePool, config: &Config) {
    let Ok(repositories) = repository::list_plugin_repositories(pool).await else {
        return;
    };
    let repos_dir = config.plugin_repos_dir();
    let policy = super::SyncPolicy::from_env();
    for repo in repositories.into_iter().filter(|repo| repo.enabled) {
        match super::sync_repository(&repo, &repos_dir, &policy).await {
            Ok(report) => {
                tracing::info!(repository = %repo.id, entries = report.entries, files = report.files, "dépôt de plugins synchronisé");
                let _ = repository::touch_plugin_repository_sync(pool, &repo.id).await;
            }
            Err(error) => {
                tracing::warn!(repository = %repo.id, error = %error, "synchronisation du dépôt impossible");
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
        std::fs::create_dir_all(dir.path().join("catalogue"))?;
        std::fs::write(dir.path().join("catalogue/index.json"), "{}")?;
        assert!(has_index(dir.path()));
        Ok(())
    }
}
