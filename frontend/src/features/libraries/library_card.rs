//! Carte descriptive d'une bibliothèque média surveillée.

use leptos::prelude::*;

#[component]
pub fn LibraryCard(
    name: &'static str,
    path: &'static str,
    pattern: &'static str,
    status: &'static str,
    flow: &'static str,
) -> impl IntoView {
    view! {
        <div class="card" style="display: flex; flex-direction: column; gap: 0.75rem; padding: 1rem;">
            <div style="display: flex; justify-content: space-between; align-items: center;">
                <h3 style="margin: 0; font-size: 1.05rem; font-weight: 600;">
                    {name}
                </h3>
                <span class="status-pill success">
                    {status}
                </span>
            </div>
            <div style="font-size: 0.875rem; color: var(--text-muted); display: grid; grid-template-columns: auto 1fr; gap: 0.25rem 0.75rem;">
                <span style="font-weight: 500;">"Chemin :"</span>
                <code>{path}</code>
                <span style="font-weight: 500;">"Masque :"</span>
                <code>{pattern}</code>
                <span style="font-weight: 500;">"Flux cible :"</span>
                <span>{flow}</span>
            </div>
        </div>
    }
}
