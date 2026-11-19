//! Système : versions et mise à jour, clé de chiffrement, base de données, limites et espace disque ; socle du tableau de bord Paramètres.

use super::key_rotation::KeyRotationControl;
use super::recovery_kit::{RecoveryKitControl, kit_state, kit_state_label};
use crate::api;
use crate::ui::{ProgressBar, SectionHeader};
use forgearr_shared::format::format_bytes;
use forgearr_shared::system::{KeySource, SystemStatus, UpdateCheckStatus, VolumeSpace};
use leptos::prelude::*;

const SYSTEM_UPDATES_PATH: &str = "/api/v1/system/updates";

/// Version compilée dans l'interface WASM chargée dans le navigateur (témoin de cohérence : la
/// version de la release affichée vient du serveur, voir `forgearr::version`).
const CLIENT_VERSION: &str = env!("CARGO_PKG_VERSION");
const SECONDS_PER_MINUTE: i64 = 60;
const SECONDS_PER_HOUR: i64 = 60 * SECONDS_PER_MINUTE;
const SECONDS_PER_DAY: i64 = 24 * SECONDS_PER_HOUR;

pub fn key_source_label(source: KeySource) -> &'static str {
    match source {
        KeySource::Local => "Fichier local (/config/secret_key)",
        KeySource::External => "Fichier fourni (SECRET_KEY_FILE)",
    }
}

pub fn is_version_coherent(status: &SystemStatus) -> bool {
    status.build() == CLIENT_VERSION
}

/// Version de l'interface : celle de la release quand l'interface correspond au serveur, sinon
/// la version compilée du navigateur (cache périmé).
fn displayed_interface_version(release: &str, server_build: &str, client_build: &str) -> String {
    if server_build == client_build {
        release.to_owned()
    } else {
        client_build.to_owned()
    }
}

fn interface_version(status: &SystemStatus) -> String {
    displayed_interface_version(&status.version, status.build(), CLIENT_VERSION)
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
                <div class="detail-row">
                    <span class="detail-term">{row.term}</span>
                    <span class="detail-val" class:text-mono=row.mono>{row.value}</span>
                </div>
            }).collect_view()}
        </div>
    }
}

