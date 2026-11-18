//! Fiche d'une tâche : onglet Information (cartes Détails / Traitement / Stockage + aperçu
//! des étapes) et onglet Étapes exécutées (journal détaillé).

use super::model::{
    error_label, format_size_delta, priority_label, resolve_library_label, short_digest,
    status_label, status_tone, step_status_label, step_status_tone,
};
use crate::ai::AiAssistantState;
use crate::ui::{Icon, IconName, StatusBadge, Tag, TagTone, Toasts};
use crate::{api, format};
use flux_shared::format::format_bytes;
use flux_shared::jobs::{Job, JobStatus, JobStepLog, MAX_ATTEMPTS, UpdateJobPriorityRequest};
use leptos::prelude::*;
use std::collections::HashMap;
use uuid::Uuid;

const ACTION_ICON_SIZE: u32 = 15;
const SHORT_ID_CHARS: usize = 8;

fn row(label: &'static str, value: String) -> impl IntoView {
    view! {
        <div class="detail-term">{label}</div>
        <div class="detail-val">{value}</div>
    }
}

fn mono_row(label: &'static str, value: String) -> impl IntoView {
    view! {
        <div class="detail-term">{label}</div>
        <div class="detail-val text-mono">{value}</div>
    }
}

fn optional(value: Option<String>) -> String {
    value.unwrap_or_else(|| "—".into())
}

fn format_output_summary(source_bytes: Option<i64>, output_bytes: Option<i64>) -> String {
    match output_bytes {
        Some(bytes) => {
            let size = format_bytes(bytes.unsigned_abs());
            match format_size_delta(source_bytes, Some(bytes)) {
                Some(delta) => format!("{size} ({delta})"),
                None => size,
            }
        }
        None => "—".into(),
    }
}

