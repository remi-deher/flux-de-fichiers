//! Dock Palette : recherche, catégories repliables, épinglage et insertion sur branche.

use super::dnd::DragPayload;
use super::drag::DragItem;
use super::presentation::{NodeDomain, output_presentation};
use super::state::Editor;
use crate::ui::search::{matches, query as global_query};
use crate::ui::{Icon, IconName, Toasts};
use flux_shared::flows::OwnedBlockSpec;
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
    let groups = Memo::new(move |_| {
        let catalog = editor.catalog.get();
        let after_local = filter_specs(&catalog, &local_query.get());
        grouped_specs(&after_local, &global_query())
    });

    view! {
        <aside
            class="flow-palette-dock"
            class:is-closed=move || !editor.palette_open.get()
            aria-label="Composants de flux"
        >
            <header class="flow-palette__header">
                <div class="flow-palette__title-wrap">
                    <h2 class="flow-palette__title">"Composants"</h2>
                    <span class="flow-palette__badge">
                        {move || editor.catalog.with(|c| c.len())}
                    </span>
                </div>
                <div class="flow-palette__header-actions">
                    <button
                        type="button"
                        class="header-icon-btn"
                        class:is-active=move || editor.palette_pinned.get()
                        aria-pressed=move || editor.palette_pinned.get().to_string()
                        title="Garder la palette ouverte après un ajout"
                        aria-label="Garder la palette ouverte après un ajout"
                        on:click=move |_| editor.palette_pinned.update(|pinned| *pinned = !*pinned)
                    >
                        <Icon name=IconName::Pin size=HEADER_ICON_SIZE />
                    </button>
                    <button
                        type="button"
                        class="header-icon-btn"
                        title="Fermer la palette (Ctrl+B)"
                        aria-label="Fermer la palette"
                        on:click=move |_| editor.palette_open.set(false)
                    >
                        <Icon name=IconName::Close size=HEADER_ICON_SIZE />
                    </button>
                </div>
            </header>

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
                    type="search"
                    class="form-input flow-palette__search-input"
                    placeholder="Rechercher un composant…"
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
                                                let domain = NodeDomain::from_block(
                                                    &spec.kind_enum(),
                                                    &spec.category,
                                                );
                                                let plugin_label =
                                                    if spec.from_plugin { "Plugin" } else { "Natif" };
                                                let accent_style =
                                                    format!("--component-accent: {}", domain.accent_var());
                                                let payload = DragPayload::Palette {
                                                    kind,
                                                    label: label.clone(),
                                                    defaults,
                                                };
                                                let click_spec = spec.clone();
                                                view! {
                                                    <DragItem editor=editor payload=payload class="flow-palette__drag">
                                                        <button
                                                            type="button"
                                                            class="flow-palette__card"
                                                            style=accent_style
                                                            title=description
                                                            on:click=move |_| {
                                                                if let Err(err) = editor.add_or_insert_block(&click_spec) {
                                                                    Toasts::notify_error(err.message());
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
                                                        </button>
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
            </div>
        </aside>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use flux_shared::flows::native_owned_catalog;

    #[test]
    fn every_block_belongs_to_a_listed_category() {
        let catalog = native_owned_catalog();
        let listed = categories(&catalog);
        assert!(catalog.iter().all(|spec| listed.contains(&spec.category)));
        assert_eq!(listed.first().map(String::as_str), Some("Entrée"));
        assert!(catalog.iter().any(|spec| spec.kind == "block.video.encode"));
    }

    #[test]
    fn filter_specs_and_grouped_specs_narrow_results_preserving_category_order() {
        let catalog = native_owned_catalog();
        let grouped = grouped_specs(&catalog, "sonarr");
        assert_eq!(grouped.len(), 1);
        assert_eq!(grouped[0].0, "Intégrations");
        assert_eq!(grouped[0].1.len(), 2);

        let all = grouped_specs(&catalog, "   ");
        assert_eq!(all.len(), categories(&catalog).len());
    }
}