/// Section alimentée par `/api/v1/system/status` : `rows` en tire les lignes à afficher.
#[component]
pub fn StatusSection(
    title: &'static str,
    subtitle: &'static str,
    status: Signal<Option<Result<SystemStatus, String>>>,
    rows: fn(&SystemStatus) -> Vec<DetailRow>,
    #[prop(optional)] extra: Option<fn(&SystemStatus) -> AnyView>,
    #[prop(optional)] footer: Option<AnyView>,
) -> impl IntoView {
    view! {
        <section class="settings-section section-panel">
            <SectionHeader title=title subtitle=subtitle />
            {move || match status.get() {
                None => view! { <p class="text-dim">"Chargement…"</p> }.into_any(),
                Some(Err(message)) => view! { <p class="text-danger">{message}</p> }.into_any(),
                Some(Ok(status)) => view! {
                    <DetailGrid rows=rows(&status) />
                    {extra.map(|render| render(&status))}
                }.into_any(),
            }}
            {footer}
        </section>
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

/// Pourcentage d'occupation du volume disque, ou `None` si la capacité totale est nulle.
pub fn volume_used_percent(vol: &VolumeSpace) -> Option<u8> {
    if vol.total_bytes == 0 {
        return None;
    }
    let used = u128::from(vol.total_bytes.saturating_sub(vol.available_bytes));
    let total = u128::from(vol.total_bytes);
    Some((used.saturating_mul(100) / total).min(100) as u8)
}

fn volume_meter(status: &SystemStatus) -> AnyView {
    status
        .storage
        .volume
        .as_ref()
        .and_then(volume_used_percent)
        .map(|percent| {
            view! {
                <div class="system-volume-meter">
                    <span class="detail-term">"Occupation du volume"</span>
                    <ProgressBar percent=u32::from(percent) label=format!("{percent} %") />
                </div>
            }
        })
        .into_any()
}

fn system_rows(status: &SystemStatus) -> Vec<DetailRow> {
    vec![
        DetailRow::mono("Version du central", status.version.clone()),
        DetailRow::mono("Version de l'interface", interface_version(status)),
        DetailRow::text("Cohérence", coherence_label(status.build(), CLIENT_VERSION)),
        DetailRow::mono("Clé de chiffrement", status.key_id.clone()),
        DetailRow::text("Origine de la clé", key_source_label(status.key_source)),
        DetailRow::text(
            "Kit de récupération",
            kit_state_label(kit_state(
                &status.key_id,
                status.recovery_kit_key_id.as_deref(),
            )),
        ),
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
pub fn SystemSection(
    status: Signal<Option<Result<SystemStatus, String>>>,
    on_changed: Callback<()>,
) -> impl IntoView {
    let check_updates = Action::new_local(move |()| async move {
        api::get::<UpdateCheckStatus>(SYSTEM_UPDATES_PATH).await
    });
    let updates_enabled = Signal::derive(move || {
        status
            .get()
            .and_then(Result::ok)
            .is_some_and(|s| s.check_updates_enabled)
    });
    let footer = view! {
        <div class="stack">
            <KeyRotationControl status=status on_changed=on_changed />
            <RecoveryKitControl on_changed=on_changed />
            {move || updates_enabled.get().then(|| view! {
                <div class="cluster cluster-wrap">
                    <button type="button" class="btn btn-secondary btn-sm"
                        disabled=move || check_updates.pending().get()
                        on:click=move |_| { check_updates.dispatch(()); }>
                        {move || if check_updates.pending().get() { "Vérification…" } else { "Vérifier les mises à jour" }}
                    </button>
                    {move || match check_updates.value().get() {
                        Some(Ok(res)) if res.error.is_some() => view! {
                            <span class="form-help">{res.error.unwrap_or_default()}</span>
                        }.into_any(),
                        Some(Ok(res)) if res.update_available => view! {
                            <span class="text-strong">
                                {format!("Nouvelle version disponible : v{}", res.latest_version.unwrap_or_default())}
                            </span>
                        }.into_any(),
                        Some(Ok(res)) => view! {
                            <span class="form-help">
                                {format!("À jour (v{}).", res.latest_version.unwrap_or(res.current_version))}
                            </span>
                        }.into_any(),
                        Some(Err(err)) => view! { <span class="form-error">{err.message()}</span> }.into_any(),
                        None => ().into_any(),
                    }}
                </div>
            })}
            <details class="settings-help-details">
                <summary class="settings-help-details__summary">
                    "Procédures de maintenance (clé de chiffrement & mise à jour)"
                </summary>
                <div class="settings-help-details__body">
                    <p class="form-help">
                        "Clé de chiffrement : le bouton « Renouveler la clé de chiffrement » suffit. En secours, central arrêté : "
                        <code>"forgearr rotate-secret"</code>
                        "."
                    </p>
                    <p class="form-help">
                        "Version disponible : désactivée par défaut ; activez la vérification facultative dans Paramètres → Réseau."
                    </p>
                    <p class="form-help">
                        "Mettre à jour : sauvegarder /config, tirer la nouvelle image puis redémarrer le conteneur du central, ensuite celui de chaque worker. Les migrations de la base s'appliquent au démarrage."
                    </p>
                </div>
            </details>
        </div>
    }
    .into_any();
    view! {
        <StatusSection title="Système & Stockage" subtitle="Versions, clé de chiffrement, base de données et espace disque."
            status=status rows=system_rows extra=volume_meter footer=footer />
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interface_shows_the_release_version_when_it_matches_the_server_build() {
        // Image promue : binaire compilé en 0.44.0, release 0.45.0.
        assert_eq!(
            displayed_interface_version("0.45.0", "0.44.0", "0.44.0"),
            "0.45.0"
        );
        // Navigateur resté sur une ancienne interface : sa propre version est montrée.
        assert_eq!(
            displayed_interface_version("0.45.0", "0.44.0", "0.43.0"),
            "0.43.0"
        );
    }

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
    fn volume_used_percent_handles_zero_and_typical_usage() {
        assert_eq!(
            volume_used_percent(&VolumeSpace {
                total_bytes: 0,
                available_bytes: 0,
            }),
            None
        );
        assert_eq!(
            volume_used_percent(&VolumeSpace {
                total_bytes: 1_000,
                available_bytes: 250,
            }),
            Some(75)
        );
        assert_eq!(
            volume_used_percent(&VolumeSpace {
                total_bytes: 1_000,
                available_bytes: 1_200,
            }),
            Some(0)
        );
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
