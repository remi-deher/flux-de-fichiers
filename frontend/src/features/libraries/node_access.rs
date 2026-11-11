//! Configuration et affichage des accès aux dossiers par nœud (inspiré d'encoding-central).

use leptos::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NodePathConfig {
    pub node_id: &'static str,
    pub node_name: &'static str,
    pub local_path: &'static str,
    pub access_mode: &'static str,
    pub is_online: bool,
    pub is_writable: bool,
}

#[component]
pub fn LibraryNodeAccess(nodes: Vec<NodePathConfig>) -> impl IntoView {
    view! {
        <div class="node-access-container" style="margin-top: 0.75rem; border-top: 1px solid var(--border-color); padding-top: 0.75rem;">
            <div style="display: flex; justify-content: space-between; align-items: center; margin-bottom: 0.5rem;">
                <span style="font-weight: 600; font-size: 0.8125rem; text-transform: uppercase; letter-spacing: 0.05em; color: var(--text-dim);">
                    "Accès aux dossiers par nœud"
                </span>
                <span style="font-size: 0.75rem; color: var(--text-muted);">
                    "Multi-nœuds & Points de montage spécifiques"
                </span>
            </div>

            <div style="display: flex; flex-direction: column; gap: 0.5rem;">
                {nodes.into_iter().map(|node| {
                    view! {
                        <div
                            class="node-access-row"
                            style="display: flex; justify-content: space-between; align-items: center; flex-wrap: wrap; gap: 0.5rem; background: var(--bg-subtle); padding: 0.5rem 0.75rem; border-radius: var(--radius-sm); font-size: 0.875rem;"
                        >
                            <div style="display: flex; align-items: center; gap: 0.5rem;">
                                <span class="status-dot" style=if node.is_online { "background: var(--success-border);" } else { "background: var(--text-dim);" }></span>
                                <span style="font-weight: 600;">{node.node_name}</span>
                                <span style="font-size: 0.75rem; color: var(--text-dim);">
                                    {format!("({})", node.access_mode)}
                                </span>
                            </div>

                            <div style="display: flex; align-items: center; gap: 0.75rem;">
                                <code style="background: var(--bg-card); padding: 0.2rem 0.4rem; border-radius: 4px; font-size: 0.8125rem;">
                                    {node.local_path}
                                </code>
                                <span class="status-pill" style="font-size: 0.75rem; padding: 0.15rem 0.5rem;">
                                    {if node.is_writable { "Lecture / Écriture" } else { "Lecture seule" }}
                                </span>
                            </div>
                        </div>
                    }
                }).collect_view()}
            </div>
        </div>
    }
}
