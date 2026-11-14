//! Système : versions et mise à jour, clé de chiffrement, base de données, limites et espace disque ; socle de la section Réseau.

use crate::api;
use crate::ui::SectionHeader;
use flux_shared::format::format_bytes;
use flux_shared::system::{KeySource, SystemStatus, VolumeSpace};
use leptos::prelude::*;

/// Version de l'interface WASM actuellement chargée dans le navigateur.
const CLIENT_VERSION: &str = env!("CARGO_PKG_VERSION");
const STATUS_PATH: &str = "/api/v1/system/status";
const SECONDS_PER_MINUTE: i64 = 60;
const SECONDS_PER_HOUR: i64 = 60 * SECONDS_PER_MINUTE;
const SECONDS_PER_DAY: i64 = 24 * SECONDS_PER_HOUR;

pub fn key_source_label(source: KeySource) -> &'static str {
    match source {
        KeySource::Local => "Fichier local (/config/secret_key)",
        KeySource::External => "Fichier fourni (SECRET_KEY_FILE)",
    }
}

/// Ligne d'une grille de détails : terme et valeur, la valeur éventuellement en chasse fixe.
pub struct DetailRow {
    term: &'static str,
    value: String,
    mono: bool,
}

impl DetailRow {
    pub fn text(term: &'static str, value: impl Into<String>) -> Self {
        Self {
            term,
            value: value.into(),
            mono: false,
        }
    }

    pub fn mono(term: &'static str, value: impl Into<String>) -> Self {
        Self {
            term,
            value: value.into(),
            mono: true,
        }
    }
}

#[component]
pub fn DetailGrid(rows: Vec<DetailRow>) -> impl IntoView {
    view! {
        <div class="detail-grid">
            {rows.into_iter().map(|row| view! {
                <div class="detail-term">{row.term}</div>
                <div class="detail-val" class:text-mono=row.mono>{row.value}</div>
            }).collect_view()}
        </div>
    }
}

/// Section alimentée par `/api/v1/system/status` : `rows` en tire les lignes à afficher.
#[component]
pub fn StatusSection(
    title: &'static str,
    subtitle: &'static str,
    rows: fn(&SystemStatus) -> Vec<DetailRow>,
    #[prop(optional)] footer: Option<AnyView>,
) -> impl IntoView {
    let status = LocalResource::new(|| api::get::<SystemStatus>(STATUS_PATH));
    view! {
        <div class="settings-section">
            <SectionHeader title=title subtitle=subtitle />
            {move || match status.get() {
                None => view! { <p class="text-dim">"Chargement…"</p> }.into_any(),
                Some(Err(error)) => view! { <p class="text-danger">{error.message()}</p> }.into_any(),
                Some(Ok(status)) => view! { <DetailGrid rows=rows(&status) /> }.into_any(),
            }}
            {footer}
        </div>
    }
}

/// Durée lisible : jours, heures puis minutes, sans les unités nulles.
pub fn duration_label(seconds: i64) -> String {
    let parts = [
        (seconds / SECONDS_PER_DAY, "j"),
        (seconds % SECONDS_PER_DAY / SECONDS_PER_HOUR, "h"),
        (seconds % SECONDS_PER_HOUR / SECONDS_PER_MINUTE, "min"),
    ];
    let text: Vec<String> = parts
        .iter()
        .filter(|(count, _)| *count > 0)
        .map(|(count, unit)| format!("{count} {unit}"))
        .collect();
    if text.is_empty() {
        format!("{} s", seconds.max(0))
    } else {
        text.join(" ")
    }
}

/// Ce que dit la comparaison des versions du central et de l'interface.
fn coherence_label(central: &str, client: &str) -> &'static str {
    if central == client {
        "Interface et central sont à la même version"
    } else {
        "Versions différentes : recharger la page sans cache, ou vérifier le volume du frontend"
    }
}

fn volume_label(volume: Option<VolumeSpace>) -> String {
    volume.map_or_else(
        || "Illisible".to_owned(),
        |space| {
            format!(
                "{} libres sur {}",
                format_bytes(space.available_bytes),
                format_bytes(space.total_bytes)
            )
        },
    )
}

fn system_rows(status: &SystemStatus) -> Vec<DetailRow> {
    vec![
        DetailRow::mono("Version du central", status.version.clone()),
        DetailRow::mono("Version de l'interface", CLIENT_VERSION),
        DetailRow::text(
            "Cohérence",
            coherence_label(&status.version, CLIENT_VERSION),
        ),
        DetailRow::mono("Clé de chiffrement", status.key_id.clone()),
        DetailRow::text("Origine de la clé", key_source_label(status.key_source)),
        DetailRow::text(
            "Base de données",
            status
                .database_bytes
                .map_or_else(|| "En mémoire".to_owned(), format_bytes),
        ),
        DetailRow::text(
            "Taille maximale d'un fichier",
            format_bytes(status.storage.max_file_bytes),
        ),
        DetailRow::text("Volume des données", volume_label(status.storage.volume)),
    ]
}

#[component]
pub fn SystemSection() -> impl IntoView {
    let footer = view! {
        <p class="form-help">
            "Rotation de la clé : arrêter le central, puis lancer "
            <code>"flux-de-fichiers rotate-secret"</code>
            " ; une copie de l'ancienne clé et de la base est rangée dans /config/backups."
        </p>
        <p class="form-help">
            "Version disponible : le central ne contacte aucun service externe, il ne peut donc pas la connaître. Les versions publiées se consultent sur le dépôt de l'image."
        </p>
        <p class="form-help">
            "Mettre à jour : sauvegarder /config, tirer la nouvelle image puis redémarrer le conteneur du central, ensuite celui de chaque worker. Les migrations de la base s'appliquent au démarrage."
        </p>
    }
    .into_any();
    view! {
        <StatusSection title="Système" subtitle="Versions, mise à jour, clé de chiffrement, base de données et espace disque."
            rows=system_rows footer=footer />
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

    #[test]
    fn the_volume_reads_free_over_total_or_says_it_is_unreadable() {
        let space = VolumeSpace {
            total_bytes: 2048,
            available_bytes: 1024,
        };
        assert_eq!(volume_label(Some(space)), "1 Kio libres sur 2 Kio");
        assert_eq!(volume_label(None), "Illisible");
    }

    #[test]
    fn durations_skip_empty_units_and_fall_back_to_seconds() {
        assert_eq!(duration_label(28_800), "8 h");
        assert_eq!(duration_label(2_592_000), "30 j");
        assert_eq!(duration_label(90_060), "1 j 1 h 1 min");
        assert_eq!(duration_label(45), "45 s");
        assert_eq!(duration_label(-3), "0 s");
    }
}
