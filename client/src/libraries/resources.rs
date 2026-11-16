//! Onglet Ressources & Blocs de la Librairie : import de fichiers locaux et modèles de blocs.

use super::{LOCAL_BLOCK_TEMPLATES_PATH, LOCAL_ITEMS_PATH, LOCAL_RESOURCES_PATH, bump};
use crate::api;
use crate::route::encode_segment;
use crate::ui::search::{matches, query};
use crate::ui::{
    Drawer, EmptyState, Icon, IconName, MetricTone, ResourceCard, StatItem, StatStrip, Tag,
    TagTone, Toasts,
};
use flux_shared::base64;
use flux_shared::flows::FlowRecord;
use flux_shared::local::{
    CreateBlockTemplateRequest, LocalBlockTemplate, LocalLibrarySnapshot, LocalResourceSummary,
    UploadLocalResourceRequest,
};
use flux_shared::plugins::vocab::CatalogItemType;
use leptos::prelude::*;
use serde_json::Value;
use wasm_bindgen::JsCast;

const RESOURCE_TYPES: &[(&str, &str)] = &[
    ("resource.subtitle", "Sous-titres (.srt, .ass, .vtt)"),
    ("resource.image", "Image (.png, .jpg, .webp)"),
    ("resource.audio", "Audio / Son (.mp3, .wav, .aac)"),
    ("resource.font", "Police (.ttf, .otf)"),
    ("resource.language", "Langue / Dictionnaire"),
    ("resource.color", "Palette / LUT couleur"),
    ("resource.template", "Modèle de document"),
    ("preset", "Préréglage"),
];

