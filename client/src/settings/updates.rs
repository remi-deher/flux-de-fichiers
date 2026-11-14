//! Mises à jour : version du central, version de l'interface chargée, procédure.

use super::system::{DetailRow, StatusSection};
use flux_shared::system::SystemStatus;
use leptos::prelude::*;

/// Version de l'interface WASM actuellement chargée dans le navigateur.
const CLIENT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Ce que dit la comparaison des deux versions.
fn coherence_label(central: &str, client: &str) -> &'static str {
    if central == client {
        "Interface et central sont à la même version"
    } else {
        "Versions différentes : recharger la page sans cache, ou vérifier le volume du frontend"
    }
}

fn update_rows(status: &SystemStatus) -> Vec<DetailRow> {
    vec![
        DetailRow::mono("Version du central", status.version.clone()),
        DetailRow::mono("Version de l'interface", CLIENT_VERSION),
        DetailRow::text(
            "Cohérence",
            coherence_label(&status.version, CLIENT_VERSION),
        ),
    ]
}

#[component]
pub fn UpdatesSection() -> impl IntoView {
    let footer = view! {
        <p class="form-help">
            "Version disponible : le central ne contacte aucun service externe, il ne peut donc pas la connaître. Les versions publiées se consultent sur le dépôt de l'image."
        </p>
        <p class="form-help">
            "Mettre à jour : sauvegarder /config, tirer la nouvelle image puis redémarrer le conteneur du central, ensuite celui de chaque worker. Les migrations de la base s'appliquent au démarrage."
        </p>
    }.into_any();
    view! {
        <StatusSection title="Mises à jour" subtitle="Version installée et procédure."
            rows=update_rows footer=footer />
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_version_mismatch_is_called_out() {
        assert!(coherence_label("1.0.0", "1.0.0").contains("même version"));
        assert!(coherence_label("1.0.0", "0.9.0").contains("différentes"));
    }
}
