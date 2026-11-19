//! Onglet Packs de la Librairie : assembler, inspecter et gérer des packs locaux sans dépôt GitHub.

use super::{LOCAL_PACKS_PATH, LocalDeleteButton, bump, use_local_delete};
use crate::api;
use crate::plugins::{PackDetail, PackDrawerTab};
use crate::ui::search::{matches, query};
use crate::ui::topbar_action::{ActionSpec, provide_topbar_action};
use crate::ui::{
    Drawer, EmptyState, IconName, MetricTone, ResourceCard, StatItem, StatStrip, Tag, TagTone,
    Toasts,
};
use forgearr_shared::local::{CreateLocalPackRequest, LocalLibrarySnapshot};
use forgearr_shared::plugins::vocab::CatalogItemType;
use forgearr_shared::plugins::{CatalogEntry, PackComponent};
use leptos::prelude::*;
use std::collections::HashSet;

fn available_components(snapshot: &LocalLibrarySnapshot) -> Vec<PackComponent> {
    let mut out = Vec::new();
    for s in &snapshot.scripts {
        out.push(PackComponent {
            id: s.id.clone(),
            version: "1.0.0".into(),
            kind: CatalogItemType::Script,
        });
    }
    for r in &snapshot.resources {
        out.push(PackComponent {
            id: r.id.clone(),
            version: "1.0.0".into(),
            kind: r.entry_type,
        });
    }
    for b in &snapshot.block_templates {
        if b.is_local {
            out.push(PackComponent {
                id: b.id.clone(),
                version: "1.0.0".into(),
                kind: CatalogItemType::ResourceTemplate,
            });
        }
    }
    for f in &snapshot.flow_templates {
        if f.is_local {
            out.push(PackComponent {
                id: f.id.clone(),
                version: f.version.clone(),
                kind: CatalogItemType::Flow,
            });
        }
    }
    out
}