pub fn parse_resource_type(raw: &str) -> CatalogItemType {
    serde_json::from_value(Value::String(raw.to_owned()))
        .unwrap_or(CatalogItemType::ResourceTemplate)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ResourcesKindFilter {
    #[default]
    All,
    Files,
    BlockTemplates,
}

fn format_bytes(size: u64) -> String {
    if size < 1024 {
        format!("{size} o")
    } else if size < 1024 * 1024 {
        format!("{:.1} Ko", size as f64 / 1024.0)
    } else {
        format!("{:.1} Mo", size as f64 / (1024.0 * 1024.0))
    }
}

#[component]
pub fn ResourcesSection(
    snapshot: LocalResource<Result<LocalLibrarySnapshot, api::ApiError>>,
    flows: LocalResource<Result<Vec<FlowRecord>, api::ApiError>>,
    reload: RwSignal<u32>,
    can_manage: bool,
) -> impl IntoView {
    let filter = RwSignal::new(ResourcesKindFilter::All);
    let upload_open = RwSignal::new(false);
    let template_open = RwSignal::new(false);
    let feedback = RwSignal::new(None::<String>);

    // État du formulaire d'upload
    let res_name = RwSignal::new(String::new());
    let res_type = RwSignal::new("resource.subtitle".to_owned());
    let res_desc = RwSignal::new(String::new());
    let res_filename = RwSignal::new(String::new());
    let res_b64 = RwSignal::new(String::new());
    let reading_file = RwSignal::new(false);

    // État du formulaire de modèle de blocs
    let tpl_name = RwSignal::new(String::new());
    let tpl_desc = RwSignal::new(String::new());
    let tpl_json = RwSignal::new("{\"nodes\":[],\"connections\":[]}".to_owned());

    let upload = Action::new_local(move |req: &UploadLocalResourceRequest| {
        let req = req.clone();
        async move {
            api::post::<UploadLocalResourceRequest, LocalResourceSummary>(
                LOCAL_RESOURCES_PATH,
                Some(&req),
            )
            .await
        }
    });
    Effect::new(move |_| match upload.value().get() {
        Some(Ok(_)) => {
            upload_open.set(false);
            res_name.set(String::new());
            res_desc.set(String::new());
            res_filename.set(String::new());
            res_b64.set(String::new());
            Toasts::notify_success("Fichier importé dans la librairie locale.");
            bump(reload);
        }
        Some(Err(err)) => Toasts::set_error(feedback, err.message()),
        None => {}
    });

    let save_template = Action::new_local(move |req: &CreateBlockTemplateRequest| {
        let req = req.clone();
        async move {
            api::post::<CreateBlockTemplateRequest, LocalBlockTemplate>(
                LOCAL_BLOCK_TEMPLATES_PATH,
                Some(&req),
            )
            .await
        }
    });
    Effect::new(move |_| match save_template.value().get() {
        Some(Ok(_)) => {
            template_open.set(false);
            tpl_name.set(String::new());
            tpl_desc.set(String::new());
            Toasts::notify_success("Modèle de blocs enregistré.");
            bump(reload);
        }
        Some(Err(err)) => Toasts::set_error(feedback, err.message()),
        None => {}
    });

    let remove = Action::new_local(move |id: &String| {
        let path = format!("{LOCAL_ITEMS_PATH}/{}", encode_segment(id));
        async move { api::delete(&path).await }
    });
    Effect::new(move |_| match remove.value().get() {
        Some(Ok(())) => {
            Toasts::notify_success("Élément supprimé.");
            bump(reload);
        }
        Some(Err(err)) => Toasts::notify_error(err.message()),
        None => {}
    });

    let stats = Signal::derive(move || {
        let (files_count, blocks_count) = snapshot
            .get()
            .and_then(Result::ok)
            .map_or((0, 0), |s| (s.resources.len(), s.block_templates.len()));
        let active = filter.get();
        let set_filter = move |next: ResourcesKindFilter| Callback::new(move |()| filter.set(next));
        vec![
            StatItem::new(
                "Toutes",
                (files_count + blocks_count).to_string(),
                MetricTone::Neutral,
            )
            .with_filter(
                active == ResourcesKindFilter::All,
                set_filter(ResourcesKindFilter::All),
            ),
            StatItem::new("Fichiers", files_count.to_string(), MetricTone::Neutral).with_filter(
                active == ResourcesKindFilter::Files,
                set_filter(ResourcesKindFilter::Files),
            ),
            StatItem::new(
                "Modèles de blocs",
                blocks_count.to_string(),
                MetricTone::Neutral,
            )
            .with_filter(
                active == ResourcesKindFilter::BlockTemplates,
                set_filter(ResourcesKindFilter::BlockTemplates),
            ),
        ]
    });

    view! {
        <StatStrip aria_label="Synthèse des ressources et modèles de blocs" items=stats />
        {can_manage.then(|| view! {
            <div class="inline-actions">
                <button type="button" class="btn btn-primary btn-sm" on:click=move |_| {
                    feedback.set(None);
                    upload_open.set(true);
                }>
                    <Icon name=IconName::Plus size=14 />
                    <span>"Importer un fichier"</span>
                </button>
                <button type="button" class="btn btn-secondary btn-sm" on:click=move |_| {
                    feedback.set(None);
                    template_open.set(true);
                }>
                    <Icon name=IconName::Layers size=14 />
                    <span>"Nouveau modèle de blocs"</span>
                </button>
            </div>
        })}
        {move || {
            let text = query();
            let active = filter.get();
            match snapshot.get() {
                None => view! {
                    <div class="empty-state-panel"><p class="text-dim">"Chargement des ressources…"</p></div>
                }.into_any(),
                Some(Err(err)) => view! {
                    <div class="empty-state-panel"><p class="form-error">{err.message()}</p></div>
                }.into_any(),
                Some(Ok(data)) => {
                    let show_files = matches!(active, ResourcesKindFilter::All | ResourcesKindFilter::Files);
                    let show_blocks = matches!(active, ResourcesKindFilter::All | ResourcesKindFilter::BlockTemplates);
                    let files: Vec<LocalResourceSummary> = if show_files {
                        data.resources
                            .into_iter()
                            .filter(|r| matches(&[&r.name, &r.id, &r.filename], &text))
                            .collect()
                    } else {
                        Vec::new()
                    };
                    let blocks: Vec<LocalBlockTemplate> = if show_blocks {
                        data.block_templates
                            .into_iter()
                            .filter(|b| matches(&[&b.name, &b.id, b.description.as_deref().unwrap_or("")], &text))
                            .collect()
                    } else {
                        Vec::new()
                    };
                    if files.is_empty() && blocks.is_empty() {
                        return view! {
                            <div class="empty-state-panel">
                                <EmptyState
                                    title="Aucune ressource ou modèle de blocs"
                                    message="Importez des sous-titres, images, sons ou polices sans dépôt GitHub, ou enregistrez des modèles de blocs réutilisables."
                                    icon_name=IconName::Layers
                                />
                            </div>
                        }.into_any();
                    }
                    view! {
                        <div class="resource-grid resource-grid--list">
                            {files.into_iter().map(|res| {
                                let del_id = res.id.clone();
                                let kind_label = res.entry_type.subtitle().unwrap_or("Ressource");
                                let badge = view! { <Tag label=kind_label tone=TagTone::Primary /> }.into_any();
                                let actions = can_manage.then(|| {
                                    let del_id = del_id.clone();
                                    view! {
                                        <button type="button" class="btn btn-sm btn-danger-quiet"
                                            disabled=move || remove.pending().get()
                                            on:click=move |_| { remove.dispatch(del_id.clone()); }>"Supprimer"</button>
                                    }.into_any()
                                });
                                view! {
                                    <ResourceCard
                                        title=res.name
                                        subtitle=res.description
                                        icon=IconName::Package
                                        tone=MetricTone::Neutral
                                        badge=Some(badge)
                                        footer_meta=format!("{} · {}", res.id, format_bytes(res.size_bytes))
                                        footer_actions=actions
                                    >
                                        <div class="cluster">
                                            <Tag label=res.filename tone=TagTone::Mono />
                                        </div>
                                    </ResourceCard>
                                }
                            }).collect_view()}
                            {blocks.into_iter().map(|tpl| {
                                let del_id = tpl.id.clone();
                                let nodes_count = tpl
                                    .definition
                                    .get("nodes")
                                    .and_then(Value::as_array)
                                    .map_or(0, Vec::len);
                                let badge = view! {
                                    <Tag
                                        label="Modèle de blocs"
                                        tone=if tpl.is_local { TagTone::Success } else { TagTone::Primary }
                                    />
                                }.into_any();
                                let actions = (can_manage && tpl.is_local).then(|| {
                                    let del_id = del_id.clone();
                                    view! {
                                        <button type="button" class="btn btn-sm btn-danger-quiet"
                                            disabled=move || remove.pending().get()
                                            on:click=move |_| { remove.dispatch(del_id.clone()); }>"Supprimer"</button>
                                    }.into_any()
                                });
                                view! {
                                    <ResourceCard
                                        title=tpl.name
                                        subtitle=tpl.description
                                        icon=IconName::Layers
                                        tone=MetricTone::Success
                                        badge=Some(badge)
                                        footer_meta=format!("{nodes_count} bloc(s) · {}", tpl.id)
                                        footer_actions=actions
                                    >
                                        <div class="cluster">
                                            {tpl.tags.into_iter().map(|t| view! {
                                                <Tag label=t tone=TagTone::Neutral />
                                            }).collect_view()}
                                        </div>
                                    </ResourceCard>
                                }
                            }).collect_view()}
                        </div>
                    }.into_any()
                }
            }
        }}
        <Drawer open=upload_open eyebrow="Ressource locale" title="Importer un fichier".to_owned()>
            <form class="inspection-panel" on:submit=move |ev| {
                ev.prevent_default();
                let b64 = res_b64.get_untracked();
                if b64.is_empty() {
                    Toasts::set_error(feedback, "Choisissez d'abord un fichier.");
                    return;
                }
                upload.dispatch(UploadLocalResourceRequest {
                    name: res_name.get_untracked(),
                    entry_type: parse_resource_type(&res_type.get_untracked()),
                    filename: res_filename.get_untracked(),
                    content_base64: b64,
                    description: Some(res_desc.get_untracked()),
                });
            }>
                <div class="form-group">
                    <label class="form-label" for="res-type">"Type de ressource"</label>
                    <select id="res-type" class="form-input"
                        prop:value=move || res_type.get()
                        on:change=move |e| res_type.set(event_target_value(&e))>
                        {RESOURCE_TYPES.iter().map(|(val, label)| view! {
                            <option value=*val>{*label}</option>
                        }).collect_view()}
                    </select>
                </div>
                <div class="form-group">
                    <label class="form-label" for="res-name">"Nom *"</label>
                    <input id="res-name" class="form-input" required=true
                        prop:value=move || res_name.get()
                        on:input=move |e| res_name.set(event_target_value(&e)) />
                </div>
                <div class="form-group">
                    <label class="form-label" for="res-file">"Fichier (max 5 Mo) *"</label>
                    <input id="res-file" class="form-input" type="file" on:change=move |ev| {
                        let Some(target) = ev.target().and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok()) else {
                            return;
                        };
                        let Some(files) = target.files() else { return; };
                        let Some(file) = files.get(0) else { return; };
                        let fname = file.name();
                        res_filename.set(fname.clone());
                        if res_name.get_untracked().trim().is_empty() {
                            res_name.set(fname);
                        }
                        reading_file.set(true);
                        wasm_bindgen_futures::spawn_local(async move {
                            if let Ok(buf) = wasm_bindgen_futures::JsFuture::from(file.array_buffer()).await {
                                let bytes = js_sys::Uint8Array::new(&buf).to_vec();
                                res_b64.set(base64::encode(&bytes));
                            }
                            reading_file.set(false);
                        });
                    } />
                    {move || (!res_filename.get().is_empty()).then(|| view! {
                        <p class="form-help">{format!("Fichier prêt : {}", res_filename.get())}</p>
                    })}
                </div>
                <div class="form-group">
                    <label class="form-label" for="res-desc">"Description"</label>
                    <input id="res-desc" class="form-input"
                        prop:value=move || res_desc.get()
                        on:input=move |e| res_desc.set(event_target_value(&e)) />
                </div>
                <div class="inspection-drawer__actions">
                    <button type="submit" class="btn btn-primary"
                        disabled=move || upload.pending().get() || reading_file.get()>
                        {move || if upload.pending().get() { "Import…" } else { "Importer" }}
                    </button>
                </div>
                <p class="form-error text-sm" role="status">{move || feedback.get()}</p>
            </form>
        </Drawer>
        <Drawer open=template_open eyebrow="Modèle de blocs" title="Nouveau modèle de blocs".to_owned()>
            <form class="inspection-panel" on:submit=move |ev| {
                ev.prevent_default();
                let Ok(definition) = serde_json::from_str::<Value>(&tpl_json.get_untracked()) else {
                    Toasts::set_error(feedback, "Définition JSON invalide.");
                    return;
                };
                save_template.dispatch(CreateBlockTemplateRequest {
                    id: None,
                    name: tpl_name.get_untracked(),
                    description: Some(tpl_desc.get_untracked()),
                    definition,
                    tags: Vec::new(),
                });
            }>
                <div class="form-group">
                    <label class="form-label" for="btpl-name">"Nom du modèle *"</label>
                    <input id="btpl-name" class="form-input" required=true
                        prop:value=move || tpl_name.get()
                        on:input=move |e| tpl_name.set(event_target_value(&e)) />
                </div>
                <div class="form-group">
                    <label class="form-label" for="btpl-desc">"Description"</label>
                    <input id="btpl-desc" class="form-input"
                        prop:value=move || tpl_desc.get()
                        on:input=move |e| tpl_desc.set(event_target_value(&e)) />
                </div>
                {move || {
                    let available = flows.get().and_then(Result::ok).unwrap_or_default();
                    (!available.is_empty()).then(|| view! {
                        <div class="form-group">
                            <label class="form-label" for="btpl-source">"Pré-remplir depuis un flow existant"</label>
                            <select id="btpl-source" class="form-input" on:change=move |ev| {
                                let chosen = event_target_value(&ev);
                                if let Some(flow) = available.iter().find(|f| f.id == chosen) {
                                    tpl_json.set(flow.definition.clone());
                                    if tpl_name.get_untracked().trim().is_empty() {
                                        tpl_name.set(format!("Blocs · {}", flow.name));
                                    }
                                }
                            }>
                                <option value="">"— Choisir un flow —"</option>
                                {available.iter().map(|f| view! {
                                    <option value=f.id.clone()>{f.name.clone()}</option>
                                }).collect_view()}
                            </select>
                        </div>
                    })
                }}
                <div class="form-group">
                    <label class="form-label" for="btpl-json">"Graphe JSON ({ nodes, connections }) *"</label>
                    <textarea id="btpl-json" class="form-input text-mono" rows="8" required=true
                        prop:value=move || tpl_json.get()
                        on:input=move |e| tpl_json.set(event_target_value(&e)) />
                </div>
                <div class="inspection-drawer__actions">
                    <button type="submit" class="btn btn-primary" disabled=move || save_template.pending().get()>
                        "Enregistrer le modèle"
                    </button>
                </div>
                <p class="form-error text-sm" role="status">{move || feedback.get()}</p>
            </form>
        </Drawer>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_resource_type_covers_subtitles_and_fallbacks() {
        assert_eq!(
            parse_resource_type("resource.subtitle"),
            CatalogItemType::ResourceSubtitle
        );
        assert_eq!(
            parse_resource_type("resource.image"),
            CatalogItemType::ResourceImage
        );
        assert_eq!(
            parse_resource_type("unknown"),
            CatalogItemType::ResourceTemplate
        );
    }

    #[test]
    fn format_bytes_scales_cleanly() {
        assert_eq!(format_bytes(500), "500 o");
        assert_eq!(format_bytes(2048), "2.0 Ko");
    }
}
