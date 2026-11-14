//! Action contextuelle d'un écran, affichée dans la capsule de recherche de la barre du haut.

use leptos::prelude::*;

/// Bouton proposé par l'écran affiché : libellé et réaction au clic.
#[derive(Clone, Copy)]
pub struct ActionSpec {
    pub label: &'static str,
    pub on_click: Callback<()>,
}

/// Emplacement partagé entre la barre du haut (qui l'affiche) et les écrans (qui le remplissent).
#[derive(Clone, Copy)]
pub struct TopbarAction {
    current: RwSignal<Option<ActionSpec>>,
}

impl TopbarAction {
    pub fn install() -> Self {
        let slot = Self {
            current: RwSignal::new(None),
        };
        provide_context(slot);
        slot
    }

    fn current(self) -> Option<ActionSpec> {
        self.current.get()
    }

    fn set(self, spec: ActionSpec) {
        self.current.set(Some(spec));
    }

    /// Ne retire que l'action de l'écran qui la déclarait : un écran suivant peut déjà avoir pris la place.
    fn clear_if(self, label: &'static str) {
        self.current.update(|current| {
            if current.is_some_and(|spec| spec.label == label) {
                *current = None;
            }
        });
    }
}

/// Déclare l'action de l'écran ; elle disparaît avec lui.
pub fn provide_topbar_action(spec: ActionSpec) {
    let Some(slot) = use_context::<TopbarAction>() else {
        return;
    };
    slot.set(spec);
    on_cleanup(move || slot.clear_if(spec.label));
}

/// Zone d'actions de la capsule de recherche ; masquée par le thème quand elle est vide.
#[component]
pub fn TopbarActionSlot() -> impl IntoView {
    let slot = use_context::<TopbarAction>().unwrap_or_else(TopbarAction::install);
    view! {
        <div class="search-capsule__tools">
            <div class="search-capsule__actions">
                {move || slot.current().map(|spec| view! {
                    <button type="button" class="search-capsule__action search-capsule__action--primary"
                        on:click=move |_| spec.on_click.run(())>{spec.label}</button>
                })}
            </div>
        </div>
    }
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
    fn an_action_is_cleared_only_by_the_screen_that_declared_it() {
        let slot = TopbarAction {
            current: RwSignal::new(None),
        };
        slot.set(spec("Ajouter un dépôt"));
        assert_eq!(slot.current().map(|s| s.label), Some("Ajouter un dépôt"));

        slot.clear_if("Autre action");
        assert!(slot.current().is_some());

        slot.set(spec("Nouvel écran"));
        slot.clear_if("Ajouter un dépôt");
        assert_eq!(slot.current().map(|s| s.label), Some("Nouvel écran"));

        slot.clear_if("Nouvel écran");
        assert!(slot.current().is_none());
    }
}
