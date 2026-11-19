//! Actions contextuelles d'un écran, réparties par le header commun (principale, menu, retour).

use leptos::prelude::*;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_REGISTRATION_ID: AtomicU64 = AtomicU64::new(1);

/// Bouton proposé par l'écran affiché : libellé et réaction au clic.
#[derive(Clone, Copy)]
pub struct ActionSpec {
    pub label: &'static str,
    pub on_click: Callback<()>,
}

#[derive(Clone, Default)]
struct RegisteredActions {
    id: u64,
    specs: Vec<ActionSpec>,
}

/// Emplacement partagé entre la barre du haut (qui l'affiche) et les écrans (qui le remplissent).
#[derive(Clone, Copy)]
pub struct TopbarAction {
    current: RwSignal<Option<RegisteredActions>>,
}

impl TopbarAction {
    pub fn install() -> Self {
        let slot = Self {
            current: RwSignal::new(None),
        };
        provide_context(slot);
        slot
    }

    pub fn specs(&self) -> Vec<ActionSpec> {
        self.current
            .get()
            .map_or_else(Vec::new, |entry| entry.specs)
    }

    fn set_many(self, specs: Vec<ActionSpec>) -> u64 {
        let id = NEXT_REGISTRATION_ID.fetch_add(1, Ordering::Relaxed);
        self.current.set(Some(RegisteredActions { id, specs }));
        id
    }

    /// Ne retire que l'enregistrement dont ce composant était propriétaire.
    fn clear_id(self, id: u64) {
        self.current.update(|current| {
            if current.as_ref().is_some_and(|entry| entry.id == id) {
                *current = None;
            }
        });
    }
}

/// Déclare l'action unique de l'écran ; elle disparaît avec lui.
pub fn provide_topbar_action(spec: ActionSpec) {
    provide_topbar_actions(vec![spec]);
}

/// Déclare une ou plusieurs actions d'écran dans la barre du haut ; elles disparaissent avec lui.
pub fn provide_topbar_actions(specs: Vec<ActionSpec>) {
    let Some(slot) = use_context::<TopbarAction>() else {
        return;
    };
    let id = slot.set_many(specs);
    on_cleanup(move || slot.clear_id(id));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(label: &'static str) -> ActionSpec {
        ActionSpec {
            label,
            on_click: Callback::new(|()| {}),
        }
    }

    #[test]
    fn an_action_is_cleared_only_by_its_owner_registration() {
        let slot = TopbarAction {
            current: RwSignal::new(None),
        };
        let first_id = slot.set_many(vec![spec("Ajouter un dépôt")]);
        assert_eq!(slot.specs().len(), 1);
        assert_eq!(slot.specs()[0].label, "Ajouter un dépôt");

        slot.clear_id(first_id + 999);
        assert_eq!(slot.specs().len(), 1);

        let second_id = slot.set_many(vec![spec("Importer un fichier"), spec("Modèle de blocs")]);
        slot.clear_id(first_id);
        assert_eq!(slot.specs().len(), 2);
        assert_eq!(slot.specs()[0].label, "Importer un fichier");
        assert_eq!(slot.specs()[1].label, "Modèle de blocs");

        slot.clear_id(second_id);
        assert!(slot.specs().is_empty());
    }
}
