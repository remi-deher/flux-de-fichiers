//! Formulaire d'une bibliothèque : nom, flux et chemins par worker avec sélecteur d'arborescence.

use super::draft::{LibraryDraft, access_label};
use super::transfer_workers::TransferWorkersSection;
use crate::api;
use crate::plugins::{FlowMiniPreview, preview_from_definition_json};
use crate::ui::FolderTreePicker;
use forgearr_shared::flows::FlowRecord;
use forgearr_shared::libraries::{Library, LibraryAccess, LibraryOutputPolicy, LibraryWrite};
use forgearr_shared::workers::Worker;
use leptos::prelude::*;
use uuid::Uuid;

const LIBRARIES_PATH: &str = "/api/v1/libraries";

async fn save(id: Option<Uuid>, body: LibraryWrite) -> Result<Library, api::ApiError> {
    match id {
        Some(id) => api::put(&format!("{LIBRARIES_PATH}/{id}"), &body).await,
        None => api::post(LIBRARIES_PATH, Some(&body)).await,
    }
}

fn access_value(access: LibraryAccess) -> &'static str {
    match access {
        LibraryAccess::Read => "read",
        LibraryAccess::ReadWrite => "read_write",
    }
}

fn parse_access(raw: &str) -> LibraryAccess {
    if raw == "read" {
        LibraryAccess::Read
    } else {
        LibraryAccess::ReadWrite
    }
}

