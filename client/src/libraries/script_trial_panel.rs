//! Panneau « Essai » de l'éditeur de script (ADR 0031, lot 3c) : choix du worker, exemples de `config` et de
//! `vars.*`, puis le résultat du dernier essai (verdict, réponse interprétée, stdin, stdout et stderr).

use super::WORKERS_PATH;
use super::script_trial::{
    ScriptTrial, default_worker, object_text, response_fields, sample_vars, status_label, verdict,
    worker_choices,
};
use crate::api;
use crate::ui::Tag;
use chrono::Utc;
use forgearr_shared::script_trial::{ScriptOutput, ScriptTrialResult};
use forgearr_shared::script_variables::ScriptVariable;
use forgearr_shared::workers::Worker;
use leptos::prelude::*;
use uuid::Uuid;

/// Lignes visibles des zones de saisie JSON.
const JSON_INPUT_ROWS: u32 = 4;

#[component]
pub(super) fn ScriptTrialPanel(
    trial: ScriptTrial,
    /// Variables déclarées, ou l'erreur qui empêche de les lire.
    declared: Signal<Result<Vec<ScriptVariable>, String>>,
) -> impl IntoView {
    let workers = LocalResource::new(|| api::get::<Vec<Worker>>(WORKERS_PATH));
    let choices = Memo::new(move |_| {
        workers
            .get()
            .and_then(Result::ok)
            .map(|list| worker_choices(&list, Utc::now()))
            .unwrap_or_default()
    });
    // Un worker choisissable est proposé dès que la liste arrive.
    Effect::new(move |_| {
        let list = choices.get();
        let current = trial.worker.get_untracked();
        let still_available = list
            .iter()
            .any(|choice| Some(choice.id) == current && choice.available);
        if !still_available {
            trial.worker.set(default_worker(&list));
        }
    });
    let fill_vars = move |_| match declared.get_untracked() {
        Ok(variables) => trial.vars_text.set(object_text(&sample_vars(&variables))),
        Err(message) => trial.outcome.set(Some(Err(message))),
    };

    view! {
        <p class="form-help">
            "« Tester » exécute le code en cours d'édition (même non enregistré) sur le worker choisi, avec un fichier d'exemple vide. Aucun job ni historique n'est créé ; les secrets et réglages de plugin ne sont jamais envoyés."
        </p>
        <div class="form-group">
            <label class="form-label" for="trial-worker">"Worker"</label>
            <select id="trial-worker" class="form-input"
                prop:value=move || {
                    // Relu quand les options arrivent, pour que la sélection les suive.
                    choices.track();
                    trial.worker.get().map(|id| id.to_string()).unwrap_or_default()
                }
                on:change=move |e| trial.worker.set(Uuid::parse_str(&event_target_value(&e)).ok())>
                {move || {
                    let list = choices.get();
                    if list.is_empty() {
                        return view! { <option value="">"Aucun worker enregistré"</option> }.into_any();
                    }
                    list.into_iter().map(|choice| view! {
                        <option value=choice.id.to_string() disabled={!choice.available}>{choice.label}</option>
                    }).collect_view().into_any()
                }}
            </select>
        </div>
        <JsonField id="trial-config" label="Exemple de config" text=trial.config_text />
        <JsonField id="trial-vars" label="Exemple de vars.*" text=trial.vars_text />
        <button type="button" class="btn btn-secondary btn-sm" on:click=fill_vars>
            "Reprendre les valeurs par défaut des variables"
        </button>
        <div class="script-trial__result" role="status" aria-live="polite">
            {move || trial.pending().get().then(|| view! {
                <p class="text-dim text-sm">"Essai en cours…"</p>
            })}
            {move || trial.outcome.get().map(|outcome| match outcome {
                Ok(result) => view! { <TrialResult result=result /> }.into_any(),
                Err(message) => view! {
                    <p class="flow-workbench__feedback">{message}</p>
                }.into_any(),
            })}
        </div>
    }
}

/// Zone de saisie d'un objet JSON d'exemple.
#[component]
fn JsonField(id: &'static str, label: &'static str, text: RwSignal<String>) -> impl IntoView {
    view! {
        <div class="form-group">
            <label class="form-label" for=id>{label}</label>
            <textarea id=id class="form-input text-mono" rows=JSON_INPUT_ROWS spellcheck="false"
                prop:value=move || text.get()
                on:input=move |e| text.set(event_target_value(&e))></textarea>
        </div>
    }
}

#[component]
fn TrialResult(result: ScriptTrialResult) -> impl IntoView {
    let (verdict_label, verdict_tone) = verdict(&result);
    let duration = format!("{} ms", result.duration_ms);
    view! {
        <div class="cluster">
            <Tag label=verdict_label tone=verdict_tone />
            <Tag label=duration />
            {result.truncated.then(|| view! { <Tag label="Sortie tronquée à 64 Kio" /> })}
        </div>
        {result.launch_error.map(|text| view! { <p class="flow-workbench__feedback">{text}</p> })}
        <p class="inspection-panel__title">"Réponse interprétée"</p>
        {match (result.response, result.response_error) {
            (Some(output), _) => view! { <TrialResponse output=output /> }.into_any(),
            (None, Some(error)) => view! { <p class="flow-workbench__feedback">{error}</p> }.into_any(),
            (None, None) => view! {
                <p class="text-dim text-sm">"stdout vide : le script n'a renvoyé aucune réponse JSON."</p>
            }.into_any(),
        }}
        <TrialStream title="stdout" text=result.stdout open=true />
        <TrialStream title="stderr" text=result.stderr open=true />
        <TrialStream title="stdin envoyé" text=result.stdin open=false />
    }
}

#[component]
fn TrialResponse(output: ScriptOutput) -> impl IntoView {
    let (status, tone) = status_label(output.status);
    view! {
        <dl class="script-trial__response">
            <dt>"status"</dt>
            <dd><Tag label=status tone=tone /></dd>
            {response_fields(&output).into_iter().map(|(name, value)| view! {
                <dt>{name}</dt>
                <dd><pre class="script-trial__stream">{value}</pre></dd>
            }).collect_view()}
        </dl>
    }
}

/// Un flux capturé, repliable ; un flux vide le dit.
#[component]
fn TrialStream(title: &'static str, text: String, open: bool) -> impl IntoView {
    let empty = text.is_empty();
    view! {
        <details class="script-trial__details" open={open && !empty}>
            <summary class="script-trial__summary">
                {title}{empty.then_some(" (vide)")}
            </summary>
            <pre class="script-trial__stream">{text}</pre>
        </details>
    }
}
