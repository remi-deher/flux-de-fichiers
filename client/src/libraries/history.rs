use crate::{
    api, format,
    jobs::{
        JobDetail,
        model::{error_label, status_label},
    },
};
use forgearr_shared::{
    flows::FlowVersionSummary,
    jobs::{Job, JobStatus},
    libraries::{
        IgnoredScanFile, LIBRARY_HISTORY_PAGE_SIZE, Library, LibraryFileHistory,
        ReprocessLibraryFile, ScanIgnoreReason,
    },
    workers::Worker,
};
use leptos::prelude::*;
use uuid::Uuid;

#[derive(Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ScanFeedback {
    created_jobs: usize,
    waiting_for_stability: usize,
    unavailable_folders: Vec<String>,
    pending_files: Vec<String>,
    ignored_files: Vec<IgnoredScanFile>,
    skipped_existing: usize,
}

#[component]
pub(super) fn LibraryHistory(
    library: Library,
    can_scan: bool,
    can_manage_jobs: bool,
    workers: Vec<Worker>,
) -> impl IntoView {
    let id = library.id;
    let worker_names = StoredValue::new(
        workers
            .into_iter()
            .map(|worker| (worker.id, worker.name))
            .collect::<std::collections::HashMap<_, _>>(),
    );
    let offset = RwSignal::new(0_usize);
    let reload = RwSignal::new(0_u32);
    let selected = RwSignal::new(None::<Job>);
    let tab = RwSignal::new(0_usize);
    let files = LocalResource::new(move || {
        let offset = offset.get();
        reload.track();
        async move {
            api::get::<Vec<LibraryFileHistory>>(&format!(
                "/api/v1/libraries/{id}/files?offset={offset}"
            ))
            .await
        }
    });
    let scan = Action::new_local(move |_: &()| async move {
        api::post::<(), ScanFeedback>(&format!("/api/v1/libraries/{id}/scan"), None).await
    });
    Effect::new(move |_| {
        if matches!(scan.value().get(), Some(Ok(_))) {
            offset.set(0);
            super::bump(reload);
        }
    });
    view! {
        {can_scan.then(|| view! {
            <div class="cluster">
                <button type="button" class="btn btn-secondary" disabled=move || scan.pending().get()
                    on:click=move |_| { scan.dispatch(()); }>"Scanner les dossiers"</button>
                <p role="status">
                    {move || match scan.value().get() {
                        Some(Ok(report)) => format!("{} tâche(s) créée(s), {} fichier(s) en cours d’arrivée, {} déjà connus, {} dossier(s) indisponible(s){}",
                            report.created_jobs, report.waiting_for_stability, report.skipped_existing, report.unavailable_folders.len(),
                            if report.unavailable_folders.is_empty() { String::new() } else { format!(" · {}", report.unavailable_folders.join(", ")) }),
                        Some(Err(error)) => error.message(),
                        None => String::new(),
                    }}
                </p>
            </div>
            {move || scan.value().get().and_then(Result::ok).filter(|scan| !scan.pending_files.is_empty()).map(|scan| view! {
                <details>
                    <summary>"Fichiers en attente de stabilité"</summary>
                    <ul>{scan.pending_files.into_iter().map(|path| view! { <li class="text-mono">{path}</li> }).collect_view()}</ul>
                </details>
            })}
            {move || scan.value().get().and_then(Result::ok).filter(|scan| !scan.ignored_files.is_empty()).map(|scan| view! {
                <details>
                    <summary>"Exemples de fichiers ignorés"</summary>
                    <ul>{scan.ignored_files.into_iter().map(|file| {
                        let reason = match file.reason {
                            ScanIgnoreReason::FileSize => "Taille non autorisée",
                            ScanIgnoreReason::LibraryFilter => "Filtres de la bibliothèque",
                        };
                        view! { <li>{file.path} " · " {reason}</li> }
                    }).collect_view()}</ul>
                </details>
            })}
        })}
        {move || match files.get() {
            None => view! { <p>"Chargement…"</p> }.into_any(),
            Some(Err(error)) => view! { <p role="status">{error.message()}</p> }.into_any(),
            Some(Ok(files)) if files.is_empty() => view! { <p>"Aucun fichier traité sur cette page."</p> }.into_any(),
            Some(Ok(files)) => files.into_iter().filter(|file| !file.jobs.is_empty()).map(|file| {
                let name = file.jobs[0].source_name.clone();
                let root = file.source_root.unwrap_or_else(|| "Entrée de la centrale".to_owned());
                let host = file.source_worker_id.map_or_else(|| "Centrale".to_owned(), |id| {
                    worker_names.read_value().get(&id).cloned().unwrap_or_else(|| id.to_string())
                });
                let location = format!("{host} · {root}");
                view! {
                    <details class="inspection-panel__section">
                        <summary>{name} " · " {status_label(file.jobs[0].status)}</summary>
                        <p class="text-mono">{location}</p>
                        <ul class="library-worker-list">
                        {file.jobs.into_iter().map(|job| {
                            let label = format!("{} · {}{}", format::datetime(job.created_at), status_label(job.status),
                                job.error_code.map_or(String::new(), |code| format!(" · {}", error_label(code))));
                            view! {
                                <li>
                                    <button type="button" class="btn btn-secondary btn-sm"
                                        on:click=move |_| { tab.set(0); selected.set(Some(job.clone())); }>{label}</button>
                                </li>
                            }
                        }).collect_view()}
                        </ul>
                    </details>
                }
            }).collect_view().into_any(),
        }}
        <div class="cluster">
            <button type="button" class="btn btn-secondary btn-sm" disabled=move || offset.get() == 0
                on:click=move |_| offset.update(|value| *value = value.saturating_sub(LIBRARY_HISTORY_PAGE_SIZE))>"Précédent"</button>
            <button type="button" class="btn btn-secondary btn-sm"
                disabled=move || !files.get().is_some_and(|page| page.is_ok_and(|files| files.len() == LIBRARY_HISTORY_PAGE_SIZE))
                on:click=move |_| offset.update(|value| *value += LIBRARY_HISTORY_PAGE_SIZE)>"Suivant"</button>
        </div>
        {move || selected.get().map(|job| {
            let job_id = job.id;
            let completed = !matches!(job.status, JobStatus::Queued | JobStatus::Running);
            let folder = library.name.clone();
            view! {
                <h3>{job.source_name.clone()}</h3>
                { (can_manage_jobs && completed).then(|| view! {
                    <Reprocess library_id=id job=job.clone() reload=reload />
                })}
                <JobDetail job=job tab=tab
                    libraries=Signal::derive(move || [(id, folder.clone())].into_iter().collect())
                    workers=Signal::derive(move || worker_names.get_value())
                    flows=Signal::derive(Default::default)
                    can_manage=false
                    on_changed=Callback::new(move |job: Job| {
                        if job.id == job_id { selected.set(Some(job)); }
                        super::bump(reload);
                    })
                />
            }
        })}
    }
}

