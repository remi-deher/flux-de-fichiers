//! Palette : les blocs exécutables du catalogue partagé, par catégorie.

use super::state::Editor;
use flux_shared::flows::{FlowBlockKind, block_catalog};
use leptos::prelude::*;

/// Catégories dans l'ordre de première apparition du catalogue.
pub fn categories() -> Vec<&'static str> {
    let mut list = Vec::new();
    for spec in block_catalog() {
        if !list.contains(&spec.category) {
            list.push(spec.category);
        }
    }
    list
}

#[component]
pub fn Palette(editor: Editor) -> impl IntoView {
    view! {
        <aside class="flow-palette" aria-label="Blocs disponibles">
            <h2 class="section-title">"Blocs"</h2>
            {categories().into_iter().map(|category| view! {
                <div>
                    <p class="nav-group-label">{category}</p>
                    {block_catalog().iter().filter(|spec| spec.category == category).map(|spec| view! {
                        <button type="button" class="btn btn-secondary btn-sm flow-palette__item" title=spec.description
                            on:click=move |_| {
                                let mut added = String::new();
                                editor.edit(|graph| {
                                    added = graph.append(FlowBlockKind::parse(spec.kind), spec.label, spec.default_config());
                                });
                                editor.selected.set(Some(added));
                            }>
                            {spec.label}
                        </button>
                    }).collect_view()}
                </div>
            }).collect_view()}
        </aside>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_block_belongs_to_a_listed_category() {
        let listed = categories();
        assert!(
            block_catalog()
                .iter()
                .all(|spec| listed.contains(&spec.category))
        );
        assert_eq!(listed.first(), Some(&"Entrée"));
    }
}