#[component]
pub fn LibraryForm(
    draft: LibraryDraft,
    flows: Vec<FlowRecord>,
    workers: Vec<Worker>,
    on_done: Callback<()>,
) -> impl IntoView {
    let draft = RwSignal::new(draft);
    let feedback = RwSignal::new(None::<String>);
    let new_worker = RwSignal::new(Uuid::nil().to_string());
    let new_path = RwSignal::new(String::new());
    let source_key = RwSignal::new(String::new());
    let new_access = RwSignal::new(LibraryAccess::ReadWrite);
    let detected_writable = RwSignal::new(None::<bool>);
    let browse_tick = RwSignal::new(0_u32);

    let worker_names = StoredValue::new(
        workers
            .iter()
            .map(|worker| (worker.id, worker.name.clone()))
            .collect::<Vec<_>>(),
    );
    let worker_name = move |id: Uuid| {
        if id.is_nil() {
            return "Centrale".to_owned();
        }
        worker_names
            .read_value()
            .iter()
            .find(|(candidate, _)| *candidate == id)
            .map_or_else(|| id.to_string(), |(_, name)| name.clone())
    };
    let selected_worker_id = Signal::derive(move || new_worker.get().parse::<Uuid>().ok());
    let browse_signal = Signal::derive(move || browse_tick.get());
    let submit = Action::new_local(move |(id, body): &(Option<Uuid>, LibraryWrite)| {
        let (id, body) = (*id, body.clone());
        async move { save(id, body).await }
    });
    let remove = Action::new_local(move |id: &Uuid| {
        let path = format!("{LIBRARIES_PATH}/{id}");
        async move { api::delete(&path).await }
    });
    Effect::new(move |_| match submit.value().get() {
        Some(Ok(_)) => on_done.run(()),
        Some(Err(error)) => feedback.set(Some(error.message())),
        None => {}
    });
    Effect::new(move |_| match remove.value().get() {
        Some(Ok(())) => on_done.run(()),
        Some(Err(error)) => feedback.set(Some(error.message())),
        None => {}
    });

    let on_worker_change = move |raw: String| {
        new_worker.set(raw);
        detected_writable.set(None);
        feedback.set(None);
        new_path.set(String::new());
        new_access.set(LibraryAccess::ReadWrite);
    };

    let on_tree_select = Callback::new(move |(path, writable): (String, bool)| {
        new_path.set(path.clone());
        detected_writable.set(Some(writable));
        let next_access = if writable {
            LibraryAccess::ReadWrite
        } else {
            LibraryAccess::Read
        };
        new_access.set(next_access);
        draft.update(|value| value.apply_suggested_name(&path));
        feedback.set(None);
    });

    let add_worker = move |_| {
        let Ok(worker_id) = new_worker.get_untracked().parse::<Uuid>() else {
            feedback.set(Some("Choisissez un hôte.".into()));
            return;
        };
        let path_value = new_path.get_untracked();
        let access_value_sel = new_access.get_untracked();
        let mut result = Ok(());
        draft.update(|value| {
            value.apply_suggested_name(&path_value);
            result = value.set_or_add_worker(worker_id, &path_value, access_value_sel);
            value.set_source_key(worker_id, &path_value, &source_key.get_untracked());
        });
        match result {
            Ok(()) => {
                feedback.set(None);
                detected_writable.set(None);
                new_path.set(String::new());
            }
            Err(message) => feedback.set(Some(message.into())),
        }
    };

    let on_submit = move |event: leptos::ev::SubmitEvent| {
        event.prevent_default();
        let pending_worker = new_worker.get_untracked().parse::<Uuid>().ok();
        let pending_path = new_path.get_untracked();
        let pending_access = new_access.get_untracked();
        let pending_key = source_key.get_untracked();
        let mut commit_result = Ok(());
        draft.update(|value| {
            value.apply_suggested_name(&pending_path);
            commit_result =
                value.commit_pending_worker(pending_worker, &pending_path, pending_access);
            if let Some(host) = pending_worker {
                value.set_source_key(host, &pending_path, &pending_key);
            }
        });
        if let Err(message) = commit_result {
            feedback.set(Some(message.into()));
            return;
        }
        let current = draft.get_untracked();
        match current.to_write() {
            Ok(body) => {
                submit.dispatch((current.id, body));
            }
            Err(message) => feedback.set(Some(message.into())),
        }
    };
    let existing_id = draft.get_untracked().id;
    let flows_for_preview = StoredValue::new(flows.clone());
    let transfer_candidates = workers.clone();

    view! {
        <form class="inspection-panel" on:submit=on_submit>
            <div class="inspection-panel__section">
                <div class="form-group">
                    <label for="library-name" class="form-label">"Nom"</label>
                    <input id="library-name" class="form-input" maxlength="80"
                        placeholder="Ex. Films 4K"
                        prop:value=move || draft.get().name
                        on:input=move |event| draft.update(|value| value.name = event_target_value(&event)) />
                </div>
                <div class="form-group">
                    <label for="library-flow" class="form-label">"Flux"</label>
                    <select id="library-flow" class="form-select"
                        prop:value=move || draft.get().flow_id.unwrap_or_default()
                        on:change=move |event| draft.update(|value| {
                            let id = event_target_value(&event);
                            value.flow_id = (!id.is_empty()).then_some(id);
                        })>
                        <option value="">"Aucun (pas de traitement automatique)"</option>
                        {flows.into_iter().map(|flow| view! {
                            <option value=flow.id.clone()>{format!("{} (v{})", flow.name, flow.version)}</option>
                        }).collect_view()}
                    </select>
                    {move || {
                        let selected = draft.get().flow_id?;
                        let preview = flows_for_preview.with_value(|list| {
                            list.iter()
                                .find(|f| f.id == selected)
                                .and_then(|f| preview_from_definition_json(&f.definition))
                        })?;
                        Some(view! { <FlowMiniPreview preview=preview /> })
                    }}
                </div>
                <div class="form-group">
                    <label for="library-scan-interval" class="form-label">"Scan automatique"</label>
                    <select id="library-scan-interval" class="form-select"
                        prop:value=move || draft.get().scan_interval_minutes.to_string()
                        on:change=move |event| draft.update(|value| {
                            if let Ok(mins) = event_target_value(&event).parse::<i32>() {
                                value.scan_interval_minutes = mins;
                            }
                        })>
                        <option value="0">"Manuel uniquement"</option>
                        <option value="5">"Toutes les 5 minutes"</option>
                        <option value="15">"Toutes les 15 minutes"</option>
                        <option value="60">"Toutes les heures"</option>
                        <option value="360">"Toutes les 6 heures"</option>
                        <option value="1440">"Une fois par jour"</option>
                    </select>
                </div>
                <div class="form-group">
                    <label for="library-output-policy" class="form-label">"Stratégie de sortie"</label>
                    <select id="library-output-policy" class="form-select"
                        prop:value=move || draft.get().output_policy.as_str().to_owned()
                        on:change=move |event| draft.update(|value| {
                            if let Some(policy) = LibraryOutputPolicy::parse(&event_target_value(&event)) {
                                value.output_policy = policy;
                            }
                        })>
                        <option value="output_dir">"Dossier de sortie dédié (défaut)"</option>
                        <option value="replace_with_backup">"Remplacer la source (avec sauvegarde .bak)"</option>
                        <option value="replace_in_place">"Remplacer la source directement"</option>
                    </select>
                </div>
                <div class="form-group">
                    <label for="library-include-ext" class="form-label">"Extensions incluses (facultatif)"</label>
                    <input id="library-include-ext" class="form-input"
                        placeholder="Ex. mkv, mp4, avi (vide = toutes)"
                        prop:value=move || draft.get().include_extensions_text
                        on:input=move |event| draft.update(|value| value.include_extensions_text = event_target_value(&event)) />
                </div>
                <div class="form-group">
                    <label for="library-exclude-pat" class="form-label">"Motifs exclus (facultatif)"</label>
                    <input id="library-exclude-pat" class="form-input"
                        placeholder="Ex. *sample*, trailer, *.part"
                        prop:value=move || draft.get().exclude_patterns_text
                        on:input=move |event| draft.update(|value| value.exclude_patterns_text = event_target_value(&event)) />
                </div>
                <div class="form-group">
                    <label for="library-min-size" class="form-label">"Taille minimale (Mo)"</label>
                    <input id="library-min-size" type="number" min="0" class="form-input"
                        prop:value=move || draft.get().min_file_mb.to_string()
                        on:input=move |event| draft.update(|value| {
                            if let Ok(mb) = event_target_value(&event).parse::<u64>() {
                                value.min_file_mb = mb;
                            }
                        }) />
                </div>
            </div>

            <div class="inspection-panel__section">
                <p class="inspection-panel__title">"Dossiers de la bibliothèque"</p>
                <ul class="library-worker-list">
                {move || {
                    let entries = draft.get().workers;
                    if entries.is_empty() {
                        return view! {
                            <li class="text-dim text-sm">
                                "Aucun dossier déclaré. Le dossier d’entrée de la centrale est utilisé."
                            </li>
                        }.into_any();
                    }
                    entries.into_iter().map(|entry| {
                        let id = entry.worker_id;
                        let name = worker_name(id);
                        let row_path = entry.local_path.clone();
                        let access_path = row_path.clone();
                        let remove_path = row_path.clone();
                        let row_access = entry.access;
                        let row_key = entry.source_key.unwrap_or_default();
                        let row_label = row_key.clone();
                        view! {
                            <li class="library-worker-row">
                                <span class="text-strong">{name.clone()}</span>
                                <span class="library-worker-path">{entry.local_path}{(!row_label.is_empty()).then(|| format!(" · Source {row_label}"))}</span>
                                <div class="library-worker-row__controls">
                                    <select
                                        class="form-select library-worker-access-select"
                                        aria-label=format!("Accès à {access_path} sur {name}")
                                        prop:value=access_value(row_access)
                                        on:change=move |event| {
                                            let next = parse_access(&event_target_value(&event));
                                            draft.update(|value| { let _ = value.set_or_add_worker(id, &access_path, next); });
                                        }
                                    >
                                        <option value="read_write">{access_label(LibraryAccess::ReadWrite)}</option>
                                        <option value="read">{access_label(LibraryAccess::Read)}</option>
                                    </select>
                                    <button
                                        type="button"
                                        class="btn btn-secondary btn-sm"
                                        on:click=move |_| {
                                            new_path.set(row_path.clone());
                                            source_key.set(row_key.clone());
                                            new_access.set(row_access);
                                            new_worker.set(id.to_string());
                                            detected_writable.set(None);
                                            browse_tick.update(|v| *v = v.wrapping_add(1));
                                            feedback.set(None);
                                        }
                                    >
                                        "Parcourir"
                                    </button>
                                    <button
                                        type="button"
                                        class="btn btn-secondary btn-sm"
                                        on:click=move |_| {
                                            draft.update(|value| value.remove_folder(id, &remove_path));
                                            if new_worker.get_untracked() == id.to_string() && new_path.get_untracked() == remove_path {
                                                new_path.set(String::new());
                                                detected_writable.set(None);
                                            }
                                        }
                                    >
                                        "Retirer"
                                    </button>
                                </div>
                            </li>
                        }
                    }).collect_view().into_any()
                }}
                </ul>
            </div>

            <TransferWorkersSection draft=draft workers=transfer_candidates />

            <div class="library-worker-editor">
                <p class="inspection-panel__title">"Ajouter un dossier"</p>
                <div class="form-group">
                    <label for="library-worker" class="form-label">"Hôte"</label>
                    <select id="library-worker" class="form-select"
                        prop:value=move || new_worker.get()
                        on:change=move |event| on_worker_change(event_target_value(&event))>
                        <option value=Uuid::nil().to_string()>"Centrale"</option>
                        {workers.into_iter().filter(|worker| !worker.is_integrated()).map(|worker| view! {
                            <option value=worker.id.to_string()>{worker.name}</option>
                        }).collect_view()}
                    </select>
                </div>

                <div class="form-group">
                    <label for="library-path" class="form-label">"Chemin sur cet hôte"</label>
                    <div class="copy-input-row">
                        <input id="library-path" class="form-input" placeholder="/mnt/medias/films"
                            prop:value=move || new_path.get()
                            on:input=move |event| {
                                detected_writable.set(None);
                                new_path.set(event_target_value(&event));
                            } />
                        <button
                            type="button"
                            class="btn btn-secondary btn-sm"
                            disabled=move || selected_worker_id.get().is_none()
                            on:click=move |_| browse_tick.update(|v| *v = v.wrapping_add(1))
                        >
                            "Ouvrir"
                        </button>
                    </div>
                    <p class="form-help">
                        "Choisissez un dossier dans l'arborescence ci-dessous ou saisissez un chemin absolu."
                    </p>
                </div>

                <div class="form-group">
                    <label class="form-label" for="library-source-key">"Identifiant de source partagée"</label>
                    <input id="library-source-key" class="form-input" maxlength="80" aria-describedby="library-source-help"
                        prop:value=move || source_key.get()
                        on:input=move |event| source_key.set(event_target_value(&event)) />
                    <p id="library-source-help" class="form-help">"Saisissez le même identifiant sur les hôtes qui voient le même dossier physique. Laissez vide pour un dossier indépendant."</p>
                </div>
                <FolderTreePicker
                    worker_id=selected_worker_id
                    selected_path=new_path
                    on_select=on_tree_select
                    navigate_tick=browse_signal
                />

                <div class="form-group">
                    <label for="library-access" class="form-label">"Accès"</label>
                    <select id="library-access" class="form-select"
                        prop:value=move || access_value(new_access.get())
                        on:change=move |event| new_access.set(parse_access(&event_target_value(&event)))>
                        <option value="read_write">{access_label(LibraryAccess::ReadWrite)}</option>
                        <option value="read">{access_label(LibraryAccess::Read)}</option>
                    </select>
                    {move || {
                        (detected_writable.get() == Some(false)
                            && new_access.get() == LibraryAccess::ReadWrite)
                            .then(|| view! {
                                <p class="form-help library-access-warning">
                                    "Attention : ce dossier apparaît en lecture seule sur cet hôte ; l'écriture risque d'échouer."
                                </p>
                            })
                    }}
                </div>
                <div class="form-group">
                    <button type="button" class="btn btn-secondary" on:click=add_worker>
                        "Ajouter le dossier"
                    </button>
                </div>
            </div>

            <p class="form-help" role="status">{move || feedback.get()}</p>
            <div class="inspection-drawer__actions">
                <button type="submit" class="btn btn-primary" disabled=move || submit.pending().get()>
                    "Enregistrer"
                </button>
                <button type="button" class="btn btn-secondary" on:click=move |_| on_done.run(())>"Annuler"</button>
                {existing_id.map(|id| view! {
                    <button type="button" class="btn btn-danger-quiet" disabled=move || remove.pending().get()
                        on:click=move |_| { remove.dispatch(id); }>
                        "Supprimer la bibliothèque"
                    </button>
                })}
            </div>
        </form>
    }
}
