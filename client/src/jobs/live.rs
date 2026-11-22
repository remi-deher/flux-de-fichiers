//! État en direct des tâches, alimenté par le flux commun de l'interface (ADR 0028).

use super::model::{needs_reload, progress_of};
use forgearr_shared::jobs::JobEvent;
use leptos::prelude::*;
use std::collections::HashMap;
use uuid::Uuid;

/// Tâches en direct, partagées par contexte.
#[derive(Clone, Copy)]
pub struct LiveJobs {
    /// Incrémenté à chaque changement d'état d'une tâche.
    pub reload: RwSignal<u32>,
    /// Dernière progression connue par tâche.
    pub progress: RwSignal<HashMap<Uuid, u32>>,
}

impl LiveJobs {
    /// Fournit le contexte ; le flux commun appelle ensuite [`Self::apply`] et [`Self::resync`].
    pub fn install() -> Self {
        let live = Self {
            reload: RwSignal::new(0),
            progress: RwSignal::new(HashMap::new()),
        };
        provide_context(live);
        live
    }

    pub fn apply(self, event: &JobEvent) {
        if let Some((job_id, pct)) = progress_of(event) {
            let _ = self.progress.try_update(|map| {
                map.insert(job_id, pct);
            });
        }
        if needs_reload(event.event_type) {
            self.resync();
        }
    }

    /// Fait relire les listes de tâches.
    pub fn resync(self) {
        let _ = self
            .reload
            .try_update(|count| *count = count.wrapping_add(1));
    }
}
