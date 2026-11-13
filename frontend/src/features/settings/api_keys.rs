//! Domaine Réglages : clés d'API des machines (workers, scripts), au gabarit commun des registres.

use super::role_option;
use crate::shared::ui::{
    Button, ButtonVariant, DetailRow, FormField, InspectionPane, RegistreTable, SectionHeader,
    inspection_drawer,
};
use flux_shared::Role;
use leptos::prelude::*;

fn api_key_panes() -> Vec<InspectionPane> {
    vec![InspectionPane {
        suffix: "overview",
        label: "Clé",
        body: view! {
            <div id="api-key-form">
                <FormField id="api-key-name" label="Nom" required=true autocomplete="off" help="Par exemple le nom de la machine." />
                <div class="form-group">
                    <label for="api-key-role" class="form-label">"Rôle"</label>
                    <select id="api-key-role" class="form-select">
                        {role_option(Role::Worker, "Worker (traitement des tâches)")}
                        {role_option(Role::Admin, "Administrateur (scripts)")}
                    </select>
                    <p class="form-help">"Un worker ne peut que réclamer des tâches. Un administrateur peut tout piloter."</p>
                </div>
            </div>

            <div id="api-key-created" hidden>
                <p class="toast-notice visible info">
                    "Copiez cette clé maintenant : elle ne sera plus jamais affichée."
                </p>
                <pre class="code-block-wrapper"><code id="api-key-created-value"></code></pre>
                <Button id="api-key-copy-btn" variant=ButtonVariant::Secondary>"Copier la clé"</Button>
            </div>

            <div id="api-key-detail" class="detail-grid" hidden>
                <DetailRow label="Identifiant" id="api-key-detail-id" />
                <DetailRow label="Nom" id="api-key-detail-name" />
                <DetailRow label="Rôle" id="api-key-detail-role" />
                <DetailRow label="Créée" id="api-key-detail-created" />
                <DetailRow label="Dernière utilisation" id="api-key-detail-used" />
                <DetailRow label="État" id="api-key-detail-state" />
            </div>
        }
        .into_any(),
    }]
}

fn api_key_actions() -> AnyView {
    view! {
        <div class="inspection-drawer__actions">
            <button type="button" id="api-key-create-btn" class="btn btn-primary btn-sm inspection-panel__action" hidden disabled>
                "Créer la clé"
            </button>
            <button type="button" id="api-key-revoke-btn" class="btn btn-danger btn-sm inspection-panel__action" hidden disabled>
                "Révoquer"
            </button>
        </div>
    }
    .into_any()
}

#[component]
pub fn SettingsApiKeys() -> impl IntoView {
    view! {
        <section class="section-panel" aria-labelledby="api-keys-title">
            <SectionHeader
                id="api-keys-title"
                title="Clés d'API"
                subtitle="Une clé par machine (worker, script). Seul son haché est conservé ; elle se révoque à tout moment."
            />
            <div class="registre" id="api-keys-registre">
                <RegistreTable
                    aria_label="Clés d'API"
                    headers=vec!["Nom", "Rôle", "Créée", "Dernière utilisation", "État", "Action"]
                    body_id="api-keys-table-body"
                    empty="Chargement des clés…"
                />
                {inspection_drawer("api-keys-fact", api_key_panes(), Some(api_key_actions()))}
            </div>
        </section>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render() -> String {
        view! { <SettingsApiKeys /> }.to_html()
    }

    #[test]
    fn test_registre_uses_the_common_table_and_drawer() {
        let html = render();
        assert!(html.contains("id=\"api-keys-table-body\""));
        assert!(html.contains("id=\"api-keys-fact\""));
        assert!(html.contains("class=\"registre\""));
    }

    #[test]
    fn test_role_choices_come_from_the_shared_names() {
        let html = render();
        assert!(html.contains(&format!("value=\"{}\"", Role::Worker.as_str())));
        assert!(html.contains(&format!("value=\"{}\"", Role::Admin.as_str())));
    }

    #[test]
    fn test_drawer_has_every_id_used_by_the_client() {
        let html = render();
        for id in [
            "api-key-name",
            "api-key-role",
            "api-key-created",
            "api-key-created-value",
            "api-key-copy-btn",
            "api-key-detail",
            "api-key-detail-id",
            "api-key-detail-state",
            "api-key-create-btn",
            "api-key-revoke-btn",
        ] {
            assert!(html.contains(&format!("id=\"{id}\"")), "{id}");
        }
        assert!(!html.contains("style="));
    }
}
