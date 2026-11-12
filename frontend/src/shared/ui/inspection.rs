//! Tiroir d'inspection commun : en-tête, sous-navigation, puis le dossier de la ligne.

use super::icon::{Icon, IconName};
use leptos::prelude::*;

/// Section du tiroir. Le suffixe entre dans les identifiants `tab` et `pane`.
pub struct InspectionPane {
    pub suffix: &'static str,
    pub label: &'static str,
    pub body: AnyView,
}

/// Tiroir dont les onglets sont les sections du dossier ouvert.
pub fn inspection_drawer(
    id: &'static str,
    panes: Vec<InspectionPane>,
    header_actions: Option<AnyView>,
) -> impl IntoView {
    let title_id = format!("{id}-title");
    let eyebrow_id = format!("{id}-eyebrow");
    let labelled_by = title_id.clone();
    let mut tabs = Vec::with_capacity(panes.len());
    let mut sections = Vec::with_capacity(panes.len());
    for (index, pane) in panes.into_iter().enumerate() {
        let active = index == 0;
        let tab_id = format!("{id}-tab-{}", pane.suffix);
        let pane_id = format!("{id}-pane-{}", pane.suffix);
        let tab_class = if active {
            "inspection-drawer__tab is-active"
        } else {
            "inspection-drawer__tab"
        };
        let selected = if active { "true" } else { "false" };
        let tab_index = if active { "0" } else { "-1" };
        let controls = pane_id.clone();
        let labelled = tab_id.clone();
        let body = pane.body;
        tabs.push(
            view! {
                <button
                    type="button"
                    role="tab"
                    id=tab_id
                    class=tab_class
                    aria-selected=selected
                    aria-controls=controls
                    tabindex=tab_index
                >
                    {pane.label}
                </button>
            }
            .into_any(),
        );
        sections.push(if active {
            view! {
                <section
                    id=pane_id
                    class="inspection-drawer__pane"
                    role="tabpanel"
                    aria-labelledby=labelled
                >
                    {body}
                </section>
            }
            .into_any()
        } else {
            view! {
                <section
                    id=pane_id
                    class="inspection-drawer__pane"
                    role="tabpanel"
                    aria-labelledby=labelled
                    hidden
                >
                    {body}
                </section>
            }
            .into_any()
        });
    }

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
                    {header_actions}
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
                    {tabs}
                </div>
                <div class="inspection-drawer__body">
                    {sections}
                </div>
            </div>
        </aside>
    }
}

fn default_inspection_panes(id: &'static str, action: &'static str) -> Vec<InspectionPane> {
    let facts_id = format!("{id}-facts");
    let action_id = format!("{id}-action");
    vec![
        InspectionPane {
            suffix: "overview",
            label: "Aperçu",
            body: view! {
                <p id=facts_id class="inspection-panel__facts">"Le détail de la ligne apparaît ici."</p>
            }
            .into_any(),
        },
        InspectionPane {
            suffix: "verb",
            label: action,
            body: view! {
                <p class="inspection-panel__facts">"Cette action concerne la ligne ouverte."</p>
                <button
                    type="button"
                    id=action_id
                    class="btn btn-primary inspection-panel__action"
                    disabled
                >
                    {action}
                </button>
            }
            .into_any(),
        },
    ]
}

/// Dossier d'une ligne : aperçu, puis le verbe de cette ligne.
#[component]
pub fn InspectionPanel(id: &'static str, action: &'static str) -> impl IntoView {
    inspection_drawer(id, default_inspection_panes(id, action), None)
}
