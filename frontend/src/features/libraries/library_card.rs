//! Carte descriptive d'une bibliothèque média surveillée sans masque rigide.

use super::node_access::{LibraryNodeAccess, NodePathConfig};
use leptos::prelude::*;

#[component]
pub fn LibraryCard(
    name: &'static str,
    media_type: &'static str,
    status: &'static str,
    assigned_flow: &'static str,
    nodes: Vec<NodePathConfig>,
) -> impl IntoView {
    view! {
        <article class="card" style="display: flex; flex-direction: column; gap: 0.85rem; padding: 1.25rem; border: 1px solid var(--border-color); border-radius: var(--radius-md);">
            <div style="display: flex; justify-content: space-between; align-items: flex-start; gap: 1rem; flex-wrap: wrap;">
                <div>
                    <div style="display: flex; align-items: center; gap: 0.5rem; margin-bottom: 0.25rem;">
                        <h3 style="margin: 0; font-size: 1.125rem; font-weight: 600;">
                            {name}
                        </h3>
                        <span class="status-pill info">
                            {media_type}
                        </span>
                    </div>
                    <p style="margin: 0; font-size: 0.875rem; color: var(--text-muted);">
                        "Tous types de médias détectés automatiquement sans contrainte d'extension."
                    </p>
                </div>

                <div style="display: flex; align-items: center; gap: 0.5rem;">
                    <span class="status-pill success">
                        <span class="status-dot"></span>
                        {status}
                    </span>
                </div>
            </div>

            // Flow assigné à cette bibliothèque
            <div style="display: flex; align-items: center; justify-content: space-between; flex-wrap: wrap; gap: 0.5rem; background: var(--bg-subtle); padding: 0.5rem 0.75rem; border-radius: var(--radius-sm); font-size: 0.875rem;">
                <div style="display: flex; align-items: center; gap: 0.5rem;">
                    <span style="font-weight: 600; color: var(--text-main);">"⚡ Flow assigné :"</span>
                    <span style="color: var(--primary); font-weight: 500;">{assigned_flow}</span>
                </div>
                <span style="font-size: 0.75rem; color: var(--text-dim);">"Déclenché automatiquement à l'ingestion"</span>
            </div>

            // Configuration multi-nœuds des chemins
            <LibraryNodeAccess nodes=nodes />
        </article>
    }
}
