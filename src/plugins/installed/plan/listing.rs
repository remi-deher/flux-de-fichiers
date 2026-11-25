//! Ce que montrent le Store et la liste des éléments installés, d'après le même contexte que le plan.

use super::{context::PlanContext, states::InstalledState};
use crate::{plugins::CatalogItemType, repository::Installation};
use forgearr_shared::plugins::store::{InstallState, InstalledItem, ItemRef, StoreEntry};

impl PlanContext {
    /// Éléments de toutes les sources, homonymes compris ; les blocs sont montrés par leur propriétaire. Bloquant.
    pub fn store_entries(&self) -> Vec<StoreEntry> {
        self.sources
            .iter()
            .flat_map(|source| {
                source
                    .catalog
                    .entries
                    .iter()
                    .filter(|(_, entry)| entry.entry_type != CatalogItemType::Block)
                    .map(move |(_, entry)| (source, entry))
            })
            .map(|(source, entry)| {
                let item = ItemRef {
                    repository_id: source.origin.repository_id.clone(),
                    id: entry.id.clone(),
                };
                StoreEntry {
                    origin: source.origin.clone(),
                    install: self.install_state(&item).unwrap_or(InstallState::Available),
                    tool_status: None,
                    entry: entry.clone(),
                }
            })
            .collect()
    }

    /// Éléments inscrits, avec l'entrée que lit le moteur ; un élément illisible garde son inscription. Bloquant.
    pub fn installed_items(&self, installations: &[Installation]) -> Vec<InstalledItem> {
        installations
            .iter()
            .filter_map(|installation| {
                let record = &installation.record;
                let entry = self
                    .engine
                    .entry(&record.item_id)
                    .cloned()
                    .or_else(|| self.frozen_content(record).map(|content| content.entry))?;
                let InstalledState {
                    origin_available,
                    update,
                    damaged,
                } = self
                    .installed_state(&record.item_id)
                    .unwrap_or(InstalledState {
                        origin_available: false,
                        update: None,
                        damaged: true,
                    });
                Some(InstalledItem {
                    origin: record.origin.clone(),
                    origin_available,
                    update,
                    damaged,
                    missing: Vec::new(),
                    installed_at: installation.installed_at.clone(),
                    copy_id: record.copy_id.clone(),
                    copy_modified: None,
                    tool: None,
                    entry,
                })
            })
            .collect()
    }
}
