//! Tiroir d'inspection commun : en-tête, sous-navigation, puis le dossier de la ligne.

use super::icon::{Icon, IconName};
use leptos::prelude::*;

/// Dossier de la ligne choisie. Le script ouvre le tiroir et remplit l'en-tête.
#[component]
pub fn InspectionPanel(id: &'static str, action: &'static str) -> impl IntoView {
    let title_id = format!("{id}-title");
    let facts_id = format!("{id}-facts");
    let action_id = format!("{id}-action");
    let eyebrow_id = format!("{id}-eyebrow");
    let overview_id = format!("{id}-pane-overview");
    let verb_id = format!("{id}-pane-verb");
    let overview_tab = format!("{id}-tab-overview");
    let verb_tab = format!("{id}-tab-verb");
    let labelled_by = title_id.clone();
    let overview_controls = overview_id.clone();
    let verb_controls = verb_id.clone();
    let overview_label = overview_tab.clone();
    let verb_label = verb_tab.clone();
    view! {
        <aside id=id class="inspection-drawer" aria-hidden="true">
            <button
                type="button"
                class="inspection-drawer__scrim"
                data-inspection-close
                aria-label="Fermer le tiroir"
            ></button>
            <div
                class="inspection-drawer__panel"
                role="dialog"
                aria-modal="false"
                aria-labelledby=labelled_by
                tabindex="-1"
            >
                <header class="inspection-drawer__header">
                    <div class="inspection-drawer__identity">
                        <p id=eyebrow_id class="inspection-drawer__eyebrow">"Dossier"</p>
                        <p id=title_id class="inspection-panel__title">"Sélectionnez une ligne"</p>
                    </div>
                    <button
                        type="button"
                        class="header-icon-btn"
                        data-inspection-close
                        aria-label="Fermer le tiroir"
                    >
                        <Icon name=IconName::Close size=18 />
                    </button>
                </header>
                <div class="inspection-drawer__subnav" role="tablist" aria-label="Sections du dossier">
                    <button
                        type="button"
                        role="tab"
                        id=overview_tab
                        class="inspection-drawer__tab is-active"
                        aria-selected="true"
                        aria-controls=overview_controls
                        tabindex="0"
                    >
                        "Aperçu"
                    </button>
                    <button
                        type="button"
                        role="tab"
                        id=verb_tab
                        class="inspection-drawer__tab"
                        aria-selected="false"
                        aria-controls=verb_controls
                        tabindex="-1"
                    >
                        {action}
                    </button>
                </div>
                <div class="inspection-drawer__body">
                    <section
                        id=overview_id
                        class="inspection-drawer__pane"
                        role="tabpanel"
                        aria-labelledby=overview_label
                    >
                        <p id=facts_id class="inspection-panel__facts">"Le détail de la ligne apparaît ici."</p>
                    </section>
                    <section
                        id=verb_id
                        class="inspection-drawer__pane"
                        role="tabpanel"
                        aria-labelledby=verb_label
                        hidden
                    >
                        <p class="inspection-panel__facts">"Cette action concerne la ligne ouverte."</p>
                        <button
                            type="button"
                            id=action_id
                            class="btn btn-primary inspection-panel__action"
                            disabled
                        >
                            {action}
                        </button>
                    </section>
                </div>
            </div>
        </aside>
    }
}
