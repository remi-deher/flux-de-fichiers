//! Sources du Store (ADR 0029) : une copie par dépôt synchronisé, plus le dossier de plugins détecté à côté
//! du projet. Le moteur ne les lit plus, sauf pendant la reprise (`engine`).

use super::{
    CatalogCache, PluginRepository, SyncPolicy, SyncReport, SyncedRepository,
    installed::configured_plugins,
    repository_copy::{self, COPY_LOCK},
};
use crate::{
    config::Config,
    error::{AppError, AppResult},
    repository,
};
use forgearr_shared::plugins::store::{DEV_SOURCE_ID, ItemOrigin};
use sqlx::SqlitePool;
use std::path::{Path, PathBuf};

const MAX_REPOSITORY_ID_LEN: usize = 64;
pub(super) const INDEX_RELATIVE_PATH: &str = "index.json";
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

pub(super) fn has_index(dir: &Path) -> bool {
    dir.join(INDEX_RELATIVE_PATH).is_file()
}

/// Dossier du catalogue local géré depuis l'interface (sans dépôt GitHub).
pub fn local_catalog_dir(config: &Config) -> PathBuf {
    config.plugin_repos_dir().join(LOCAL_CATALOG_ID)
}

/// Source du Store dont une copie est sur disque.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoreSource {
    pub origin: ItemOrigin,
    pub root: PathBuf,
}

/// Nom affiché du dossier de plugins détecté.
const DEV_SOURCE_NAME: &str = "Dossier de développement";

/// Dossier de plugins détecté, s'il a un index.
fn detected_source(config: &Config) -> Option<StoreSource> {
    let root = config
        .detected_plugins_dir
        .clone()
        .filter(|dir| has_index(dir))?;
    Some(StoreSource {
        origin: ItemOrigin {
            repository_id: DEV_SOURCE_ID.to_owned(),
            repository_url: root.display().to_string(),
            repository_name: DEV_SOURCE_NAME.to_owned(),
        },
        root,
    })
}

/// Copie synchronisée d'un dépôt, s'il en a une.
fn repository_source(config: &Config, repo: &PluginRepository) -> Option<StoreSource> {
    let root = Some(repo)
        .filter(|repo| owns_copy(&repo.id))
        .and_then(|repo| repository_dir(&config.plugin_repos_dir(), &repo.id))
        .filter(|dir| has_index(dir))?;
    Some(StoreSource {
        origin: ItemOrigin {
            repository_id: repo.id.clone(),
            repository_url: repo.index_url.clone(),
            repository_name: repo.name.clone(),
        },
        root,
    })
}

/// Sources dont l'identifiant est retenu, dans l'ordre de priorité.
async fn sources_where(
    pool: &SqlitePool,
    config: &Config,
    keep: impl Fn(&PluginRepository) -> bool,
) -> AppResult<Vec<StoreSource>> {
    let repositories = repository::list_plugin_repositories(pool).await?;
    Ok(repositories
        .iter()
        .filter(|repo| keep(repo))
        .filter_map(|repo| repository_source(config, repo))
        .collect())
}

/// Sources du Store : le dossier détecté, puis les dépôts activés déjà synchronisés. La collection locale
/// n'en est pas une : tout ce qu'elle contient est déjà lu par le moteur.
pub async fn store_sources(pool: &SqlitePool, config: &Config) -> AppResult<Vec<StoreSource>> {
    let mut sources: Vec<StoreSource> = detected_source(config).into_iter().collect();
    sources.extend(sources_where(pool, config, |repo| repo.enabled).await?);
    Ok(sources)
}