#[component]
pub fn LocalPacksSection(
    snapshot: LocalResource<Result<LocalLibrarySnapshot, api::ApiError>>,
    reload: RwSignal<u32>,
    can_manage: bool,
) -> impl IntoView {
    let create_open = RwSignal::new(false);
    let pack_name = RwSignal::new(String::new());
    let pack_desc = RwSignal::new(String::new());
    let selected = RwSignal::new(HashSet::<String>::new());
    let feedback = RwSignal::new(None::<String>);

    let inspect_open = RwSignal::new(false);
    let inspect_id = RwSignal::new(None::<String>);
    let inspect_name = RwSignal::new(String::new());
    let inspect_tab = RwSignal::new(0_usize);

    Effect::new(move |_| {
        if !create_open.get() {
            pack_name.set(String::new());
            pack_desc.set(String::new());
            selected.set(HashSet::new());
            feedback.set(None);
        }
    });
    Effect::new(move |_| {
        if !inspect_open.get() {
            inspect_id.set(None);
        }
    });

    let save = Action::new_local(move |req: &CreateLocalPackRequest| {
        let req = req.clone();
        async move {
            api::post::<CreateLocalPackRequest, CatalogEntry>(LOCAL_PACKS_PATH, Some(&req)).await
        }
    });
    Effect::new(move |_| match save.value().get() {
        Some(Ok(_)) => {
            create_open.set(false);
            Toasts::notify_success("Pack local créé.");
            bump(reload);
        }
        Some(Err(err)) => Toasts::set_error(feedback, err.message()),
        None => {}
    });

    let remove = use_local_delete(reload, "Pack local supprimé.");

    let stats = Signal::derive(move || {
        let count = snapshot
            .get()
            .and_then(Result::ok)
            .map_or(0, |s| s.packs.len());
        vec![StatItem::new(
            "Packs locaux",
            count.to_string(),
            if count > 0 {
                MetricTone::Success
            } else {
                MetricTone::Neutral
            },
        )]
    });

    if can_manage {
        provide_topbar_action(ActionSpec {
            label: "Créer un pack local",
            on_click: Callback::new(move |()| {
                feedback.set(None);
                create_open.set(true);
            }),
        });
    }

    let inspect_title = Signal::derive(move || {
        let name = inspect_name.get();
        if name.is_empty() {
            "Pack local".to_owned()
        } else {
            name
        }
    });

    view! {
        <StatStrip aria_label="Synthèse des packs locaux" items=stats />
        {move || {
            let text = query();
            match snapshot.get() {
                None => view! {
                    <div class="empty-state-panel"><p class="text-dim">"Chargement des packs…"</p></div>
                }.into_any(),
                Some(Err(err)) => view! {
                    <div class="empty-state-panel"><p class="form-error">{err.message()}</p></div>
                }.into_any(),
                Some(Ok(data)) => {
                    let shown: Vec<CatalogEntry> = data
                        .packs
                        .into_iter()
                        .filter(|p| matches(&[&p.name, &p.id], &text))
                        .collect();
                    if shown.is_empty() {
                        return view! {
                            <div class="empty-state-panel">
                                <EmptyState
                                    title="Aucun pack local"
                                    message="Regroupez vos scripts, ressources et modèles de flows dans un pack local réutilisable."
                                    icon_name=IconName::Package
                                />
                            </div>
                        }.into_any();
                    }
                    view! {
                        <div class="resource-grid resource-grid--list">
                            {shown.into_iter().map(|pack| {
                                let pid = pack.id.clone();
                                let pname = pack.name.clone();
                                let card_id = pack.id.clone();
                                let open_inspect = Callback::new(move |()| {
                                    inspect_id.set(Some(pid.clone()));
                                    inspect_name.set(pname.clone());
                                    inspect_tab.set(0);
                                    inspect_open.set(true);
                                });
                                let active = Signal::derive(move || {
                                    inspect_open.get() && inspect_id.get().as_deref() == Some(card_id.as_str())
                                });
                                let badge = view! { <Tag label=pack.version.clone() tone=TagTone::Mono /> }.into_any();
                                let actions = view! {
                                    <button type="button" class="btn btn-secondary btn-sm"
                                        on:click=move |ev| {
                                            ev.stop_propagation();
                                            open_inspect.run(());
                                        }>"Inspecter"</button>
                                    {can_manage.then(|| {
                                        let del_id = pack.id.clone();
                                        view! { <LocalDeleteButton id=del_id remove=remove /> }
                                    })}
                                }.into_any();
                                view! {
                                    <ResourceCard
                                        title=pack.name
                                        icon=IconName::Download
                                        tone=MetricTone::Success
                                        badge=Some(badge)
                                        active=Some(active)
                                        on_click=Some(open_inspect)
                                        footer_meta=pack.id
                                        footer_actions=Some(actions)
                                    >
                                        <div class="cluster">
                                            <Tag label="Dépôt local" tone=TagTone::Primary />
                                        </div>
                                    </ResourceCard>
                                }
                            }).collect_view()}
                        </div>
                    }.into_any()
                }
            }
        }}
        <Drawer open=create_open eyebrow="Pack local" title="Assembler un pack".to_owned()>
            {move || {
                let choices = snapshot
                    .get()
                    .and_then(Result::ok)
                    .map(|s| available_components(&s))
                    .unwrap_or_default();
                let choices_submit = choices.clone();
                view! {
                    <form class="inspection-panel" on:submit=move |ev| {
                        ev.prevent_default();
                        let chosen_ids = selected.get_untracked();
                        let components: Vec<PackComponent> = choices_submit
                            .iter()
                            .filter(|c| chosen_ids.contains(&c.id))
                            .cloned()
                            .collect();
                        if components.is_empty() {
                            Toasts::set_error(feedback, "Sélectionnez au moins un composant.");
                            return;
                        }
                        save.dispatch(CreateLocalPackRequest {
                            id: None,
                            name: pack_name.get_untracked(),
                            description: Some(pack_desc.get_untracked()),
                            components,
                            entrypoint: None,
                        });
                    }>
                        <div class="form-group">
                            <label class="form-label" for="lpack-name">"Nom du pack *"</label>
                            <input id="lpack-name" class="form-input" required=true
                                prop:value=move || pack_name.get()
                                on:input=move |e| pack_name.set(event_target_value(&e)) />
                        </div>
                        <div class="form-group">
                            <label class="form-label" for="lpack-desc">"Description"</label>
                            <input id="lpack-desc" class="form-input"
                                prop:value=move || pack_desc.get()
                                on:input=move |e| pack_desc.set(event_target_value(&e)) />
                        </div>
                        <div class="form-group">
                            <span class="form-label">"Composants locaux à inclure *"</span>
                            {if choices.is_empty() {
                                view! {
                                    <p class="text-dim text-sm">"Créez d'abord un script, une ressource ou un modèle de flow."</p>
                                }.into_any()
                            } else {
                                view! {
                                    <ul class="step-list">
                                        {choices.into_iter().map(|comp| {
                                            let cid = comp.id.clone();
                                            let cid_check = cid.clone();
                                            let cid_toggle = cid.clone();
                                            view! {
                                                <li class="step-item">
                                                    <label class="step-item__head">
                                                        <input type="checkbox"
                                                            prop:checked=move || selected.get().contains(&cid_check)
                                                            on:change=move |_| {
                                                                selected.update(|set| {
                                                                    if !set.remove(&cid_toggle) {
                                                                        set.insert(cid_toggle.clone());
                                                                    }
                                                                });
                                                            } />
                                                        <span class="text-mono text-strong">{cid}</span>
                                                        <Tag label=comp.kind.as_str() tone=TagTone::Neutral />
                                                    </label>
                                                </li>
                                            }
                                        }).collect_view()}
                                    </ul>
                                }.into_any()
                            }}
                        </div>
                        <div class="inspection-drawer__actions">
                            <button type="submit" class="btn btn-primary" disabled=move || save.pending().get()>
                                "Créer le pack"
                            </button>
                            <button type="button" class="btn btn-secondary" on:click=move |_| create_open.set(false)>
                                "Annuler"
                            </button>
                        </div>
                        <p class="form-error text-sm" role="status">{move || feedback.get()}</p>
                    </form>
                }
            }}
        </Drawer>
        <Drawer
            open=inspect_open
            eyebrow="Pack local"
            title=inspect_title
            tabs=PackDrawerTab::TABS.to_vec()
            tab=inspect_tab
        >
            {move || inspect_id.get().map(|id| view! {
                <PackDetail
                    pack_id=id
                    tab=inspect_tab
                    can_install=can_manage
                    on_installed=Callback::new(move |()| bump(reload))
                />
            })}
        </Drawer>
    }
}
