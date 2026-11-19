//! Carte « Diagnostic IA » de l'écran Activité : synthèse des échecs récents (si activée).

use crate::ai::render_blocks;
use crate::api;
use crate::format;
use crate::ui::{SectionHeader, Toasts};
use chrono::DateTime;
use forgearr_shared::ai::{AiDiagnosis, AiDiagnosisSummary};
use leptos::prelude::*;

const DIAGNOSIS_PATH: &str = "/api/v1/ai/diagnosis";
const REFRESH_PATH: &str = "/api/v1/ai/diagnosis/refresh";

/// « 3 échecs analysés · calculé le … ».
fn caption(summary: &AiDiagnosisSummary) -> String {
    let when = DateTime::parse_from_rfc3339(&summary.generated_at)
        .map(|date| format::datetime(date.to_utc()))
        .unwrap_or_else(|_| summary.generated_at.clone());
    let failures = match summary.failures {
        0 => "aucun échec".to_owned(),
        1 => "1 échec analysé".to_owned(),
        count => format!("{count} échecs analysés"),
    };
    format!("{failures} sur 24 h · calculé le {when}")
}

/// Rien n'est affiché tant que l'administrateur n'a pas activé le diagnostic proactif.
#[component]
pub fn AiDiagnosisCard(can_refresh: bool) -> impl IntoView {
    let reload = RwSignal::new(0u64);
    let feedback = RwSignal::new(None::<String>);
    let busy = RwSignal::new(false);
    let diagnosis = LocalResource::new(move || {
        reload.track();
        async move { api::get::<AiDiagnosis>(DIAGNOSIS_PATH).await }
    });
    let refresh = move |_| {
        busy.set(true);
        feedback.set(None);
        leptos::task::spawn_local(async move {
            match api::post::<(), AiDiagnosis>(REFRESH_PATH, None).await {
                Ok(_) => reload.update(|n| *n = n.wrapping_add(1)),
                Err(err) => Toasts::set_error(feedback, err.message()),
            }
            busy.set(false);
        });
    };

    view! {
        {move || match diagnosis.get() {
            Some(Ok(state)) if state.enabled => {
                let body = match state.summary {
                    Some(summary) => view! {
                        <p class="form-help">{caption(&summary)}</p>
                        <div class="ai-chat__msg-body">{render_blocks(&summary.text)}</div>
                    }.into_any(),
                    None => view! {
                        <p class="form-help">
                            "Pas encore de synthèse : elle est calculée dès qu'un échec récent est détecté (au plus une fois par heure)."
                        </p>
                    }.into_any(),
                };
                let actions = can_refresh.then(|| view! {
                    <button
                        type="button"
                        class="btn btn-secondary btn-sm"
                        disabled=move || busy.get()
                        on:click=refresh
                    >
                        {move || if busy.get() { "Analyse…" } else { "Actualiser" }}
                    </button>
                }.into_any());
                view! {
                    <section class="section-panel">
                        <SectionHeader
                            title="Diagnostic IA des échecs"
                            subtitle="Synthèse automatique des tâches en échec, par cause probable."
                            actions=actions.unwrap_or_else(|| ().into_any())
                        />
                        {body}
                        {move || feedback.get().map(|err| view! { <p class="form-error" role="alert">{err}</p> })}
                    </section>
                }.into_any()
            }
            _ => ().into_any(),
        }}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn summary(failures: u32) -> AiDiagnosisSummary {
        AiDiagnosisSummary {
            text: "x".into(),
            generated_at: "2026-11-18T17:10:06Z".into(),
            failures,
        }
    }

    #[test]
    fn the_caption_counts_failures_in_plain_french() {
        assert!(caption(&summary(0)).starts_with("aucun échec"));
        assert!(caption(&summary(1)).starts_with("1 échec analysé"));
        assert!(caption(&summary(7)).starts_with("7 échecs analysés"));
        assert!(caption(&summary(2)).contains("2026"));
    }
}
