//! Registre du catalogue : une sous-navigation par nature, plus les dépôts.

use crate::core::{SubNav, SubNavTabItem};
use crate::shared::ui::{RegistreFact, RegistreTable};
use leptos::prelude::*;

fn plugin_tab(
    id: &'static str,
    label: &'static str,
    target_id: &'static str,
    is_active: bool,
) -> SubNavTabItem {
    SubNavTabItem {
        id,
        label,
        icon: None,
        target_id,
        is_active,
        badge: None,
    }
}

fn catalog_pane(
    dom_id: &'static str,
    label: &'static str,
    headers: Vec<&'static str>,
    body_id: &'static str,
    fact_id: &'static str,
    action_id: &'static str,
    hidden: bool,
) -> impl IntoView {
    view! {
        <div id=dom_id class="registre" hidden=hidden>
            <RegistreTable
                aria_label=label
                headers=headers
                body_id=body_id
                empty="Chargement du catalogue…"
            />
            <RegistreFact id=fact_id action_id=action_id action="Ouvrir" />
        </div>
    }
}

#[component]
pub fn PluginsPanel() -> impl IntoView {
    let subnav_tabs = vec![
        plugin_tab(
            "btn-subtab-plugins-tool",
            "Outils",
            "subtab-plugins-tool",
            true,
        ),
        plugin_tab(
            "btn-subtab-plugins-plugin",
            "Plugins",
            "subtab-plugins-plugin",
            false,
        ),
        plugin_tab(
            "btn-subtab-plugins-flow",
            "Flux",
            "subtab-plugins-flow",
            false,
        ),
        plugin_tab(
            "btn-subtab-plugins-block",
            "Blocs",
            "subtab-plugins-block",
            false,
        ),
        plugin_tab(
            "btn-subtab-plugins-app",
            "Applications",
            "subtab-plugins-app",
            false,
        ),
        plugin_tab(
            "btn-subtab-plugins-preset",
            "Préréglages",
            "subtab-plugins-preset",
            false,
        ),
        plugin_tab(
            "btn-subtab-plugins-resource",
            "Ressources",
            "subtab-plugins-resource",
            false,
        ),
        plugin_tab(
            "btn-subtab-plugins-repos",
            "Dépôts",
            "subtab-plugins-repos",
            false,
        ),
    ];

    view! {
        <div
            id=crate::core::panel::PLUGINS.dom_id
            class="tab-panel"
            role="tabpanel"
            aria-labelledby=crate::core::panel::PLUGINS.button_id
            hidden
        >
            <SubNav
                aria_label="Natures du catalogue"
                tabs=subnav_tabs
            />

            {catalog_pane(
                "subtab-plugins-tool",
                "Outils du catalogue",
                vec!["Nom", "Version", "Capacité", "État", "Action"],
                "catalog-body-tool",
                "catalog-fact-tool",
                "catalog-fact-tool-action",
                false,
            )}
            {catalog_pane(
                "subtab-plugins-plugin",
                "Plugins du catalogue",
                vec!["Nom", "Version", "Tags", "Action"],
                "catalog-body-plugin",
                "catalog-fact-plugin",
                "catalog-fact-plugin-action",
                true,
            )}
            {catalog_pane(
                "subtab-plugins-flow",
                "Flux publiés",
                vec!["Nom", "Version", "Capacité", "Action"],
                "catalog-body-flow",
                "catalog-fact-flow",
                "catalog-fact-flow-action",
                true,
            )}
            {catalog_pane(
                "subtab-plugins-block",
                "Blocs du catalogue",
                vec!["Nom", "Version", "Catégorie", "Action"],
                "catalog-body-block",
                "catalog-fact-block",
                "catalog-fact-block-action",
                true,
            )}
            {catalog_pane(
                "subtab-plugins-app",
                "Applications du catalogue",
                vec!["Nom", "Version", "Type", "Action"],
                "catalog-body-app",
                "catalog-fact-app",
                "catalog-fact-app-action",
                true,
            )}
            {catalog_pane(
                "subtab-plugins-preset",
                "Préréglages du catalogue",
                vec!["Nom", "Version", "Type", "Action"],
                "catalog-body-preset",
                "catalog-fact-preset",
                "catalog-fact-preset-action",
                true,
            )}
            {catalog_pane(
                "subtab-plugins-resource",
                "Ressources du catalogue",
                vec!["Nom", "Type", "Action"],
                "catalog-body-resource",
                "catalog-fact-resource",
                "catalog-fact-resource-action",
                true,
            )}

            <div id="subtab-plugins-repos" class="registre" hidden>
                <div class="registre-toolbar">
                    <h2 class="section-title">"Dépôts"</h2>
                    <button
                        type="button"
                        id="btn-open-add-repo-dialog"
                        class="btn btn-primary btn-sm"
                        aria-label="Ajouter un nouveau dépôt de plugins"
                    >
                        "Ajouter un dépôt"
                    </button>
                </div>
                <RegistreTable
                    aria_label="Dépôts de plugins"
                    headers=vec!["Dépôt", "URL d'index", "Type", "État", "Dernière synchro", "Action"]
                    body_id="repos-table-body"
                    empty="Chargement des dépôts…"
                />
                <RegistreFact id="repos-fact" action_id="repos-fact-action" action="Synchroniser" />
            </div>

            <dialog id="add-repo-dialog" class="app-dialog dialog-narrow" aria-labelledby="add-repo-title" aria-modal="true">
                <div class="dialog-header">
                    <h3 id="add-repo-title" class="dialog-title">"Ajouter un dépôt de sources"</h3>
                    <button type="button" id="add-repo-close-x" class="btn btn-secondary btn-sm" aria-label="Fermer">
                        "✕"
                    </button>
                </div>
                <div class="dialog-body">
                    <p class="section-subtitle no-margin-top">
                        "Renseignez l'URL HTTP(S) pointant vers le fichier d'index JSON (schema catalogue v1) du dépôt distant."
                    </p>
                    <div class="form-group">
                        <label for="repo-name-input" class="form-label">"Nom du dépôt"</label>
                        <input type="text" id="repo-name-input" class="form-input" placeholder="Ex : Dépôt Communautaire Omarchy" required />
                    </div>
                    <div class="form-group">
                        <label for="repo-url-input" class="form-label">"URL de l'index (index.json)"</label>
                        <input type="url" id="repo-url-input" class="form-input" placeholder="https://raw.githubusercontent.com/.../catalogue/index.json" required />
                    </div>
                </div>
                <div class="dialog-footer dialog-footer-ruled">
                    <button type="button" id="add-repo-cancel-btn" class="btn btn-secondary">"Annuler"</button>
                    <button type="button" id="add-repo-save-btn" class="btn btn-primary">"Ajouter le dépôt"</button>
                </div>
            </dialog>

            <dialog id="plugin-config-dialog" class="app-dialog" aria-labelledby="plugin-config-title" aria-modal="true">
                <div class="dialog-header">
                    <h3 id="plugin-config-title" class="dialog-title">"Configuration du plugin"</h3>
                    <button type="button" id="plugin-config-close-x" class="btn btn-secondary btn-sm" aria-label="Fermer">
                        "✕"
                    </button>
                </div>
                <div class="dialog-body">
                    <p class="section-subtitle no-margin-top">
                        "Ces paramètres sont sauvegardés dans votre base centrale SQLite (/config) et sont automatiquement transmis aux blocs correspondants dans les flux."
                    </p>
                    <input type="hidden" id="plugin-config-current-id" value="" />
                    <div id="plugin-config-fields"></div>
                </div>
                <div class="dialog-footer dialog-footer-ruled">
                    <button type="button" id="plugin-config-cancel-btn" class="btn btn-secondary">"Annuler"</button>
                    <button type="button" id="plugin-config-save-btn" class="btn btn-primary">
                        "Enregistrer les valeurs par défaut"
                    </button>
                </div>
            </dialog>
        </div>
    }
}