/// Sources que le moteur lit encore : celles qui restent à reprendre et ont une copie, ou toutes tant que la
/// reprise n'a pas commencé. Un dépôt désactivé pendant la reprise y reste.
pub(super) async fn sources_to_resume(
    pool: &SqlitePool,
    config: &Config,
    pending: Option<&[String]>,
) -> AppResult<Vec<StoreSource>> {
    let Some(pending) = pending else {
        return store_sources(pool, config).await;
    };
    if pending.is_empty() {
        return Ok(Vec::new());
    }
    let is_pending = |id: &str| pending.iter().any(|known| known == id);
    let mut sources: Vec<StoreSource> = detected_source(config)
        .filter(|_| is_pending(DEV_SOURCE_ID))
        .into_iter()
        .collect();
    sources.extend(sources_where(pool, config, |repo| is_pending(&repo.id)).await?);
    Ok(sources)
}

/// Sources que le moteur lisait avant l'ADR 0029, copiées ou non : ce que la reprise doit traiter.
pub(super) async fn repositories_to_resume(
    pool: &SqlitePool,
    config: &Config,
) -> AppResult<Vec<String>> {
    let detected = detected_source(config).map(|source| source.origin.repository_id);
    let repositories = repository::list_plugin_repositories(pool).await?;
    Ok(detected
        .into_iter()
        .chain(
            repositories
                .into_iter()
                .filter(|repo| repo.enabled && owns_copy(&repo.id))
                .map(|repo| repo.id),
        )
        .collect())
}

/// Un dépôt à reprendre qui n'a pas de copie lisible peut-il encore en recevoir une ?
pub(super) async fn is_awaited(pool: &SqlitePool, config: &Config, id: &str) -> AppResult<bool> {
    if id == DEV_SOURCE_ID {
        return Ok(config.detected_plugins_dir.is_some());
    }
    match repository::get_plugin_repository(pool, id).await {
        Ok(repo) => Ok(repo.enabled),
        Err(AppError::NotFound) => Ok(false),
        Err(error) => Err(error),
    }
}

/// Dossiers des anciennes routes d'installation, qui cherchent dans le Store par identifiant seul. À retirer
/// avec elles.
pub async fn legacy_store_roots(pool: &SqlitePool, config: &Config) -> AppResult<Vec<PathBuf>> {
    let mut roots: Vec<PathBuf> = config.plugins_dir.iter().cloned().collect();
    let local_dir = local_catalog_dir(config);
    if has_index(&local_dir) {
        roots.push(local_dir);
    }
    roots.extend(
        store_sources(pool, config)
            .await?
            .into_iter()
            .map(|source| source.root),
    );
    Ok(roots)
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
    config: &Config,
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
    remove_copy(&config.plugin_repos_dir(), id).await?;
    let repo =
        repository::add_plugin_repository(pool, cache.changes(), id, name, index_url).await?;
    if let Err(error) = resume_from_if_blank(pool, config, id).await {
        tracing::warn!(repository = %id, ?error, "installation neuve : dépôt ajouté mais pas repris");
    }
    cache.invalidate().await;
    Ok(repo)
}

/// Installation neuve sans aucun dépôt à attendre (l'officiel désactivé, un miroir le remplace) : le socle
/// viendra du dépôt ajouté. Rien n'est installé ni configuré, aucun réglage ne peut donc changer de main.
async fn resume_from_if_blank(pool: &SqlitePool, config: &Config, id: &str) -> AppResult<()> {
    let Some(mut pending) = repository::pending_store_repositories(pool).await? else {
        return Ok(());
    };
    for known in &pending {
        if is_awaited(pool, config, known).await? {
            return Ok(());
        }
    }
    let is_blank = owns_copy(id)
        && sources_to_resume(pool, config, Some(&pending))
            .await?
            .is_empty()
        && repository::list_installations(pool).await?.is_empty()
        && configured_plugins(pool).await?.is_empty();
    if is_blank {
        pending.push(id.to_owned());
        repository::set_pending_store_repositories(pool, &pending).await?;
    }
    Ok(())
}

