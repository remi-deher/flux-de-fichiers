//! Gel et inscription d'un élément pendant la reprise des dépôts (ADR 0029). Rejouable : ce que le moteur lit
//! déjà n'est plus touché.

use super::{
    InstallationGuard, ItemContent, freeze_item, installed_root,
    integrity::is_readable,
    read_item,
    script_copy::{check_script_copy, write_script_copy},
    usage::serves_every_task,
};
use crate::{
    config::Config,
    error::{AppError, AppResult},
    plugins::{CatalogItemType, StoreSource, local_catalog_dir},
    repository::{self, InstallationRecord},
};
use forgearr_shared::plugins::store::ItemOrigin;
use sqlx::SqlitePool;

/// Élément que le moteur ne lit pas encore depuis une copie figée.
pub(super) struct Candidate {
    content: ItemContent,
    /// Origine de son inscription, si elle porte ce même contenu : seule sa copie est à refaire.
    recorded: Option<ItemOrigin>,
}

impl Candidate {
    pub fn id(&self) -> &str {
        &self.content.entry.id
    }

    pub fn is_recorded(&self) -> bool {
        self.recorded.is_some()
    }

    /// Fichiers que le moteur lit et que la source n'a pas ; déjà admis pour un contenu inscrit.
    pub fn missing(&self) -> &[String] {
        if self.is_recorded() {
            &[]
        } else {
            &self.content.missing
        }
    }
}

/// Lit un élément dans sa source ; `None` si le moteur le lit déjà depuis sa copie figée.
pub(super) async fn prepare(
    pool: &SqlitePool,
    config: &Config,
    source: &StoreSource,
    item_id: &str,
) -> AppResult<Option<Candidate>> {
    let known = {
        let mut conn = pool.acquire().await?;
        repository::get_installation(&mut conn, item_id).await?
    }
    .map(|item| item.record);
    if let Some(known) = &known
        && is_in_engine(config, known).await?
    {
        return Ok(None);
    }
    let (root, id) = (source.root.clone(), item_id.to_owned());
    let content = tokio::task::spawn_blocking(move || read_item(&root, &id))
        .await
        .map_err(|_| AppError::Internal)??;
    let recorded = match known {
        Some(known) if known.content_sha256 == content.content_sha256 => Some(known.origin),
        Some(known) if !known.origin.is_same_as(&source.origin) => {
            return Err(AppError::InvalidManifest(format!(
                "{item_id} : copie figée illisible, et un autre dépôt publie un autre contenu"
            )));
        }
        Some(_) => {
            // Le moteur lit déjà ce contenu dans le dépôt, qui reste à reprendre.
            tracing::warn!(item = %item_id, "copie figée illisible : reprise du contenu que son dépôt publie maintenant");
            None
        }
        None => None,
    };
    Ok(Some(Candidate { content, recorded }))
}

/// Fige un élément et l'inscrit.
pub(super) async fn adopt(
    lock: &InstallationGuard,
    pool: &SqlitePool,
    config: &Config,
    source: &StoreSource,
    candidate: Candidate,
) -> AppResult<()> {
    let Candidate { content, recorded } = candidate;
    let is_script = content.entry.entry_type == CatalogItemType::Script;
    let is_recorded = recorded.is_some();
    let origin = recorded.unwrap_or_else(|| source.origin.clone());
    let mut record = InstallationRecord {
        item_id: content.entry.id.clone(),
        kind: content.entry.entry_type,
        version: content.entry.version.clone(),
        content_sha256: content.content_sha256.clone(),
        origin: origin.clone(),
        copy_id: is_script.then(|| content.entry.id.clone()),
        copy_pending: is_script,
        serves_every_task: false,
    };
    let installed_dir = config.installed_dir();
    let frozen = freeze_item(lock, &installed_dir, content, &origin).await?;
    if !is_recorded {
        let (item_id, local_dir) = (record.item_id.clone(), local_catalog_dir(config));
        record.serves_every_task = tokio::task::spawn_blocking(move || {
            if is_script {
                // Avant d'inscrire : une ligne sans copie possible resterait à réparer.
                check_script_copy(&installed_dir, &frozen, &local_dir, &item_id)?;
            }
            AppResult::Ok(serves_every_task(&frozen))
        })
        .await
        .map_err(|_| AppError::Internal)??;
        let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
        repository::save_installation(&mut tx, &record).await?;
        // Les réglages en place servaient au plugin de ce dépôt.
        repository::set_settings_origin(&mut tx, &record.item_id, &record.origin).await?;
        tx.commit().await?;
    }
    if is_script {
        finish_script_copy(lock, pool, config, &record).await?;
    }
    Ok(())
}

/// Le moteur lit-il cet élément inscrit ?
async fn is_in_engine(config: &Config, record: &InstallationRecord) -> AppResult<bool> {
    if record.kind == CatalogItemType::Script {
        return Ok(!record.copy_pending);
    }
    let Some(root) = installed_root(&config.installed_dir(), record) else {
        return Ok(false);
    };
    let record = record.clone();
    tokio::task::spawn_blocking(move || is_readable(&root, &record))
        .await
        .map_err(|_| AppError::Internal)
}

/// Écrit les copies modifiables qu'un arrêt a interrompues ; un échec est journalisé.
pub(super) async fn finish_script_copies(
    lock: &InstallationGuard,
    pool: &SqlitePool,
    config: &Config,
) -> AppResult<bool> {
    let mut changed = false;
    for item in repository::list_installations(pool).await? {
        let record = item.record;
        if !record.copy_pending || record.kind != CatalogItemType::Script {
            continue;
        }
        match finish_script_copy(lock, pool, config, &record).await {
            Ok(()) => changed = true,
            Err(error) => {
                tracing::warn!(item = %record.item_id, ?error, "copie modifiable du script à réécrire");
            }
        }
    }
    Ok(changed)
}

pub(super) async fn finish_script_copy(
    _lock: &InstallationGuard,
    pool: &SqlitePool,
    config: &Config,
    record: &InstallationRecord,
) -> AppResult<()> {
    let frozen = installed_root(&config.installed_dir(), record).ok_or(AppError::Invalid)?;
    let (local_dir, item_id) = (local_catalog_dir(config), record.item_id.clone());
    tokio::task::spawn_blocking(move || write_script_copy(&frozen, &local_dir, &item_id))
        .await
        .map_err(|_| AppError::Internal)??;
    let mut conn = pool.acquire().await?;
    repository::finish_installation_copy(&mut conn, &record.item_id, &record.content_sha256)
        .await?;
    Ok(())
}
