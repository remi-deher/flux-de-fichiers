//! Fiche d'une tâche : état, erreur, métadonnées entrée/sortie, étapes exécutées et relance.

use super::model::{
    error_label, format_size_delta, resolve_library_label, short_digest, status_label, status_tone,
    step_status_label, step_status_tone,
};
use crate::ui::{Icon, IconName, StatusBadge, Tag, TagTone, Toasts};
use crate::{api, format};
use flux_shared::format::format_bytes;
use flux_shared::jobs::{Job, JobStatus, JobStepLog, MAX_ATTEMPTS};
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
    libraries: Signal<HashMap<Uuid, String>>,
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

    let flow = match (&job.flow_id, job.flow_version) {
        (Some(id), Some(version)) => format!("{id} (version {version})"),
        _ => "Aucun (remux simple)".into(),
    };
    let download = (job.status == JobStatus::Succeeded).then(|| {
        let href = format!("/api/v1/jobs/{}/output", job.id);
        let name = job
            .output_name
            .clone()
            .unwrap_or_else(|| "resultat.mkv".into());
        view! {
            <a class="btn btn-secondary" href=href download=name>
                <Icon name=IconName::Download size=ACTION_ICON_SIZE />
                <span>"Télécharger le résultat"</span>
            </a>
        }
    });
    let retryable = can_manage && job.is_retryable();
    let short_id: String = job.id.to_string().chars().take(SHORT_ID_CHARS).collect();
    let diagnostic = job.error_code.map(error_label);
    let library_id = job.library_id;
    let library_name = move || optional(resolve_library_label(library_id, &libraries.get()));
    let source_meta = match (job.source_bytes, job.source_sha256.as_deref()) {
        (Some(bytes), Some(_)) => format!(
            "{} · sha256 {}",
            format_bytes(bytes.unsigned_abs()),
            short_digest(job.source_sha256.as_deref())
        ),
        (Some(bytes), None) => format_bytes(bytes.unsigned_abs()),
        (None, Some(_)) => format!("sha256 {}", short_digest(job.source_sha256.as_deref())),
        (None, None) => "—".into(),
    };
    let output_size = format_output_summary(job.source_bytes, job.output_bytes);
    let output_sha = short_digest(job.output_sha256.as_deref());

    view! {
        <div class="inspection-panel">
            <div class="inspection-banner">
                <StatusBadge label=status_label(job.status) variant=status_tone(job.status) />
                {job.warning.then(|| view! {
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
            <div class="detail-grid">
                {row("Identifiant", job.id.to_string())}
                <div class="detail-term">"Bibliothèque"</div>
                <div class="detail-val">{library_name}</div>
                {row("Tentatives", format!("{} sur {MAX_ATTEMPTS}", job.attempts))}
                {row("Erreur", optional(job.error_code.map(|code| error_label(code).to_owned())))}
                {row("Flux", flow)}
                {row("Worker", optional(job.worker_id.map(|id| id.to_string())))}
                {row("Source", source_meta)}
                {row("Fichier produit", optional(job.output_name.clone()))}
                {row("Taille produite", output_size)}
                {row("SHA-256 produit", output_sha)}
                {row("Créée", format::datetime(job.created_at))}
                {row("Mise à jour", format::datetime(job.updated_at))}
            </div>
            <div class="pack-components">
                <h3 class="form-label">"Étapes exécutées"</h3>
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
            <div class="inspection-drawer__actions">
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
            </div>
            <p class="form-help" role="status">{move || feedback.get()}</p>
        </div>
    }
}