#[component]
fn Reprocess(library_id: Uuid, job: Job, reload: RwSignal<u32>) -> impl IntoView {
    let job_id = job.id;
    let selected = RwSignal::new(job.flow_version);
    let flow = StoredValue::new(job.flow_id);
    let versions = LocalResource::new(move || async move {
        let Some(flow) = flow.get_value() else {
            return Ok(Vec::<FlowVersionSummary>::new());
        };
        api::get(&format!(
            "/api/v1/flows/{}/versions",
            crate::route::encode_segment(&flow)
        ))
        .await
    });
    let submit = Action::new_local(move |version: &Option<i64>| {
        let request = ReprocessLibraryFile {
            flow_version: *version,
        };
        async move {
            api::post::<_, Job>(
                &format!("/api/v1/libraries/{library_id}/files/{job_id}/reprocess"),
                Some(&request),
            )
            .await
        }
    });
    Effect::new(move |_| {
        if matches!(submit.value().get(), Some(Ok(_))) {
            super::bump(reload);
        }
    });
    view! {
        <div class="cluster">
            {move || versions.get().and_then(Result::ok).filter(|list| !list.is_empty()).map(|list| view! {
                <label>"Version du flux"
                    <select class="form-select" prop:value=move || selected.get().map(|v| v.to_string()).unwrap_or_default()
                        on:change=move |event| selected.set(event_target_value(&event).parse().ok())>
                        {list.into_iter().map(|version| view! {
                            <option value=version.version.to_string()>{format!("v{}", version.version)}</option>
                        }).collect_view()}
                    </select>
                </label>
            })}
            <button type="button" class="btn btn-secondary" disabled=move || submit.pending().get()
                on:click=move |_| { submit.dispatch(selected.get_untracked()); }>"Retraiter le fichier"</button>
            <p role="status">{move || submit.value().get().map(|result| match result {
                Ok(_) => "Nouveau traitement mis en file.".to_owned(),
                Err(error) => error.message(),
            })}</p>
        </div>
    }
}
