//! Dock Palette : recherche, catégories repliables, épinglage et insertion sur branche.

use super::dnd::{DragPayload, insert_subgraph};
use super::drag::DragItem;
use super::presentation::{NodeDomain, output_presentation};
use super::state::Editor;
use crate::api;
use crate::route::{Link, Route};
use crate::ui::search::{matches, query as global_query};
use crate::ui::{Icon, IconName, Toasts};
use flux_shared::flows::{FlowGraph, OwnedBlockSpec};
use flux_shared::local::LocalLibrarySnapshot;
use leptos::prelude::*;

const HEADER_ICON_SIZE: u32 = 15;
const CARD_ICON_SIZE: u32 = 15;

/// Catégories dans l'ordre de première apparition du catalogue.
pub fn categories(catalog: &[OwnedBlockSpec]) -> Vec<String> {
    let mut list = Vec::new();
    for spec in catalog {
        if !list.iter().any(|category| category == &spec.category) {
            list.push(spec.category.clone());
        }
    }
    list
}

/// Filtre les composants selon une requête libre (libellé, catégorie, type, description).
pub fn filter_specs(catalog: &[OwnedBlockSpec], query: &str) -> Vec<OwnedBlockSpec> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return catalog.to_vec();
    }
    catalog
        .iter()
        .filter(|spec| {
            matches(
                &[&spec.label, &spec.category, &spec.kind, &spec.description],
                trimmed,
            )
        })
        .cloned()
        .collect()
}

/// Regroupe les fiches filtrées par catégorie en conservant leur ordre d'apparition.
pub fn grouped_specs(
    catalog: &[OwnedBlockSpec],
    query: &str,
) -> Vec<(String, Vec<OwnedBlockSpec>)> {
    let filtered = filter_specs(catalog, query);
    categories(&filtered)
        .into_iter()
        .map(|category| {
            let items: Vec<OwnedBlockSpec> = filtered
                .iter()
                .filter(|spec| spec.category == category)
                .cloned()
                .collect();
            (category, items)
        })
        .collect()
}

