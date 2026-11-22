//! Dossiers de catalogue : un dossier par dépôt synchronisé, plus un dossier local de développement.

use super::{
    CatalogCache, PluginRepository, SyncPolicy, SyncReport,
    repository_copy::{self, COPY_LOCK},
};
use crate::{
    config::Config,
    error::{AppError, AppResult},
    repository,
};
use sqlx::SqlitePool;
use std::path::{Path, PathBuf};

const MAX_REPOSITORY_ID_LEN: usize = 64;
const INDEX_RELATIVE_PATH: &str = "index.json";
/// Dossier de la librairie locale, à côté des copies de dépôts.
const LOCAL_CATALOG_ID: &str = "local";
/// Identifiants qu'un nouveau dépôt ne peut pas prendre : ils désignent d'autres sources de pièces.
const RESERVED_REPOSITORY_IDS: [&str; 3] = [LOCAL_CATALOG_ID, "dev", "installed"];

/// Un identifiant de dépôt sert de nom de dossier : lettres, chiffres, `.`, `-` et `_` seulement.
pub fn is_valid_repository_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAX_REPOSITORY_ID_LEN
        && !id.starts_with('.')
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
}

/// Identifiant refusé à l'ajout d'un dépôt. Un dépôt plus ancien nommé `dev` ou `installed` reste
/// utilisable ; nommé `local`, il n'est plus synchronisé, car sa copie remplacerait la librairie locale.
pub fn is_reserved_repository_id(id: &str) -> bool {
    RESERVED_REPOSITORY_IDS
        .iter()
        .any(|reserved| reserved.eq_ignore_ascii_case(id))
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
    config.plugin_repos_dir().join(LOCAL_CATALOG_ID)
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

/// L'identifiant désigne-t-il une copie de dépôt sur disque, et non la librairie locale ?
pub(super) fn owns_copy(repository_id: &str) -> bool {
    repository_id != LOCAL_CATALOG_ID && is_valid_repository_id(repository_id)
}

/// Efface la copie d'un dépôt, jamais la librairie locale.
async fn remove_copy(repos_dir: &Path, repository_id: &str) -> std::io::Result<()> {
    if owns_copy(repository_id) {
        repository_copy::remove(repos_dir, repository_id).await?;
    }
    Ok(())
}

/// Ajoute un dépôt. La copie qu'un ancien dépôt de même identifiant aurait laissée est effacée : elle
/// peut venir d'une autre adresse. Elle l'est avant l'ajout, qui échoue si elle ne peut pas l'être : une
/// fois le dépôt inscrit, elle serait lue.
pub async fn add_repository(
    pool: &SqlitePool,
    cache: &CatalogCache,
    repos_dir: &Path,
    id: &str,
    name: &str,
    index_url: &str,
) -> AppResult<PluginRepository> {
    let _copies = COPY_LOCK.lock().await;
    // Un doublon refusé n'efface pas la copie du dépôt en place ; le verrou exclut tout autre ajout.
    match repository::get_plugin_repository(pool, id).await {
        Ok(_) => return Err(AppError::Conflict),
        Err(AppError::NotFound) => {}
        Err(error) => return Err(error),
    }
    remove_copy(repos_dir, id).await?;
    let repo =
        repository::add_plugin_repository(pool, cache.changes(), id, name, index_url).await?;
    cache.invalidate().await;
    Ok(repo)
}

/// Retire un dépôt et efface sa copie. Un échec de l'effacement est journalisé : sans ligne en base, la
/// copie n'est plus lue.
pub async fn delete_repository(
    pool: &SqlitePool,
    cache: &CatalogCache,
    repos_dir: &Path,
    id: &str,
) -> AppResult<()> {
    let _copies = COPY_LOCK.lock().await;
    repository::delete_plugin_repository(pool, cache.changes(), id).await?;
    if let Err(error) = remove_copy(repos_dir, id).await {
        tracing::warn!(repository = %id, %error, "copie du dépôt non effacée");
    }
    cache.invalidate().await;
    Ok(())
}

/// Démarrage du central : remet en place les copies qu'un arrêt brutal a laissées de côté entre les
/// deux renommages d'une publication. À appeler avant toute lecture du catalogue.
pub async fn recover_repository_copies(pool: &SqlitePool, config: &Config) {
    let repositories = match repository::list_plugin_repositories(pool).await {
        Ok(repositories) => repositories,
        Err(error) => {
            tracing::warn!(%error, "copies de dépôts non vérifiées");
            return;
        }
    };
    let repos_dir = config.plugin_repos_dir();
    let _copies = COPY_LOCK.lock().await;
    for repo in repositories.iter().filter(|repo| owns_copy(&repo.id)) {
        match repository_copy::recover(&repos_dir, &repo.id).await {
            Ok(false) => {}
            Ok(true) => {
                tracing::info!(repository = %repo.id, "copie du dépôt remise en place après un arrêt brutal");
            }
            Err(error) => {
                tracing::warn!(repository = %repo.id, %error, "reprise de la copie du dépôt impossible");
            }
        }
    }
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
    fn reserved_ids_are_recognised_whatever_their_case() {
        for reserved in ["local", "dev", "installed", "Local"] {
            assert!(is_reserved_repository_id(reserved), "{reserved}");
        }
        for free in ["repo.official", "local.2", "devs"] {
            assert!(!is_reserved_repository_id(free), "{free}");
        }
    }

    async fn empty_pool() -> anyhow::Result<SqlitePool> {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await?;
        sqlx::migrate!().run(&pool).await?;
        Ok(pool)
    }

    fn write_copy(repos_dir: &Path, name: &str) -> std::io::Result<PathBuf> {
        let copy = repos_dir.join(name);
        std::fs::create_dir_all(&copy)?;
        std::fs::write(copy.join(INDEX_RELATIVE_PATH), "{}")?;
        Ok(copy)
    }

    #[tokio::test]
    async fn adding_then_deleting_a_repository_leaves_no_copy_behind() -> anyhow::Result<()> {
        const URL: &str = "https://example.org/index.json";
        let pool = empty_pool().await?;
        let cache = CatalogCache::default();
        let dir = tempfile::tempdir()?;
        let repos_dir = dir.path();

        // Reste d'un ancien dépôt de même identifiant.
        let copy = write_copy(repos_dir, "repo.test")?;
        add_repository(&pool, &cache, repos_dir, "repo.test", "Test", URL).await?;
        assert!(!copy.exists());

        // Un ajout refusé (identifiant déjà pris) n'efface pas la copie du dépôt en place.
        write_copy(repos_dir, "repo.test")?;
        let duplicate = add_repository(&pool, &cache, repos_dir, "repo.test", "Test", URL).await;
        assert!(matches!(duplicate, Err(AppError::Conflict)));
        assert!(copy.exists());

        let other = write_copy(repos_dir, "repo.autre")?;
        delete_repository(&pool, &cache, repos_dir, "repo.test").await?;
        assert!(!copy.exists());
        assert!(other.exists());

        // Une suppression refusée ne touche pas au disque.
        let unknown = delete_repository(&pool, &cache, repos_dir, "repo.autre").await;
        assert!(matches!(unknown, Err(AppError::NotFound)));
        assert!(other.exists());
        Ok(())
    }

    #[tokio::test]
    async fn a_repository_whose_old_copy_cannot_be_removed_is_not_added() -> anyhow::Result<()> {
        const URL: &str = "https://example.org/index.json";
        let pool = empty_pool().await?;
        let cache = CatalogCache::default();
        let dir = tempfile::tempdir()?;
        // Un fichier à la place du dossier : son effacement échoue.
        std::fs::write(dir.path().join("repo.test"), "x")?;
        let refused = add_repository(&pool, &cache, dir.path(), "repo.test", "Test", URL).await;
        assert!(matches!(refused, Err(AppError::Storage(_))));
        let unknown = repository::get_plugin_repository(&pool, "repo.test").await;
        assert!(matches!(unknown, Err(AppError::NotFound)));
        Ok(())
    }

    #[tokio::test]
    async fn the_recovery_leaves_the_leftovers_of_the_local_library_alone() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let (state, _, _, _) = crate::api::transfers::tests::setup(dir.path()).await?;
        let repos_dir = state.config.plugin_repos_dir();
        let url = "https://example.org/index.json";
        repository::add_plugin_repository(&state.pool, &state.changes, LOCAL_CATALOG_ID, "L", url)
            .await?;
        let unique = uuid::Uuid::new_v4().simple();
        let library = write_copy(&repos_dir, &format!(".{LOCAL_CATALOG_ID}.old-{unique}"))?;
        recover_repository_copies(&state.pool, &state.config).await;
        assert!(has_index(&library));
        Ok(())
    }

    #[tokio::test]
    async fn interrupted_publications_of_known_repositories_are_undone_at_startup()
    -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let (state, _, _, _) = crate::api::transfers::tests::setup(dir.path()).await?;
        let repos_dir = state.config.plugin_repos_dir();
        let unique = uuid::Uuid::new_v4().simple();
        write_copy(&repos_dir, &format!(".repo.official.old-{unique}"))?;
        let unknown = write_copy(&repos_dir, &format!(".repo.inconnu.old-{unique}"))?;
        recover_repository_copies(&state.pool, &state.config).await;
        assert!(has_index(&repos_dir.join("repo.official")));
        assert!(unknown.exists() && !repos_dir.join("repo.inconnu").exists());
        Ok(())
    }

    #[tokio::test]
    async fn deleting_an_old_repository_named_local_keeps_the_local_library() -> anyhow::Result<()>
    {
        let pool = empty_pool().await?;
        let cache = CatalogCache::default();
        let dir = tempfile::tempdir()?;
        let library = write_copy(dir.path(), LOCAL_CATALOG_ID)?;
        let url = "https://example.org/index.json";
        repository::add_plugin_repository(&pool, cache.changes(), LOCAL_CATALOG_ID, "Local", url)
            .await?;
        delete_repository(&pool, &cache, dir.path(), LOCAL_CATALOG_ID).await?;
        assert!(library.join(INDEX_RELATIVE_PATH).is_file());
        Ok(())
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
