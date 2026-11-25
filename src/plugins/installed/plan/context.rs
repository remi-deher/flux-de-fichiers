//! Ce que le plan compare : les sources du Store, ce que le moteur lit et ce qui s'en sert.

use crate::{
    config::Config,
    error::AppResult,
    plugins::{
        CatalogCache, CatalogItemType, engine_sources,
        installed::{
            EngineCatalog, ItemContent, UsageSources, files::read_published_item, installed_root,
            read_item, script_copy::script_copy_dir,
        },
        local_catalog_dir, store_sources, tool_version,
    },
    repository::{self, InstallationRecord, SavedSettings},
    tools_cache::ToolCacheManager,
};
use forgearr_shared::plugins::{
    CatalogEntry,
    store::{DEV_SOURCE_ID, ItemOrigin, LOCAL_SOURCE_ID},
};
use sqlx::SqlitePool;
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};

/// Origines affichées des éléments que le moteur lit sans qu'ils soient installés.
const LOCAL_SOURCE_NAME: &str = "la collection locale";
const PLUGINS_DIR_NAME: &str = "le dossier PLUGINS_DIR";

/// Une source et ce qu'elle porte.
pub(super) struct StoreCatalog {
    pub origin: ItemOrigin,
    pub root: PathBuf,
    pub catalog: EngineCatalog,
}

/// Où en est un identifiant dans le moteur.
pub(super) enum Standing<'a> {
    Absent,
    Installed {
        record: &'a InstallationRecord,
        /// Version que le moteur lit.
        version: String,
        /// « À réparer » : sa copie n'est plus lisible.
        is_damaged: bool,
    },
    /// Lu depuis `PLUGINS_DIR` ou la collection locale.
    Provided {
        origin: ItemOrigin,
        version: String,
    },
}

/// Tout ce dont un plan dépend, lu une fois. Les méthodes qui s'en servent lisent le disque : bloquantes, et sans
/// effet.
pub struct PlanContext {
    /// Dans l'ordre de priorité.
    pub(super) sources: Vec<StoreCatalog>,
    /// Collection locale : seuls ses packs sont des cibles.
    pub(super) local: StoreCatalog,
    pub(super) engine: EngineCatalog,
    pub(super) usage: UsageSources,
    pub(super) settings: HashMap<String, SavedSettings>,
    pub(super) tools_cache: ToolCacheManager,
    /// Seules des publications immuables sont acceptées.
    pub(super) requires_immutable: bool,
    /// L'appelant peut-il créer un flux ?
    pub(super) may_manage_flows: bool,
    installed: HashMap<String, InstallationRecord>,
    damaged: HashSet<String>,
    installed_dir: PathBuf,
    plugins_dir: Option<PathBuf>,
}

impl PlanContext {
    pub async fn read(
        pool: &SqlitePool,
        config: &Config,
        cache: &CatalogCache,
        may_manage_flows: bool,
    ) -> AppResult<Self> {
        let local_dir = local_catalog_dir(config);
        let engine = engine_sources(pool, config).await?;
        let mut sources = Vec::new();
        for source in store_sources(pool, config).await? {
            sources.push(store_catalog(cache, source.origin, source.root).await);
        }
        let local =
            store_catalog(cache, provided_origin(&local_dir, true), local_dir.clone()).await;
        let installed = repository::list_installations(pool).await?;
        let policy = crate::runtime_settings::load_effective(pool, config)
            .await
            .sync_policy();
        let mut conn = pool.acquire().await?;
        let usage = UsageSources::read(&mut conn, &local_dir).await?;
        let settings = repository::list_saved_settings(&mut conn).await?;
        Ok(Self {
            sources,
            local,
            engine: EngineCatalog::cached(cache, &engine.roots).await,
            usage,
            settings: settings
                .into_iter()
                .map(|saved| (saved.plugin_id.clone(), saved))
                .collect(),
            tools_cache: ToolCacheManager::new(&config.tools_cache_dir),
            requires_immutable: policy.require_immutable_releases,
            may_manage_flows,
            installed: installed
                .into_iter()
                .map(|item| (item.record.item_id.clone(), item.record))
                .collect(),
            damaged: engine.damaged.into_iter().map(|item| item.id).collect(),
            installed_dir: config.installed_dir(),
            plugins_dir: config.plugins_dir.clone(),
        })
    }

    pub(super) fn local_dir(&self) -> &Path {
        &self.local.root
    }

    pub(super) fn record(&self, id: &str) -> Option<&InstallationRecord> {
        self.installed.get(id)
    }

