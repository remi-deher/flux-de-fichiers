//! Palette : blocs natifs et blocs des dépôts de plugins, par catégorie.

use super::dnd::DragPayload;
use super::drag::DragItem;
use super::state::Editor;
use flux_shared::flows::{FlowBlockKind, OwnedBlockSpec};
use leptos::prelude::*;

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

#[component]
pub fn Palette(editor: Editor) -> impl IntoView {
    view! {
        <aside class="flow-palette" aria-label="Blocs disponibles">
            <h2 class="section-title">"Blocs"</h2>
            <p class="form-help">"Cliquez pour ajouter, ou glissez un bloc sur une carte."</p>
            {move || {
                let catalog = editor.catalog.get();
                categories(&catalog).into_iter().map(|category| {
                    let items: Vec<_> = catalog.iter().filter(|spec| spec.category == category).cloned().collect();
                    view! {
                        <div>
                            <p class="nav-group-label">{category.clone()}</p>
                            {items.into_iter().map(|spec| {
                                let kind = spec.kind.clone();
                                let label = spec.label.clone();
                                let description = spec.description.clone();
                                let defaults = spec.default_config();
                                let button_label = spec.label.clone();
                                let payload = DragPayload::Palette {
                                    kind: kind.clone(),
                                    label: label.clone(),
                                    defaults: defaults.clone(),
                                };
                                view! {
                                    <DragItem editor=editor payload=payload>
                                    <button type="button" class="btn btn-secondary btn-sm flow-palette__item" title=description
                                        on:click=move |_| {
                                            let mut added = String::new();
                                            let kind = FlowBlockKind::parse(&kind);
                                            let label = label.clone();
                                            let defaults = defaults.clone();
                                            editor.edit(|graph| {
                                                added = graph.append(kind, &label, defaults);
                                            });
                                            editor.selected.set(Some(added));
                                        }>
                                        {button_label}
                                    </button>
                                    </DragItem>
                                }
                            }).collect_view()}
                        </div>
                    }
                }).collect_view()
            }}
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
}
