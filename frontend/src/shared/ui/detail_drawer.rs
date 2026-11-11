//! Composant DetailDrawer : Tiroir latéral contextuel pour l'inspection et la configuration sans modale bloquante.

use leptos::prelude::*;

#[component]
pub fn DetailDrawer(
    #[prop(into)] id: String,
    #[prop(into)] title: String,
    #[prop(into, optional)] subtitle: Option<String>,
    #[prop(into, optional)] eyebrow: Option<String>,
    children: Children,
) -> impl IntoView {
    let scrim_id = id.clone();
    let btn_id = id.clone();
    view! {
        <aside
            id=id
            class="detail-drawer"
            role="dialog"
            aria-modal="true"
            aria-hidden="true"
            style="display: none;"
        >
            <div class="drawer-scrim" data-drawer-close=scrim_id></div>
            <div class="drawer-panel">
                <header class="drawer-header">
                    <div class="drawer-header-content">
                        {eyebrow.map(|eb| view! {
                            <span class="drawer-eyebrow">{eb}</span>
                        })}
                        <h3 class="drawer-title">{title}</h3>
                        {subtitle.map(|sub| view! {
                            <p class="drawer-subtitle">{sub}</p>
                        })}
                    </div>
                    <button
                        type="button"
                        class="btn btn-secondary btn-sm drawer-close-btn"
                        data-drawer-close=btn_id
                        aria-label="Fermer le tiroir"
                    >
                        "✕"
                    </button>
                </header>

                <div class="drawer-body">
                    {children()}
                </div>

                <footer class="drawer-footer">
                    <button
                        type="button"
                        class="btn btn-secondary btn-sm"
                        data-drawer-action="cancel"
                    >
                        "Fermer"
                    </button>
                    <button
                        type="button"
                        class="btn btn-primary btn-sm"
                        data-drawer-action="confirm"
                    >
                        "Valider"
                    </button>
                </footer>
            </div>
        </aside>
    }
}