    pub(super) fn standing(&self, id: &str) -> Standing<'_> {
        if let Some(record) = self.record(id) {
            return Standing::Installed {
                record,
                version: self
                    .engine_version(id)
                    .unwrap_or_else(|| record.version.clone()),
                is_damaged: self.is_damaged(record),
            };
        }
        let provided = self.engine.located(id).filter(|(root, entry)| {
            entry.entry_type.is_installed_by_itself() && self.provides(root)
        });
        match provided {
            Some((root, entry)) => Standing::Provided {
                origin: provided_origin(root, root == self.local_dir()),
                version: effective_version(root, entry),
            },
            None => Standing::Absent,
        }
    }

    /// Version que le moteur lit ; `None` s'il n'a pas cet élément.
    fn engine_version(&self, id: &str) -> Option<String> {
        self.engine
            .located(id)
            .map(|(root, entry)| effective_version(root, entry))
    }

    fn provides(&self, root: &Path) -> bool {
        root == self.local_dir() || self.plugins_dir.as_deref() == Some(root)
    }

    fn is_damaged(&self, record: &InstallationRecord) -> bool {
        let copy_is_lost = || {
            record.kind == CatalogItemType::Script
                && matches!(
                    script_copy_dir(self.local_dir(), &record.item_id).try_exists(),
                    Ok(false)
                )
        };
        self.damaged.contains(&record.item_id) || copy_is_lost()
    }

    pub(super) fn frozen_root(&self, record: &InstallationRecord) -> Option<PathBuf> {
        installed_root(&self.installed_dir, record)
    }

    /// Contenu figé d'un élément inscrit ; `None` s'il est absent ou altéré.
    pub(super) fn frozen_content(&self, record: &InstallationRecord) -> Option<ItemContent> {
        read_item(&self.frozen_root(record)?, &record.item_id)
            .ok()
            .filter(|content| content.content_sha256 == record.content_sha256)
    }

    /// Contenu qu'une source publie.
    pub(super) fn published_content(&self, source: usize, id: &str) -> AppResult<ItemContent> {
        read_published_item(&self.sources[source].root, id, self.requires_immutable)
    }

    pub(super) fn published_version(&self, source: usize, id: &str) -> Option<String> {
        let source = &self.sources[source];
        source
            .catalog
            .entry(id)
            .map(|entry| effective_version(&source.root, entry))
    }

    pub(super) fn source_named(&self, repository_id: &str) -> Option<usize> {
        self.sources
            .iter()
            .position(|source| source.origin.repository_id == repository_id)
    }

    /// Source de cette origine : même identifiant, même adresse.
    pub(super) fn source_of(&self, origin: &ItemOrigin) -> Option<usize> {
        self.sources
            .iter()
            .position(|source| source.origin.is_same_as(origin))
    }

    /// Sources, celle-ci d'abord.
    fn sources_from(&self, preferred: Option<usize>) -> impl Iterator<Item = usize> {
        let others = (0..self.sources.len()).filter(move |index| Some(*index) != preferred);
        preferred.into_iter().chain(others)
    }

    /// Sources qui portent cet élément, la préférée d'abord.
    pub(super) fn candidates(&self, id: &str, preferred: Option<usize>) -> Vec<usize> {
        self.sources_from(preferred)
            .filter(|index| {
                let entry = self.sources[*index].catalog.entry(id);
                entry.is_some_and(|entry| entry.entry_type.is_installed_by_itself())
            })
            .collect()
    }

    /// Ce que cherche `find` dans les sources, la préférée d'abord.
    pub(super) fn find_in_sources<T>(
        &self,
        preferred: Option<usize>,
        find: impl Fn(&EngineCatalog) -> Option<T>,
    ) -> Option<T> {
        self.sources_from(preferred)
            .find_map(|index| find(&self.sources[index].catalog))
    }

    /// Ce que cherche `find` dans le moteur, sinon dans les sources.
    pub(super) fn find<T>(
        &self,
        preferred: Option<usize>,
        find: impl Fn(&EngineCatalog) -> Option<T>,
    ) -> Option<T> {
        find(&self.engine).or_else(|| self.find_in_sources(preferred, find))
    }
}

async fn store_catalog(cache: &CatalogCache, origin: ItemOrigin, root: PathBuf) -> StoreCatalog {
    let catalog = EngineCatalog::cached(cache, std::slice::from_ref(&root)).await;
    StoreCatalog {
        origin,
        root,
        catalog,
    }
}

fn provided_origin(root: &Path, is_local: bool) -> ItemOrigin {
    let (id, name) = if is_local {
        (LOCAL_SOURCE_ID, LOCAL_SOURCE_NAME)
    } else {
        (DEV_SOURCE_ID, PLUGINS_DIR_NAME)
    };
    ItemOrigin {
        repository_id: id.to_owned(),
        repository_url: root.display().to_string(),
        repository_name: name.to_owned(),
    }
}

/// Version qu'une exigence compare : celle du contrat pour un outil, car c'est elle que reçoit un bail.
fn effective_version(root: &Path, entry: &CatalogEntry) -> String {
    match entry.entry_type {
        CatalogItemType::App => {
            tool_version(&[root.to_path_buf()], &entry.id, Some(&entry.version))
        }
        _ => entry.version.clone(),
    }
}
