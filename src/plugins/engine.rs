//! Dossiers de catalogue que le moteur lit (ADR 0029).

use super::{
    installed::{DamagedItem, frozen_roots},
    sources::{has_index, local_catalog_dir, sources_to_resume},
};
use crate::{
    config::Config,
    error::{AppError, AppResult},
    repository,
};
use sqlx::SqlitePool;
use std::path::PathBuf;

/// Ce que le moteur lit, et ce qui lui manque.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EngineSources {
    /// Dans l'ordre de priorité : `PLUGINS_DIR`, collection locale, copies figées, puis les dépôts qui
    /// restent à reprendre.
    pub roots: Vec<PathBuf>,
    /// Éléments inscrits dont le moteur ne peut pas lire la copie : « à réparer ».
    pub damaged: Vec<DamagedItem>,
    /// La reprise des dépôts est-elle finie ?
    pub resumed: bool,
}

impl EngineSources {
    /// Ces dossiers donnent-ils tous les outils encore utiles ? Sinon, rien ne doit être purgé.
    pub fn is_complete(&self) -> bool {
        self.resumed && self.damaged.is_empty()
    }
}

pub async fn engine_sources(pool: &SqlitePool, config: &Config) -> AppResult<EngineSources> {
    // Lu d'abord : un dépôt n'est retiré de la reprise qu'après l'inscription de ses éléments.
    let pending = repository::pending_store_repositories(pool).await?;
    let installations = repository::list_installations(pool).await?;
    let installed_dir = config.installed_dir();
    let (frozen, damaged) =
        tokio::task::spawn_blocking(move || frozen_roots(&installed_dir, &installations))
            .await
            .map_err(|_| AppError::Internal)?;

    let mut roots: Vec<PathBuf> = config.plugins_dir.iter().cloned().collect();
    roots.extend(Some(local_catalog_dir(config)).filter(|dir| has_index(dir)));
    roots.extend(frozen);
    roots.extend(
        sources_to_resume(pool, config, pending.as_deref())
            .await?
            .into_iter()
            .map(|source| source.root),
    );
    Ok(EngineSources {
        roots,
        damaged,
        resumed: pending.is_some_and(|pending| pending.is_empty()),
    })
}

/// Dossiers du moteur. Une erreur n'est jamais un catalogue vide : l'appelant rend son bail ou échoue.
pub async fn engine_roots(pool: &SqlitePool, config: &Config) -> AppResult<Vec<PathBuf>> {
    Ok(engine_sources(pool, config).await?.roots)
}
