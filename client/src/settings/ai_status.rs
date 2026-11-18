//! État du fournisseur IA, consommation du jour et quotas quotidiens (Paramètres > Assistant IA).

use crate::api;
use crate::format;
use crate::ui::{SectionHeader, Tag, TagTone, Toasts};
use chrono::DateTime;
use flux_shared::ai::{
    AiDiagnosis, AiDiagnosisSettings, AiHealth, AiQuotaSettings, MAX_AI_DAILY_QUOTA,
};
use leptos::ev;
use leptos::prelude::*;

const AI_HEALTH_PATH: &str = "/api/v1/ai/health";
const AI_QUOTAS_PATH: &str = "/api/v1/ai/quotas";
const AI_DIAGNOSIS_PATH: &str = "/api/v1/ai/diagnosis";

/// Date RFC 3339 du central, affichée dans le fuseau du navigateur ; telle quelle si illisible.
fn when(raw: &str) -> String {
    DateTime::parse_from_rfc3339(raw)
        .map(|date| format::datetime(date.to_utc()))
        .unwrap_or_else(|_| raw.to_owned())
}

/// Plafond saisi : vide ou `0` laisse la limite ouverte ; au-delà du maximum, refusé.
fn parse_quota(raw: &str) -> Option<u32> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Some(0);
    }
    trimmed
        .parse::<u32>()
        .ok()
        .filter(|value| *value <= MAX_AI_DAILY_QUOTA)
}

/// « 12 / 200 » ou « 12 (sans limite) ».
fn usage_label(used: u32, limit: u32) -> String {
    if limit == 0 {
        format!("{used} (sans limite)")
    } else {
        format!("{used} / {limit}")
    }
}

#[component]
pub fn AiStatusPanel() -> impl IntoView {
    let reload = RwSignal::new(0u64);
    let health = LocalResource::new(move || {
        reload.track();
        async move { api::get::<AiHealth>(AI_HEALTH_PATH).await }
    });
    view! {
        <section class="settings-section section-panel">
            <SectionHeader
                title="État et quotas"
                subtitle="Derniers appels au fournisseur (depuis le démarrage du central) et questions posées aujourd'hui (UTC). Une question est une conversation lancée."
            />
            {move || match health.get() {
                None => view! { <p class="text-dim">"Chargement…"</p> }.into_any(),
                Some(Err(err)) => view! { <p class="form-error">{err.message()}</p> }.into_any(),
                Some(Ok(state)) => view! {
                    <AiStatusBody state=state on_saved=Callback::new(move |()| reload.update(|n| *n = n.wrapping_add(1))) />
                }.into_any(),
            }}
        </section>
    }
}

#[component]
fn AiStatusBody(state: AiHealth, on_saved: Callback<()>) -> impl IntoView {
    let per_user = RwSignal::new(state.quota.per_user_daily.to_string());
    let global = RwSignal::new(state.quota.global_daily.to_string());
    let feedback = RwSignal::new(None::<String>);
    let saving = RwSignal::new(false);
    let quota = state.quota;

    let submit = move |event: ev::SubmitEvent| {
        event.prevent_default();
        let (Some(per_user_daily), Some(global_daily)) = (
            parse_quota(&per_user.get_untracked()),
            parse_quota(&global.get_untracked()),
        ) else {
            Toasts::set_error(
                feedback,
                "Plafond invalide : un nombre entier, 0 pour aucune limite.",
            );
            return;
        };
        feedback.set(None);
        saving.set(true);
        leptos::task::spawn_local(async move {
            let body = AiQuotaSettings {
                per_user_daily,
                global_daily,
            };
            match api::put::<AiQuotaSettings, AiQuotaSettings>(AI_QUOTAS_PATH, &body).await {
                Ok(_) => {
                    Toasts::notify_success("Quotas enregistrés.");
                    on_saved.run(());
                }
                Err(err) => Toasts::set_error(feedback, err.message()),
            }
            saving.set(false);
        });
    };

    view! {
        <div class="stack-xs">
            <div class="cluster cluster-wrap">
                {match (&state.last_success_at, &state.last_latency_ms) {
                    (Some(at), latency) => view! {
                        <Tag label="Dernier appel réussi" tone=TagTone::Success />
                        <span class="text-mono">
                            {format!("{}{}", when(at), latency.map(|ms| format!(" · {ms} ms")).unwrap_or_default())}
                        </span>
                    }.into_any(),
                    (None, _) => view! {
                        <Tag label="Aucun appel réussi" tone=TagTone::Neutral />
                    }.into_any(),
                }}
            </div>
            {state.last_error.map(|error| view! {
                <p class="form-error" role="status">
                    {format!("Dernière erreur ({}) : {}", when(&error.at), error.message)}
                </p>
            })}
            <p class="form-help">
                {format!(
                    "Aujourd'hui : {} au total, {} pour vous.",
                    usage_label(state.requests_today, quota.global_daily),
                    usage_label(state.your_requests_today, quota.per_user_daily),
                )}
            </p>
            <DiagnosisToggle />
            <form class="settings-form" on:submit=submit>
                <div class="form-group">
                    <label for="ai-quota-user" class="form-label">"Questions par jour et par compte (0 : sans limite)"</label>
                    <input
                        id="ai-quota-user"
                        class="form-input text-mono"
                        inputmode="numeric"
                        prop:value=move || per_user.get()
                        on:input=move |ev| per_user.set(event_target_value(&ev))
                    />
                </div>
                <div class="form-group">
                    <label for="ai-quota-global" class="form-label">"Questions par jour pour toute l'installation (0 : sans limite)"</label>
                    <input
                        id="ai-quota-global"
                        class="form-input text-mono"
                        inputmode="numeric"
                        prop:value=move || global.get()
                        on:input=move |ev| global.set(event_target_value(&ev))
                    />
                </div>
                {move || feedback.get().map(|err| view! { <p class="form-error" role="alert">{err}</p> })}
                <div class="cluster-end">
                    <button type="submit" class="btn btn-primary" disabled=move || saving.get()>
                        {move || if saving.get() { "Enregistrement…" } else { "Enregistrer les quotas" }}
                    </button>
                </div>
            </form>
        </div>
    }
}

