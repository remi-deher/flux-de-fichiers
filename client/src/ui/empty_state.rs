//! Composant EmptyState : État vide sémantique et réutilisable.

use leptos::prelude::*;

#[component]
pub fn EmptyState(
    #[prop(into)] title: String,
    #[prop(into, optional)] message: Option<String>,
    #[prop(into, optional)] icon: Option<&'static str>,
) -> impl IntoView {
    view! {
        <div class="empty-state">
            {icon.map(|ic| view! {
                <span class="empty-state-icon" aria-hidden="true" style="font-size: 2rem; margin-bottom: 0.5rem; display: block;">
                    {ic}
                </span>
            })}
            <h3 class="empty-state-title" style="font-size: 1rem; font-weight: 600; color: var(--text-main); margin-bottom: 0.25rem;">
                {title}
            </h3>
            {message.map(|msg| view! {
                <p class="empty-state-message" style="font-size: 0.875rem; color: var(--text-muted); margin: 0;">
                    {msg}
                </p>
            })}
        </div>
    }
}
