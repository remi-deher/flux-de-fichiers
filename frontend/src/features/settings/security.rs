//! Domaine Réglages : chiffrement des secrets stockés et rotation de la clé.

use crate::shared::ui::{Button, ButtonVariant, Card, DetailRow, SectionHeader};
use leptos::prelude::*;

/// Commande à lancer dans le conteneur ; `<conteneur>` est son nom (`docker ps`).
const ROTATE_COMMAND: &str = "docker exec -it <conteneur> flux-de-fichiers rotate-secret";

#[component]
pub fn SettingsEncryption() -> impl IntoView {
    view! {
        <Card aria_labelledby="encryption-title">
            <SectionHeader
                id="encryption-title"
                title="Chiffrement des secrets"
                subtitle="Réglages de plugins et en-têtes de webhook, chiffrés au repos."
            />
            <div class="detail-grid">
                <DetailRow label="Clé active" id="security-key-id" />
                <DetailRow label="Origine de la clé" id="security-key-source" />
            </div>
            <div class="form-group">
                <label for="security-rotate-cmd" class="form-label">"Faire tourner la clé"</label>
                <pre class="code-block-wrapper"><code id="security-rotate-cmd">{ROTATE_COMMAND}</code></pre>
                <Button id="security-rotate-copy-btn" variant=ButtonVariant::Secondary>
                    "Copier la commande"
                </Button>
                <p class="form-help">
                    "Range l'ancienne clé, la nouvelle et une copie de la base dans /config/backups, rechiffre tous les secrets en une transaction, puis remplace la clé : une valeur illisible annule tout. À lancer de préférence central arrêté. Sans effet si la clé vient de SECRET_KEY_FILE."
                </p>
            </div>
        </Card>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render() -> String {
        view! { <SettingsEncryption /> }.to_html()
    }

    #[test]
    fn test_section_has_the_ids_used_by_the_client() {
        let html = render();
        for id in [
            "security-key-id",
            "security-key-source",
            "security-rotate-cmd",
            "security-rotate-copy-btn",
        ] {
            assert!(html.contains(&format!("id=\"{id}\"")), "{id}");
        }
        assert!(!html.contains("style="));
    }

    #[test]
    fn test_the_command_matches_the_cli_and_the_page_never_shows_a_secret() {
        let html = render();
        assert!(html.contains("rotate-secret"));
        assert!(!html.contains("enc:v2"));
    }
}
