//! État système : version, clé de chiffrement, taille de la base et rotation de clé.

use crate::api;
use flux_shared::format::format_bytes;
use flux_shared::system::{KeySource, SystemStatus};
use leptos::prelude::*;

const STATUS_PATH: &str = "/api/v1/system/status";

pub fn key_source_label(source: KeySource) -> &'static str {
    match source {
        KeySource::Local => "Fichier local (/config/secret_key)",
        KeySource::External => "Fichier fourni (SECRET_KEY_FILE)",
    }
}

#[component]
pub fn SystemSection() -> impl IntoView {
    let status = LocalResource::new(|| api::get::<SystemStatus>(STATUS_PATH));
    view! {
        <section class="section-panel" aria-labelledby="system-title">
            <h2 id="system-title" class="section-title">"Système"</h2>
            {move || match status.get() {
                None => view! { <p class="text-dim">"Chargement…"</p> }.into_any(),
                Some(Err(error)) => view! { <p class="text-danger">{error.message()}</p> }.into_any(),
                Some(Ok(status)) => view! {
                    <div class="detail-grid">
                        <div class="detail-term">"Version"</div><div class="detail-val text-mono">{status.version}</div>
                        <div class="detail-term">"Clé de chiffrement"</div><div class="detail-val text-mono">{status.key_id}</div>
                        <div class="detail-term">"Origine de la clé"</div><div class="detail-val">{key_source_label(status.key_source)}</div>
                        <div class="detail-term">"Base de données"</div>
                        <div class="detail-val">{status.database_bytes.map_or_else(|| "En mémoire".to_owned(), format_bytes)}</div>
                    </div>
                }.into_any(),
            }}
            <p class="form-help">
                "Rotation de la clé : arrêter le central, puis lancer "
                <code>"flux-de-fichiers rotate-secret"</code>
                " ; une copie de l'ancienne clé et de la base est rangée dans /config/backups."
            </p>
            <p class="form-help">
                "Mises à jour : l'image se met à jour en tirant une nouvelle version publiée ; aucune vérification automatique n'est faite ici."
            </p>
        </section>
    }
}