#[component]
pub fn JobDetail(
    job: Job,
    tab: RwSignal<usize>,
    libraries: Signal<HashMap<Uuid, String>>,
    workers: Signal<HashMap<Uuid, String>>,
    flows: Signal<HashMap<String, String>>,
    can_manage: bool,
    on_changed: Callback<Job>,
) -> impl IntoView {
    let feedback = RwSignal::new(None::<String>);
    let job_id = job.id;
    let steps = LocalResource::new(move || async move {
        let path = format!("/api/v1/jobs/{job_id}/steps");
        api::get::<Vec<JobStepLog>>(&path).await
    });
    let retry = Action::new_local(move |id: &uuid::Uuid| {
        let path = format!("/api/v1/jobs/{id}/retry");
        async move { api::post::<(), Job>(&path, None).await }
    });
    Effect::new(move |_| {
        if let Some(result) = retry.value().get() {
            match result {
                Ok(job) => {
                    Toasts::set_success(feedback, "Tâche remise en file.");
                    on_changed.run(job);
                }
                Err(error) => Toasts::set_error(feedback, error.message()),
            }
        }
    });
    let cancel = Action::new_local(move |id: &uuid::Uuid| {
        let path = format!("/api/v1/jobs/{id}/cancel");
        async move { api::post::<(), Job>(&path, None).await }
    });
    Effect::new(move |_| {
        if let Some(result) = cancel.value().get() {
            match result {
                Ok(job) => {
                    Toasts::set_success(feedback, "Tâche annulée.");
                    on_changed.run(job);
                }
                Err(error) => Toasts::set_error(feedback, error.message()),
            }
        }
    });
    let set_priority = Action::new_local(move |priority: &i32| {
        let path = format!("/api/v1/jobs/{job_id}/priority");
        let req = UpdateJobPriorityRequest {
            priority: *priority,
        };
        async move { api::put::<UpdateJobPriorityRequest, Job>(&path, &req).await }
    });
    Effect::new(move |_| {
        if let Some(result) = set_priority.value().get() {
            match result {
                Ok(job) => {
                    Toasts::set_success(feedback, "Priorité mise à jour.");
                    on_changed.run(job);
                }
                Err(error) => Toasts::set_error(feedback, error.message()),
            }
        }
    });

    let flow_id = job.flow_id.clone();
    let flow_version = job.flow_version;
    let flow_label = Signal::derive(move || match (&flow_id, flow_version) {
        (Some(id), Some(version)) => {
            let name = flows.get().get(id).cloned().unwrap_or_else(|| id.clone());
            format!("{name} (v{version})")
        }
        _ => "Aucun (opération par défaut)".into(),
    });
    let download = (job.status == JobStatus::Succeeded).then(|| {
        let href = format!("/api/v1/jobs/{}/output", job.id);
        let name = job.output_name.clone().unwrap_or_else(|| "resultat".into());
        view! {
            <a class="btn btn-secondary" href=href download=name>
                <Icon name=IconName::Download size=ACTION_ICON_SIZE />
                <span>"Télécharger le résultat"</span>
            </a>
        }
    });
    let retryable = can_manage && job.is_retryable();
    let cancelable = can_manage && job.is_cancelable();
    let can_prioritize = can_manage && job.status == JobStatus::Queued;
    let job_priority = job.priority;
    let short_id: String = job.id.to_string().chars().take(SHORT_ID_CHARS).collect();
    let diagnostic = job.error_code.map(error_label);
    let library_id = job.library_id;
    let library_name =
        Signal::derive(move || optional(resolve_library_label(library_id, &libraries.get())));
    let worker_id = job.worker_id;
    let worker_label = Signal::derive(move || match worker_id {
        Some(id) => workers
            .get()
            .get(&id)
            .cloned()
            .unwrap_or_else(|| id.to_string()),
        None => "—".into(),
    });
    let source_meta = match (job.source_bytes, job.source_sha256.as_deref()) {
        (Some(bytes), Some(_)) => format!(
            "{} · empreinte {}",
            format_bytes(bytes.unsigned_abs()),
            short_digest(job.source_sha256.as_deref())
        ),
        (Some(bytes), None) => format_bytes(bytes.unsigned_abs()),
        (None, Some(_)) => format!("empreinte {}", short_digest(job.source_sha256.as_deref())),
        (None, None) => "—".into(),
    };
    let output_size = format_output_summary(job.source_bytes, job.output_bytes);
    let output_sha = short_digest(job.output_sha256.as_deref());
    let output_meta = if output_size != "—" && output_sha != "—" {
        format!("{output_size} · empreinte {output_sha}")
    } else {
        output_size
    };
    let source_name = job.source_name.clone();
    let output_name = optional(job.output_name.clone());
    let created_at = format::datetime(job.created_at);
    let updated_at = format::datetime(job.updated_at);
    let attempts_label = format!("{} sur {MAX_ATTEMPTS}", job.attempts);
    let error_text = optional(job.error_code.map(|code| error_label(code).to_owned()));
    let job_status = job.status;
    let job_warning = job.warning;
    let id_text = job.id.to_string();
    let ai_id_text = id_text.clone();
    let ai_state = use_context::<AiAssistantState>();
    let ai_source = source_name.clone();
    let ai_short_id = short_id.clone();

    view! {
        <div class="inspection-panel">
            {move || match tab.get() {
                0 => view! {
                    <div class="inspection-banner">
                        <StatusBadge label=status_label(job_status) variant=status_tone(job_status) />
                        {job_warning.then(|| view! {
                            <Tag label="Avertissement / repli" tone=TagTone::Warning />
                        })}
                        <span class="text-mono text-xs text-dim">{format!("#{short_id}")}</span>
                    </div>
                    {diagnostic.map(|reason| view! {
                        <div class="diagnostic-box" role="alert">
                            <Icon name=IconName::Alert size=16 />
                            <div>
                                <div class="diagnostic-box__title">"Diagnostic d'échec"</div>
                                <div class="diagnostic-box__message">{reason}</div>
                            </div>
                        </div>
                    })}
                    <div class="inspection-panel__section">
                        <p class="inspection-panel__title">"Détails"</p>
                        <div class="detail-grid">
                            {mono_row("Nom", source_name.clone())}
                            <div class="detail-term">"Flow"</div>
                            <div class="detail-val">{flow_label}</div>
                            <div class="detail-term">"Bibliothèque"</div>
                            <div class="detail-val">{library_name}</div>
                            {mono_row("Identifiant", id_text.clone())}
                        </div>
                    </div>
                    <div class="inspection-panel__section">
                        <p class="inspection-panel__title">"Traitement"</p>
                        <div class="detail-grid">
                            <div class="detail-term">"Statut"</div>
                            <div class="detail-val">
                                <StatusBadge label=status_label(job_status) variant=status_tone(job_status) />
                            </div>
                            <div class="detail-term">"Worker"</div>
                            <div class="detail-val">{worker_label}</div>
                            <div class="detail-term">"Priorité"</div>
                            <div class="detail-val">
                                {if can_prioritize {
                                    view! {
                                        <select class="form-select"
                                            prop:value=move || job_priority.to_string()
                                            disabled=move || set_priority.pending().get()
                                            on:change=move |ev| {
                                                if let Ok(val) = event_target_value(&ev).parse::<i32>() {
                                                    set_priority.dispatch(val);
                                                }
                                            }>
                                            <option value="-10">"Basse (-10)"</option>
                                            <option value="0">"Normale (0)"</option>
                                            <option value="10">"Haute (+10)"</option>
                                            <option value="20">"Urgente (+20)"</option>
                                        </select>
                                    }.into_any()
                                } else {
                                    view! { <span>{priority_label(job_priority)}</span> }.into_any()
                                }}
                            </div>
                            {row("Tentatives", attempts_label.clone())}
                            {row("Erreur", error_text.clone())}
                        </div>
                    </div>
                    <div class="inspection-panel__section">
                        <p class="inspection-panel__title">"Stockage"</p>
                        <div class="detail-grid">
                            {mono_row("Fichier produit", output_name.clone())}
                            {row("Date de création", created_at.clone())}
                            {row("Dernière mise à jour", updated_at.clone())}
                            {row("Taille originale", source_meta.clone())}
                            {row("Taille produite", output_meta.clone())}
                        </div>
                    </div>
                    <div class="inspection-panel__section">
                        <p class="inspection-panel__title">"Étapes"</p>
                        {move || match steps.get() {
                            None => view! { <p class="text-dim text-sm">"Chargement des étapes…"</p> }.into_any(),
                            Some(Err(error)) => view! { <p class="form-error text-sm">{error.message()}</p> }.into_any(),
                            Some(Ok(list)) if list.is_empty() => {
                                view! { <p class="text-dim text-sm">"Aucune étape enregistrée pour cette tâche."</p> }.into_any()
                            }
                            Some(Ok(list)) => view! {
                                <div class="table-wrapper no-margin-top">
                                    <table class="data-table">
                                        <thead>
                                            <tr>
                                                <th>"#"</th>
                                                <th>"Nom / Bloc"</th>
                                                <th>"Élément de Flow"</th>
                                                <th>"Statut"</th>
                                            </tr>
                                        </thead>
                                        <tbody>
                                            {list.into_iter().map(|step| {
                                                let op = step.operation_id.unwrap_or_else(|| step.block_kind.clone());
                                                view! {
                                                    <tr>
                                                        <td class="text-mono">{format!("#{}", step.step_index + 1)}</td>
                                                        <td class="text-mono">{step.node_id}</td>
                                                        <td class="text-mono">{op}</td>
                                                        <td>
                                                            <StatusBadge
                                                                label=step_status_label(step.status)
                                                                variant=step_status_tone(step.status)
                                                            />
                                                        </td>
                                                    </tr>
                                                }
                                            }).collect_view()}
                                        </tbody>
                                    </table>
                                </div>
                            }
                            .into_any(),
                        }}
                    </div>
                }.into_any(),
                _ => view! {
                    <div class="inspection-panel__section">
                        <p class="inspection-panel__title">"Étapes exécutées"</p>
                        {move || match steps.get() {
                            None => view! { <p class="text-dim text-sm">"Chargement des étapes…"</p> }.into_any(),
                            Some(Err(error)) => view! { <p class="form-error text-sm">{error.message()}</p> }.into_any(),
                            Some(Ok(list)) if list.is_empty() => {
                                view! { <p class="text-dim text-sm">"Aucune étape enregistrée pour cette tâche."</p> }.into_any()
                            }
                            Some(Ok(list)) => view! {
                                <ol class="step-list">
                                    {list.into_iter().map(|step| view! {
                                        <li class="step-item">
                                            <div class="step-item__head">
                                                <span class="text-mono text-xs text-dim">{format!("#{}", step.step_index + 1)}</span>
                                                <Tag label=step.block_kind tone=TagTone::Mono />
                                                <StatusBadge
                                                    label=step_status_label(step.status)
                                                    variant=step_status_tone(step.status)
                                                />
                                                {step.operation_id.map(|op| view! {
                                                    <span class="text-mono text-xs text-dim">{op}</span>
                                                })}
                                            </div>
                                            <div class="step-item__meta text-xs text-dim">
                                                <span>{format!("Nœud : {}", step.node_id)}</span>
                                            </div>
                                            {step.message.map(|msg| view! {
                                                <p class="step-item__msg text-sm">{msg}</p>
                                            })}
                                            {step.output_excerpt.filter(|s| !s.trim().is_empty()).map(|excerpt| view! {
                                                <pre class="step-excerpt">{excerpt}</pre>
                                            })}
                                        </li>
                                    }).collect_view()}
                                </ol>
                            }
                            .into_any(),
                        }}
                    </div>
                }.into_any(),
            }}
            <p class="form-help" role="status">{move || feedback.get()}</p>
            <div class="inspection-drawer__actions">
                {ai_state.map(|ai| {
                    let prompt = format!(
                        "Diagnostique la tâche #{ai_short_id} ({ai_source}) avec `diagnose_job` : explique l'état, la cause probable d'échec ou d'avertissement et les actions recommandées."
                    );
                    let hint = Some(format!("Tâche {ai_id_text} — fichier {ai_source}"));
                    view! {
                        <button
                            type="button"
                            class="btn btn-secondary"
                            on:click=move |_| ai.ask(prompt.clone(), hint.clone())
                        >
                            <Icon name=IconName::Sparkles size=ACTION_ICON_SIZE />
                            <span>"Analyser avec l'IA"</span>
                        </button>
                    }
                })}
                {download}
                {retryable.then(|| view! {
                    <button
                        type="button"
                        class="btn btn-primary"
                        disabled=move || retry.pending().get()
                        on:click=move |_| { retry.dispatch(job_id); }
                    >
                        <Icon name=IconName::Refresh size=ACTION_ICON_SIZE />
                        <span>"Relancer"</span>
                    </button>
                })}
                {cancelable.then(|| view! {
                    <button
                        type="button"
                        class="btn btn-danger"
                        disabled=move || cancel.pending().get()
                        on:click=move |_| { cancel.dispatch(job_id); }
                    >
                        <span>"Annuler la tâche"</span>
                    </button>
                })}
            </div>
        </div>
    }
}