/// Active ou coupe le diagnostic proactif des échecs de tâches (appels au fournisseur en tâche de fond).
#[component]
fn DiagnosisToggle() -> impl IntoView {
    let enabled = RwSignal::new(false);
    let feedback = RwSignal::new(None::<String>);
    let loaded = LocalResource::new(move || async move {
        if let Ok(state) = api::get::<AiDiagnosis>(AI_DIAGNOSIS_PATH).await {
            enabled.set(state.enabled);
        }
    });
    let change = move |event: ev::Event| {
        let next = event_target_checked(&event);
        enabled.set(next);
        feedback.set(None);
        leptos::task::spawn_local(async move {
            let body = AiDiagnosisSettings { enabled: next };
            match api::put::<AiDiagnosisSettings, AiDiagnosis>(AI_DIAGNOSIS_PATH, &body).await {
                Ok(state) => enabled.set(state.enabled),
                Err(err) => {
                    enabled.set(!next);
                    Toasts::set_error(feedback, err.message());
                }
            }
        });
    };
    view! {
        <div class="form-group">
            <label class="flow-settings__toggle">
                <input
                    type="checkbox"
                    disabled=move || loaded.get().is_none()
                    prop:checked=move || enabled.get()
                    on:change=change
                />
                <span>"Diagnostic proactif des échecs de tâches (Activité)"</span>
            </label>
            <p class="form-help">
                "Désactivé par défaut. Activé, le central interroge le fournisseur IA au plus une fois par heure, et seulement s'il y a de nouveaux échecs ; ces appels comptent dans les quotas. La synthèse reste en mémoire et n'utilise que des outils en lecture."
            </p>
            {move || feedback.get().map(|err| view! { <p class="form-error" role="alert">{err}</p> })}
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotas_accept_whole_numbers_up_to_the_maximum_and_empty_means_open() {
        assert_eq!(parse_quota(""), Some(0));
        assert_eq!(parse_quota(" 25 "), Some(25));
        assert_eq!(parse_quota("0"), Some(0));
        assert_eq!(
            parse_quota(&MAX_AI_DAILY_QUOTA.to_string()),
            Some(MAX_AI_DAILY_QUOTA)
        );
        assert_eq!(parse_quota(&(MAX_AI_DAILY_QUOTA + 1).to_string()), None);
        assert_eq!(parse_quota("-3"), None);
        assert_eq!(parse_quota("beaucoup"), None);
    }

    #[test]
    fn usage_is_shown_against_its_limit_when_there_is_one() {
        assert_eq!(usage_label(12, 200), "12 / 200");
        assert_eq!(usage_label(12, 0), "12 (sans limite)");
    }

    #[test]
    fn dates_fall_back_to_the_raw_text_when_unreadable() {
        assert_eq!(when("pas une date"), "pas une date");
        assert!(when("2026-11-18T17:10:06Z").contains("2026"));
    }
}
