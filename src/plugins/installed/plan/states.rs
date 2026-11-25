//! Mises à jour, éléments à réparer, et état d'installation des éléments du Store.

use super::context::{PlanContext, Standing};
use crate::{
    error::{AppError, AppResult},
    plugins::{CatalogItemType, installed::ItemContent, read_pack_installation},
    repository::InstallationRecord,
};
use forgearr_shared::{
    plugins::{
        CatalogEntry,
        store::{AvailableUpdate, InstallState, ItemRef},
    },
    version::{is_newer_version, satisfies},
};
use std::path::Path;

/// Fichiers que le moteur lit et que la source n'a pas. Les opérations n'en sont pas : la synchronisation en écarte,
/// et l'élément sert sans elles ; pas plus que ce qui manquait déjà à l'élément installé.
pub(super) fn blocking_files(
    published: &ItemContent,
    installed: Option<&ItemContent>,
) -> Vec<String> {
    let is_tolerated = |path: &String| {
        published.missing_operations.contains(path)
            || installed.is_some_and(|installed| installed.missing.contains(path))
    };
    published
        .missing
        .iter()
        .filter(|path| !is_tolerated(path))
        .cloned()
        .collect()
}

/// Mise à jour que son dépôt d'origine publie : version plus récente, ou même version et contenu différent.
/// Aucune tant que la copie du dépôt est incomplète ; seules les publications immuables changent alors de version.
pub(super) fn available_update(
    record: &InstallationRecord,
    published: &ItemContent,
    installed: Option<&ItemContent>,
    requires_immutable: bool,
) -> Option<AvailableUpdate> {
    if !blocking_files(published, installed).is_empty() {
        return None;
    }
    let version = &published.entry.version;
    let content_only = *version == record.version;
    let is_update = if content_only {
        !requires_immutable && published.content_sha256 != record.content_sha256
    } else {
        is_newer_version(&record.version, version)
    };
    is_update.then(|| AvailableUpdate {
        version: version.clone(),
        content_only,
    })
}

/// Un élément inscrit, face à son dépôt d'origine.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstalledState {
    /// Le dépôt d'origine est enregistré, activé et synchronisé.
    pub origin_available: bool,
    pub update: Option<AvailableUpdate>,
    /// « À réparer ».
    pub damaged: bool,
}

impl PlanContext {
    /// État d'un élément inscrit ; `None` s'il ne l'est pas. Bloquant.
    pub fn installed_state(&self, item_id: &str) -> Option<InstalledState> {
        let Standing::Installed {
            record, is_damaged, ..
        } = self.standing(item_id)
        else {
            return None;
        };
        let source = self.source_of(&record.origin);
        Some(InstalledState {
            origin_available: source.is_some(),
            update: source.and_then(|source| self.update_from(source, record)),
            damaged: is_damaged,
        })
    }

    /// Ce que le Store peut faire de cet élément ; `NotFound` s'il n'est dans aucune source. Bloquant.
    pub fn install_state(&self, item: &ItemRef) -> AppResult<InstallState> {
        let source = self
            .source_named(&item.repository_id)
            .ok_or(AppError::NotFound)?;
        let store = &self.sources[source];
        let entry = store.catalog.entry(&item.id).ok_or(AppError::NotFound)?;
        if entry.entry_type == CatalogItemType::Pack {
            return Ok(self.pack_state(&store.root, entry));
        }
        Ok(match self.standing(&item.id) {
            Standing::Installed {
                record,
                version,
                is_damaged,
            } if record.origin.is_same_as(&store.origin) => InstallState::Installed {
                version,
                update: self.update_from(source, record),
                damaged: is_damaged,
            },
            Standing::Installed { record, .. } => InstallState::HeldByOther {
                origin: record.origin.clone(),
            },
            Standing::Provided { .. } => InstallState::Provided,
            Standing::Absent => {
                let missing = self
                    .published_content(source, &item.id)
                    .map(|content| blocking_files(&content, None))
                    .unwrap_or_default();
                if missing.is_empty() {
                    InstallState::Available
                } else {
                    InstallState::Incomplete { missing }
                }
            }
        })
    }

    fn update_from(&self, source: usize, record: &InstallationRecord) -> Option<AvailableUpdate> {
        let published = self.published_content(source, &record.item_id).ok()?;
        let installed = self.frozen_content(record);
        available_update(
            record,
            &published,
            installed.as_ref(),
            self.requires_immutable,
        )
    }

    /// L'état d'un pack est celui de ses composants.
    fn pack_state(&self, root: &Path, entry: &CatalogEntry) -> InstallState {
        let components = read_pack_installation(&[root.to_path_buf()], &entry.id)
            .map(|pack| pack.components)
            .unwrap_or_default();
        let components: Vec<_> = components
            .iter()
            .filter(|component| component.kind.is_installed_by_itself())
            .collect();
        let remaining = components
            .iter()
            .filter(|component| match self.standing(&component.id) {
                Standing::Absent => true,
                Standing::Installed {
                    version,
                    is_damaged,
                    ..
                } => is_damaged || !satisfies(Some(&component.version), &version),
                Standing::Provided { version, .. } => {
                    !satisfies(Some(&component.version), &version)
                }
            })
            .count();
        if remaining == components.len() {
            InstallState::Available
        } else if remaining > 0 {
            InstallState::PartlyInstalled { remaining }
        } else {
            InstallState::Installed {
                version: entry.version.clone(),
                update: None,
                damaged: false,
            }
        }
    }
}
