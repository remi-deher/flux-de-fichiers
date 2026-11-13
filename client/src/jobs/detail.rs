//! Fiche d'une tâche : état, erreur, flux figé, téléchargement et relance manuelle.

use super::model::{error_label, status_label, status_tone};
use crate::{api, format};
use flux_shared::format::format_bytes;
use flux_shared::jobs::{Job, JobStatus, MAX_ATTEMPTS};
use leptos::prelude::*;

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
                    feedback.set(Some("Tâche remise en file.".into()));
                    on_changed.run(job);
                }
                Err(error) => feedback.set(Some(error.message())),
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
        view! { <a class="btn btn-secondary" href=href download=name>"Télécharger le résultat"</a> }
    });
    let retryable = can_manage && job.is_retryable();
    let job_id = job.id;

    view! {
        <aside class="section-panel" aria-labelledby="job-detail-title">
            <div class="section-panel-header">
                <h2 id="job-detail-title" class="section-title">{job.source_name.clone()}</h2>
                <span class=format!("status-pill {}", status_tone(job.status))>{status_label(job.status)}</span>
            </div>
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
            <div class="inline-actions">
                {download}
                {retryable.then(|| view! {
                    <button
                        type="button"
                        class="btn btn-primary"
                        disabled=move || retry.pending().get()
                        on:click=move |_| { retry.dispatch(job_id); }
                    >
                        "Relancer"
                    </button>
                })}
            </div>
            <p class="form-help" role="status">{move || feedback.get()}</p>
        </aside>
    }
}
