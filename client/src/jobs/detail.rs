//! Fiche d'une tâche : état, erreur, flux figé, téléchargement et relance manuelle.

use super::model::{error_label, status_label, status_tone};
use crate::ui::{Icon, IconName, StatusBadge, Toasts};
use crate::{api, format};
use flux_shared::format::format_bytes;
use flux_shared::jobs::{Job, JobStatus, MAX_ATTEMPTS};
use leptos::prelude::*;

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

#[component]
pub fn JobDetail(job: Job, can_manage: bool, on_changed: Callback<Job>) -> impl IntoView {
    let feedback = RwSignal::new(None::<String>);
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
    let job_id = job.id;
    let short_id: String = job.id.to_string().chars().take(SHORT_ID_CHARS).collect();
    let diagnostic = job.error_code.map(error_label);

    view! {
        <div class="inspection-panel">
            <div class="inspection-banner">
                <StatusBadge label=status_label(job.status) variant=status_tone(job.status) />
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
                {row("Tentatives", format!("{} sur {MAX_ATTEMPTS}", job.attempts))}
                {row("Erreur", optional(job.error_code.map(|code| error_label(code).to_owned())))}
                {row("Flux", flow)}
                {row("Worker", optional(job.worker_id.map(|id| id.to_string())))}
                {row("Source", optional(job.source_bytes.map(|bytes| format_bytes(bytes.unsigned_abs()))))}
                {row("Résultat", optional(job.output_name.clone()))}
                {row("Créée", format::datetime(job.created_at))}
                {row("Mise à jour", format::datetime(job.updated_at))}
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
