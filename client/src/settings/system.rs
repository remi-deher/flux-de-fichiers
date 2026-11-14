//! État système : version, clé de chiffrement, taille de la base ; socle des sections Réseau et Stockage.

use crate::api;
use crate::ui::SectionHeader;
use flux_shared::format::format_bytes;
use flux_shared::system::{KeySource, SystemStatus};
use leptos::prelude::*;

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

fn system_rows(status: &SystemStatus) -> Vec<DetailRow> {
    vec![
        DetailRow::mono("Version", status.version.clone()),
        DetailRow::mono("Clé de chiffrement", status.key_id.clone()),
        DetailRow::text("Origine de la clé", key_source_label(status.key_source)),
        DetailRow::text(
            "Base de données",
            status
                .database_bytes
                .map_or_else(|| "En mémoire".to_owned(), format_bytes),
        ),
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
    }
    .into_any();
    view! {
        <StatusSection title="Système" subtitle="Version, clé de chiffrement et base de données."
            rows=system_rows footer=footer />
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations_skip_empty_units_and_fall_back_to_seconds() {
        assert_eq!(duration_label(28_800), "8 h");
        assert_eq!(duration_label(2_592_000), "30 j");
        assert_eq!(duration_label(90_060), "1 j 1 h 1 min");
        assert_eq!(duration_label(45), "45 s");
        assert_eq!(duration_label(-3), "0 s");
    }
}