/// Retire un dépôt et efface sa copie. Un échec de l'effacement est journalisé : sans ligne en base, la
/// copie n'est plus lue. Refusé tant que la reprise n'a pas figé ce qui sert : cette copie en est la seule
/// source.
pub async fn delete_repository(
    pool: &SqlitePool,
    cache: &CatalogCache,
    config: &Config,
    id: &str,
) -> AppResult<()> {
    if is_to_resume(pool, config, id).await? {
        super::resume_store(pool, config, cache, None).await?;
    }
    let _copies = COPY_LOCK.lock().await;
    if is_to_resume(pool, config, id).await? {
        return Err(AppError::StoreMigrating(format!(
            "des éléments du dépôt {id} servent encore et ne sont pas tous repris"
        )));
    }
    repository::delete_plugin_repository(pool, cache.changes(), id).await?;
    if let Err(error) = remove_copy(&config.plugin_repos_dir(), id).await {
        tracing::warn!(repository = %id, %error, "copie du dépôt non effacée");
    }
    cache.invalidate().await;
    Ok(())
}

/// Le moteur lit-il encore la copie de ce dépôt, faute de reprise ?
async fn is_to_resume(pool: &SqlitePool, config: &Config, id: &str) -> AppResult<bool> {
    let pending = repository::pending_store_repositories(pool).await?;
    Ok(sources_to_resume(pool, config, pending.as_deref())
        .await?
        .iter()
        .any(|source| {
            source.origin.repository_id == id && super::load_catalog(&source.root).is_ok()
        }))
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

/// Synchronise un dépôt, vide le cache du catalogue, date la synchronisation, puis poursuit la reprise :
/// seul chemin qui publie une copie de dépôt, il signale donc chaque changement.
pub async fn sync_and_signal(
    pool: &SqlitePool,
    cache: &CatalogCache,
    repo: &PluginRepository,
    config: &Config,
    policy: &SyncPolicy,
) -> AppResult<SyncReport> {
    let repos_dir = config.plugin_repos_dir();
    let report = super::sync_repository_verified(pool, repo, &repos_dir, policy)
        .await
        .map_err(|error| AppError::RepositoryUnavailable(error.to_string()))?;
    cache.invalidate().await;
    repository::touch_plugin_repository_sync(pool, cache.changes(), &repo.id).await?;
    let synced = SyncedRepository {
        id: &repo.id,
        withheld: &report.withheld,
    };
    if let Err(error) = super::resume_store(pool, config, cache, Some(synced)).await {
        tracing::warn!(repository = %repo.id, ?error, "reprise des éléments installés à retenter");
    }
    Ok(report)
}

/// Synchronise chaque dépôt activé ; un échec est journalisé sans interrompre les autres.
pub async fn sync_enabled_repositories(pool: &SqlitePool, config: &Config, cache: &CatalogCache) {
    let Ok(repositories) = repository::list_plugin_repositories(pool).await else {
        return;
    };
    let policy = crate::runtime_settings::load_effective(pool, config)
        .await
        .sync_policy();
    for repo in repositories.into_iter().filter(|repo| repo.enabled) {
        match sync_and_signal(pool, cache, &repo, config, &policy).await {
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

    /// Configuration dont les dépôts sont copiés dans `<config>/plugins`.
    async fn state_in(dir: &Path) -> anyhow::Result<crate::api::AppState> {
        Ok(crate::api::transfers::tests::setup(dir).await?.0)
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
        let dir = tempfile::tempdir()?;
        let state = state_in(dir.path()).await?;
        let (pool, cache, config) = (&state.pool, &state.catalog_cache, &state.config);
        let repos_dir = config.plugin_repos_dir();

        // Reste d'un ancien dépôt de même identifiant.
        let copy = write_copy(&repos_dir, "repo.test")?;
        add_repository(pool, cache, config, "repo.test", "Test", URL).await?;
        assert!(!copy.exists());

        // Un ajout refusé (identifiant déjà pris) n'efface pas la copie du dépôt en place.
        write_copy(&repos_dir, "repo.test")?;
        let duplicate = add_repository(pool, cache, config, "repo.test", "Test", URL).await;
        assert!(matches!(duplicate, Err(AppError::Conflict)));
        assert!(copy.exists());

        let other = write_copy(&repos_dir, "repo.autre")?;
        delete_repository(pool, cache, config, "repo.test").await?;
        assert!(!copy.exists());
        assert!(other.exists());

        // Une suppression refusée ne touche pas au disque.
        let unknown = delete_repository(pool, cache, config, "repo.autre").await;
        assert!(matches!(unknown, Err(AppError::NotFound)));
        assert!(other.exists());
        Ok(())
    }

    #[tokio::test]
    async fn a_mirror_replaces_the_disabled_official_repository_of_a_blank_installation()
    -> anyhow::Result<()> {
        const OFFICIAL: &str = "repo.official";
        const URL: &str = "https://example.org/index.json";
        let dir = tempfile::tempdir()?;
        let state = state_in(dir.path()).await?;
        let (pool, cache, config) = (&state.pool, &state.catalog_cache, &state.config);
        let pending = [OFFICIAL.to_owned()];
        repository::set_pending_store_repositories(pool, &pending).await?;
        // Tant que l'officiel peut arriver, un autre dépôt n'est qu'une vitrine.
        add_repository(pool, cache, config, "repo.tiers", "Tiers", URL).await?;
        let listed = repository::pending_store_repositories(pool).await?;
        assert_eq!(listed.as_deref(), Some(pending.as_slice()));

        repository::toggle_plugin_repository(pool, cache.changes(), OFFICIAL).await?;
        add_repository(pool, cache, config, "repo.miroir", "Miroir", URL).await?;
        let listed = repository::pending_store_repositories(pool).await?;
        assert_eq!(
            listed,
            Some(vec![OFFICIAL.to_owned(), "repo.miroir".to_owned()])
        );

        // Un dépôt retiré n'est plus à reprendre, même rajouté.
        delete_repository(pool, cache, config, "repo.miroir").await?;
        let listed = repository::pending_store_repositories(pool).await?;
        assert_eq!(listed.as_deref(), Some(pending.as_slice()));
        Ok(())
    }

    #[tokio::test]
    async fn a_repository_whose_old_copy_cannot_be_removed_is_not_added() -> anyhow::Result<()> {
        const URL: &str = "https://example.org/index.json";
        let dir = tempfile::tempdir()?;
        let state = state_in(dir.path()).await?;
        let (pool, cache, config) = (&state.pool, &state.catalog_cache, &state.config);
        // Un fichier à la place du dossier : son effacement échoue.
        let repos_dir = config.plugin_repos_dir();
        std::fs::create_dir_all(&repos_dir)?;
        std::fs::write(repos_dir.join("repo.test"), "x")?;
        let refused = add_repository(pool, cache, config, "repo.test", "Test", URL).await;
        assert!(matches!(refused, Err(AppError::Storage(_))));
        let unknown = repository::get_plugin_repository(pool, "repo.test").await;
        assert!(matches!(unknown, Err(AppError::NotFound)));
        Ok(())
    }

    #[tokio::test]
    async fn the_recovery_leaves_the_leftovers_of_the_local_library_alone() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let state = state_in(dir.path()).await?;
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
        let state = state_in(dir.path()).await?;
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
        let dir = tempfile::tempdir()?;
        let state = state_in(dir.path()).await?;
        let (pool, cache, config) = (&state.pool, &state.catalog_cache, &state.config);
        let library = write_copy(&config.plugin_repos_dir(), LOCAL_CATALOG_ID)?;
        let url = "https://example.org/index.json";
        repository::add_plugin_repository(pool, cache.changes(), LOCAL_CATALOG_ID, "Local", url)
            .await?;
        delete_repository(pool, cache, config, LOCAL_CATALOG_ID).await?;
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
