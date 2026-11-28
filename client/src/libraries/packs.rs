//! Onglet Packs de l'Atelier : assembler, inspecter et gérer des packs locaux sans dépôt GitHub.

use super::{LOCAL_PACKS_PATH, LOCAL_TOPICS, LocalDeleteButton, use_local_delete};
use crate::api;
use crate::live::{Loaded, use_live};
use crate::plugins::{PackDetail, PackDrawerTab};
use crate::ui::search::{matches, query};
use crate::ui::topbar_action::{ActionSpec, provide_topbar_action};
use crate::ui::{
    Drawer, EmptyState, IconName, MetricTone, ResourceCard, StatItem, StatStrip, Tag, TagTone,
    Toasts,
};
use crate::url_state::{DrawerKind, use_create_drawer, use_detail_drawer, use_drawer_tab};
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
    distinct(out)
}

/// Une pièce n'est proposée qu'une fois, même listée à deux titres.
fn distinct(mut components: Vec<PackComponent>) -> Vec<PackComponent> {
    let mut known = HashSet::new();
    components.retain(|component| known.insert(component_key(component)));
    components
}

fn component_key(component: &PackComponent) -> (&'static str, String) {
    (component.kind.as_str(), component.id.clone())
}

/// Case à cocher d'un composant à inclure dans le pack.
#[component]
fn ComponentChoice(component: PackComponent, selected: RwSignal<HashSet<String>>) -> impl IntoView {
    let id = component.id.clone();
    let toggled = component.id.clone();
    view! {
        <li class="step-item">
            <label class="step-item__head">
                <input type="checkbox"
                    prop:checked=move || selected.read().contains(&id)
                    on:change=move |_| {
                        selected.update(|set| {
                            if !set.remove(&toggled) {
                                set.insert(toggled.clone());
                            }
                        });
                    } />
                <span class="text-mono text-strong">{component.id}</span>
                <Tag label=component.kind.as_str() tone=TagTone::Neutral />
            </label>
        </li>
    }
}

#[component]
pub fn LocalPacksSection(
    snapshot: Loaded<LocalLibrarySnapshot>,
    can_manage: bool,
) -> impl IntoView {
    let live = use_live();
    let create_open = use_create_drawer(DrawerKind::Pack);
    let pack_name = RwSignal::new(String::new());
    let pack_desc = RwSignal::new(String::new());
    let selected = RwSignal::new(HashSet::<String>::new());
    let feedback = RwSignal::new(None::<String>);

    let (inspect_id, inspect_open) = use_detail_drawer(DrawerKind::LocalPack);
    let inspect_tab = RwSignal::new(0_usize);
    use_drawer_tab(inspect_tab);

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
            live.touch(LOCAL_TOPICS);
        }
        Some(Err(err)) => Toasts::set_error(feedback, err.message()),
        None => {}
    });

    let choices = Memo::new(move |_| {
        snapshot
            .get()
            .and_then(Result::ok)
            .map(|known| available_components(&known))
            .unwrap_or_default()
    });

    let remove = use_local_delete("Pack local supprimé.");

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

    // Le nom vient de la liste ; l'adresse ne porte que l'identifiant.
    let inspect_title = Signal::derive(move || {
        let id = inspect_id.get().unwrap_or_default();
        let data = snapshot.get().and_then(Result::ok);
        data.and_then(|data| data.packs.into_iter().find(|pack| pack.id == id))
            .map_or_else(|| "Pack local".to_owned(), |pack| pack.name)
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
                                    message="Regroupez vos scripts, ressources et modèles de flux dans un pack local réutilisable."
                                    icon_name=IconName::Package
                                />
                            </div>
                        }.into_any();
                    }
                    view! {
                        <div class="resource-grid resource-grid--list">
                            {shown.into_iter().map(|pack| {
                                let pid = pack.id.clone();
                                let card_id = pack.id.clone();
                                let open_inspect = Callback::new(move |()| {
                                    inspect_tab.set(0);
                                    inspect_id.set(Some(pid.clone()));
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
                                        focus_key=pack.id.clone()
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
            <form class="inspection-panel" on:submit=move |ev| {
                ev.prevent_default();
                let chosen_ids = selected.get_untracked();
                let mut components = choices.get_untracked();
                components.retain(|component| chosen_ids.contains(&component.id));
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
                    <Show
                        when=move || !choices.read().is_empty()
                        fallback=|| view! {
                            <p class="text-dim text-sm">"Créez d'abord un script, une ressource ou un modèle de flux."</p>
                        }
                    >
                        <ul class="step-list">
                            <For
                                each=move || choices.get()
                                key=component_key
                                children=move |component| view! {
                                    <ComponentChoice component=component selected=selected />
                                }
                            />
                        </ul>
                    </Show>
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
                />
            })}
        </Drawer>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_component_is_offered_once_per_kind() {
        let component = |id: &str, kind| PackComponent {
            id: id.into(),
            version: "1.0.0".into(),
            kind,
        };
        let script = component("a", CatalogItemType::Script);
        let flow = component("a", CatalogItemType::Flow);
        assert_eq!(
            distinct(vec![script.clone(), flow.clone(), script.clone()]),
            vec![script, flow]
        );
    }
}
