//! Onglet Ressources & Blocs de la Librairie : import de fichiers locaux et modèles de blocs.

use super::block_templates::{BlockTemplateCard, BlockTemplateDraft, BlockTemplateDrawer};
use super::{LOCAL_ITEMS_PATH, LOCAL_RESOURCES_PATH, LocalDeleteButton, bump, use_local_delete};
use crate::api;
use crate::route::encode_segment;
use crate::ui::search::{matches, query};
use crate::ui::topbar_action::{ActionSpec, provide_topbar_actions};
use crate::ui::{
    Drawer, EmptyState, IconName, MetricTone, ResourceCard, StatItem, StatStrip, Tag, TagTone,
    Toasts,
};
use forgearr_shared::base64;
use forgearr_shared::flows::FlowRecord;
use forgearr_shared::local::{
    LocalBlockTemplate, LocalLibrarySnapshot, LocalResourceSummary, UploadLocalResourceRequest,
};
use forgearr_shared::plugins::vocab::CatalogItemType;
use leptos::prelude::*;
use serde_json::Value;
use wasm_bindgen::JsCast;

const MAX_RESOURCE_BYTES: usize = 5 * 1024 * 1024;

const RESOURCE_TYPES: &[(CatalogItemType, &str)] = &[
    (
        CatalogItemType::ResourceSubtitle,
        "Sous-titres (.srt, .ass, .vtt)",
    ),
    (CatalogItemType::ResourceImage, "Image (.png, .jpg, .webp)"),
    (
        CatalogItemType::ResourceAudio,
        "Audio / Son (.mp3, .wav, .aac)",
    ),
    (CatalogItemType::ResourceFont, "Police (.ttf, .otf)"),
    (CatalogItemType::ResourceLanguage, "Langue / Dictionnaire"),
    (CatalogItemType::ResourceColor, "Palette / LUT couleur"),
    (CatalogItemType::ResourceTemplate, "Modèle de document"),
    (CatalogItemType::Preset, "Préréglage"),
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
    let template_draft = RwSignal::new(None::<BlockTemplateDraft>);
    let feedback = RwSignal::new(None::<String>);

    let res_id = RwSignal::new(None::<String>);
    let res_name = RwSignal::new(String::new());
    let res_type = RwSignal::new(CatalogItemType::ResourceSubtitle);
    let res_desc = RwSignal::new(String::new());
    let res_filename = RwSignal::new(String::new());
    let res_b64 = RwSignal::new(String::new());
    let reading_file = RwSignal::new(false);

    Effect::new(move |_| {
        if !upload_open.get() {
            res_id.set(None);
            res_name.set(String::new());
            res_type.set(CatalogItemType::ResourceSubtitle);
            res_desc.set(String::new());
            res_filename.set(String::new());
            res_b64.set(String::new());
            reading_file.set(false);
            feedback.set(None);
        }
    });

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
            Toasts::notify_success("Fichier enregistré dans la librairie locale.");
            bump(reload);
        }
        Some(Err(err)) => Toasts::set_error(feedback, err.message()),
        None => {}
    });

    let remove = use_local_delete(reload, "Élément supprimé.");

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

    if can_manage {
        provide_topbar_actions(vec![
            ActionSpec {
                label: "Importer un fichier",
                on_click: Callback::new(move |()| {
                    feedback.set(None);
                    res_id.set(None);
                    upload_open.set(true);
                }),
            },
            ActionSpec {
                label: "Modèle de blocs",
                on_click: Callback::new(move |()| {
                    feedback.set(None);
                    template_draft.set(Some(BlockTemplateDraft::blank()));
                    template_open.set(true);
                }),
            },
        ]);
    }

    let upload_title = Signal::derive(move || {
        if res_id.get().is_some() {
            "Remplacer le fichier".to_owned()
        } else {
            "Importer un fichier".to_owned()
        }
    });

    view! {
        <StatStrip aria_label="Synthèse des ressources et modèles de blocs" items=stats />
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
                                let download_url = format!("{LOCAL_ITEMS_PATH}/{}/file", encode_segment(&res.id));
                                let dl_name = res.filename.clone();
                                let kind_label = res.entry_type.subtitle().unwrap_or("Ressource");
                                let badge = view! { <Tag label=kind_label tone=TagTone::Primary /> }.into_any();
                                let card_id = res.id.clone();
                                let replace_res = can_manage.then(|| {
                                    let r = res.clone();
                                    Callback::new(move |()| {
                                        feedback.set(None);
                                        res_id.set(Some(r.id.clone()));
                                        res_name.set(r.name.clone());
                                        res_type.set(r.entry_type);
                                        res_desc.set(r.description.clone().unwrap_or_default());
                                        res_filename.set(String::new());
                                        res_b64.set(String::new());
                                        upload_open.set(true);
                                    })
                                });
                                let active_card = Signal::derive(move || {
                                    upload_open.get() && res_id.get().as_deref() == Some(card_id.as_str())
                                });
                                let actions = view! {
                                    <a
                                        href=download_url
                                        download=dl_name
                                        target="_blank"
                                        rel="noopener"
                                        class="btn btn-secondary btn-sm"
                                        on:click=move |ev| ev.stop_propagation()
                                    >
                                        "Télécharger"
                                    </a>
                                    {can_manage.then(|| {
                                        let del_id = res.id.clone();
                                        let replace_cb = replace_res;
                                        view! {
                                            {replace_cb.map(|cb| view! {
                                                <button type="button" class="btn btn-secondary btn-sm"
                                                    on:click=move |ev| {
                                                        ev.stop_propagation();
                                                        cb.run(());
                                                    }>"Remplacer"</button>
                                            })}
                                            <LocalDeleteButton id=del_id remove=remove />
                                        }
                                    })}
                                }.into_any();
                                view! {
                                    <ResourceCard
                                        title=res.name
                                        subtitle=res.description
                                        icon=IconName::Package
                                        tone=MetricTone::Neutral
                                        badge=Some(badge)
                                        active=Some(active_card)
                                        on_click=replace_res
                                        footer_meta=format!("{} · {}", res.id, format_bytes(res.size_bytes))
                                        footer_actions=Some(actions)
                                    >
                                        <div class="cluster">
                                            <Tag label=res.filename tone=TagTone::Mono />
                                        </div>
                                    </ResourceCard>
                                }
                            }).collect_view()}
                            {blocks.into_iter().map(|tpl| view! {
                                <BlockTemplateCard
                                    tpl=tpl
                                    can_manage=can_manage
                                    draft_sig=template_draft
                                    open_sig=template_open
                                    feedback=feedback
                                    remove=remove
                                />
                            }).collect_view()}
                        </div>
                    }.into_any()
                }
            }
        }}
        <Drawer open=upload_open eyebrow="Ressource locale" title=upload_title>
            <form class="inspection-panel" on:submit=move |ev| {
                ev.prevent_default();
                let b64 = res_b64.get_untracked();
                if b64.is_empty() {
                    Toasts::set_error(feedback, "Choisissez d'abord un fichier.");
                    return;
                }
                upload.dispatch(UploadLocalResourceRequest {
                    id: res_id.get_untracked(),
                    name: res_name.get_untracked(),
                    entry_type: res_type.get_untracked(),
                    filename: res_filename.get_untracked(),
                    content_base64: b64,
                    description: Some(res_desc.get_untracked()),
                });
            }>
                <div class="form-group">
                    <label class="form-label" for="res-type">"Type de ressource"</label>
                    <select id="res-type" class="form-input"
                        prop:value=move || res_type.get().as_str()
                        on:change=move |e| res_type.set(parse_resource_type(&event_target_value(&e)))>
                        {RESOURCE_TYPES.iter().map(|&(kind, label)| view! {
                            <option value=kind.as_str()>{label}</option>
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
                        let file_size = file.size();
                        if file_size <= 0.0 {
                            Toasts::set_error(feedback, "Le fichier sélectionné est vide.");
                            target.set_value("");
                            return;
                        }
                        if file_size > MAX_RESOURCE_BYTES as f64 {
                            Toasts::set_error(feedback, "Le fichier dépasse la taille maximale de 5 Mo.");
                            target.set_value("");
                            return;
                        }
                        feedback.set(None);
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
                            } else {
                                Toasts::set_error(feedback, "Lecture du fichier impossible.");
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
                    <button type="button" class="btn btn-secondary" on:click=move |_| upload_open.set(false)>
                        "Annuler"
                    </button>
                </div>
                <p class="form-error text-sm" role="status">{move || feedback.get()}</p>
            </form>
        </Drawer>
        <BlockTemplateDrawer
            open=template_open
            draft_sig=template_draft
            flows=flows
            reload=reload
            feedback=feedback
        />
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