#[component]
pub fn Palette(editor: Editor) -> impl IntoView {
    let local_query = RwSignal::new(String::new());
    let local_snapshot = LocalResource::new(|| api::get::<LocalLibrarySnapshot>("/api/v1/local"));
    let search_ref = NodeRef::<leptos::html::Input>::new();
    let last_tick = RwSignal::new(0_u64);
    Effect::new(move |_| {
        let tick = editor.palette_focus_tick.get();
        if tick > last_tick.get_untracked() {
            last_tick.set(tick);
            if let Some(input) = search_ref.get() {
                let _ = input.focus();
            }
        }
    });

    let groups = Memo::new(move |_| {
        let catalog = editor.catalog.get();
        let after_local = filter_specs(&catalog, &local_query.get());
        grouped_specs(&after_local, &global_query())
    });

    view! {
        <div class="flow-dock-pane flow-palette-pane" aria-label="Composants de flux">
            <div class="flow-palette__subbar">
                <span class="flow-palette__badge">
                    {move || format!("{} composants", editor.catalog.with(|c| c.len()))}
                </span>
                <button
                    type="button"
                    class="header-icon-btn"
                    class:is-active=move || editor.palette_pinned.get()
                    aria-pressed=move || editor.palette_pinned.get().to_string()
                    title="Garder les composants ouverts après un ajout"
                    aria-label="Garder les composants ouverts après un ajout"
                    on:click=move |_| editor.palette_pinned.update(|pinned| *pinned = !*pinned)
                >
                    <Icon name=IconName::Pin size=HEADER_ICON_SIZE />
                </button>
            </div>

            {move || {
                let target = editor.branch_target.get()?;
                let source_name = editor.graph.with(|graph| {
                    graph
                        .node(&target.source_id)
                        .map_or_else(|| target.source_id.clone(), |node| node.name.clone())
                });
                let port_label =
                    output_presentation(target.port.as_deref().unwrap_or("default")).label;
                Some(view! {
                    <div class="flow-palette__insertion" role="status">
                        <span class="flow-palette__insertion-text">
                            {format!("Insertion après « {source_name} » ({port_label})")}
                        </span>
                        <button
                            type="button"
                            class="btn btn-secondary btn-sm"
                            on:click=move |_| editor.branch_target.set(None)
                        >
                            "Annuler"
                        </button>
                    </div>
                })
            }}

            <div class="flow-palette__search">
                <Icon name=IconName::Search size=14 class="flow-palette__search-icon" />
                <input
                    node_ref=search_ref
                    type="search"
                    class="form-input flow-palette__search-input"
                    placeholder="Filtrer (/)…"
                    aria-label="Rechercher un composant"
                    prop:value=move || local_query.get()
                    on:input=move |event| local_query.set(event_target_value(&event))
                />
                {move || (!local_query.get().is_empty()).then(|| view! {
                    <button
                        type="button"
                        class="flow-palette__search-clear"
                        aria-label="Effacer la recherche"
                        title="Effacer la recherche"
                        on:click=move |_| local_query.set(String::new())
                    >
                        <Icon name=IconName::Close size=12 />
                    </button>
                })}
            </div>

            <div class="flow-palette__groups">
                {move || {
                    let list = groups.get();
                    if list.is_empty() {
                        if !editor.catalog_loaded.get() {
                            return view! {
                                <p class="flow-palette__empty">"Chargement des blocs…"</p>
                            }
                            .into_any();
                        }
                        if editor.catalog.with(|c| c.is_empty()) {
                            return view! {
                                <div class="flow-palette__empty">
                                    <p>"Catalogue de blocs vide : synchronisez un dépôt depuis Extensions."</p>
                                    <Link to=Route::Plugins class="btn btn-secondary btn-sm">
                                        "Ouvrir Extensions"
                                    </Link>
                                </div>
                            }
                            .into_any();
                        }
                        return view! {
                            <p class="flow-palette__empty">
                                "Aucun composant ne correspond à cette recherche."
                            </p>
                        }
                        .into_any();
                    }
                    list.into_iter()
                        .map(|(category, items)| {
                            let count = items.len();
                            view! {
                                <details class="flow-palette__group" open=true>
                                    <summary class="flow-palette__summary">
                                        <span>{category}</span>
                                        <span class="flow-palette__count">{count}</span>
                                    </summary>
                                    <div class="flow-palette__list">
                                        {items
                                            .into_iter()
                                            .map(|spec| {
                                                let kind = spec.kind.clone();
                                                let label = spec.label.clone();
                                                let description = spec.description.clone();
                                                let defaults = spec.default_config();
                                                let domain = NodeDomain::from_spec(
                                                    Some(&spec),
                                                    &spec.category,
                                                );
                                                let plugin_label = "Plugin";
                                                let accent_style =
                                                    format!("--component-accent: {}", domain.accent_var());
                                                let payload = DragPayload::Palette {
                                                    kind,
                                                    label: label.clone(),
                                                    defaults,
                                                };
                                                let click_spec = spec.clone();
                                                let key_spec = spec;
                                                let add_block = move |chosen: &OwnedBlockSpec| {
                                                    if let Err(err) = editor.add_or_insert_block(chosen) {
                                                        Toasts::notify_error(err.message());
                                                    }
                                                };
                                                view! {
                                                    <DragItem editor=editor payload=payload class="flow-palette__drag">
                                                        <div
                                                            role="button"
                                                            tabindex="0"
                                                            class="flow-palette__card"
                                                            style=accent_style
                                                            title=description
                                                            on:click=move |_| add_block(&click_spec)
                                                            on:keydown=move |event| {
                                                                let key = event.key();
                                                                if key == "Enter" || key == " " {
                                                                    event.prevent_default();
                                                                    add_block(&key_spec);
                                                                }
                                                            }
                                                        >
                                                            <span class="flow-palette__card-icon" aria-hidden="true">
                                                                <Icon name=domain.icon() size=CARD_ICON_SIZE />
                                                            </span>
                                                            <span class="flow-palette__card-text">
                                                                <span class="flow-palette__card-title">{label}</span>
                                                                <span class="flow-palette__card-meta">{plugin_label}</span>
                                                            </span>
                                                            <span class="flow-palette__card-plus" aria-hidden="true">"+"</span>
                                                        </div>
                                                    </DragItem>
                                                }
                                            })
                                            .collect_view()}
                                    </div>
                                </details>
                            }
                        })
                        .collect_view()
                        .into_any()
                }}
                {move || {
                    let templates = local_snapshot
                        .get()
                        .and_then(Result::ok)
                        .map(|snap| snap.block_templates)
                        .unwrap_or_default();
                    let q_local = local_query.get();
                    let q_global = global_query();
                    let filtered: Vec<_> = templates
                        .into_iter()
                        .filter(|tpl| {
                            matches(
                                &[
                                    &tpl.name,
                                    &tpl.id,
                                    tpl.description.as_deref().unwrap_or(""),
                                ],
                                &q_local,
                            ) && matches(
                                &[
                                    &tpl.name,
                                    &tpl.id,
                                    tpl.description.as_deref().unwrap_or(""),
                                ],
                                &q_global,
                            )
                        })
                        .collect();
                    (!filtered.is_empty()).then(|| {
                        let count = filtered.len();
                        view! {
                            <details class="flow-palette__group" open=true>
                                <summary class="flow-palette__summary">
                                    <span>"Modèles de blocs"</span>
                                    <span class="flow-palette__count">{count}</span>
                                </summary>
                                <div class="flow-palette__list">
                                    {filtered.into_iter().map(|tpl| {
                                        let name = tpl.name.clone();
                                        let desc = tpl.description.clone().unwrap_or_else(|| "Sous-graphe pré-configuré".into());
                                        let def = tpl.definition.clone();
                                        let insert = move || {
                                            let Ok(parsed) = serde_json::from_value::<FlowGraph>(def.clone()) else {
                                                Toasts::notify_error("Modèle de blocs invalide.");
                                                return;
                                            };
                                            editor.mutate_graph(|graph| {
                                                let created = insert_subgraph(graph, &parsed);
                                                if let Some(first) = created.first() {
                                                    editor.selected.set(Some(first.clone()));
                                                }
                                            });
                                            Toasts::notify_success(format!("Modèle « {} » inséré.", name));
                                        };
                                        let insert_click = insert.clone();
                                        view! {
                                            <div
                                                role="button"
                                                tabindex="0"
                                                class="flow-palette__card"
                                                title=desc
                                                on:click=move |_| insert_click()
                                                on:keydown=move |event| {
                                                    if event.key() == "Enter" || event.key() == " " {
                                                        event.prevent_default();
                                                        insert();
                                                    }
                                                }
                                            >
                                                <span class="flow-palette__card-icon" aria-hidden="true">
                                                    <Icon name=IconName::Layers size=CARD_ICON_SIZE />
                                                </span>
                                                <span class="flow-palette__card-text">
                                                    <span class="flow-palette__card-title">{tpl.name}</span>
                                                    <span class="flow-palette__card-meta">"Modèle de blocs"</span>
                                                </span>
                                                <span class="flow-palette__card-plus" aria-hidden="true">"+"</span>
                                            </div>
                                        }
                                    }).collect_view()}
                                </div>
                            </details>
                        }
                    })
                }}
            </div>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::test_catalog;
    use super::*;

    #[test]
    fn every_block_belongs_to_a_listed_category() {
        let catalog = test_catalog();
        let listed = categories(&catalog);
        assert!(catalog.iter().all(|spec| listed.contains(&spec.category)));
        assert_eq!(listed.first().map(String::as_str), Some("Entrée"));
        assert!(catalog.iter().any(|spec| spec.kind == "block.video.encode"));
    }

    #[test]
    fn filter_specs_and_grouped_specs_narrow_results_preserving_category_order() {
        let catalog = test_catalog();
        let grouped = grouped_specs(&catalog, "sonarr");
        assert_eq!(grouped.len(), 1);
        assert_eq!(grouped[0].0, "Intégrations");
        assert_eq!(grouped[0].1.len(), 2);

        let all = grouped_specs(&catalog, "   ");
        assert_eq!(all.len(), categories(&catalog).len());
    }
}
