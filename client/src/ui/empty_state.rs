//! Composant EmptyState : État vide sémantique et réutilisable, sans style en ligne.

use super::icon::{Icon, IconName};
use leptos::prelude::*;

const EMPTY_ICON_SIZE: u32 = 28;

#[component]
pub fn EmptyState(
    #[prop(into)] title: String,
    #[prop(into, optional)] message: Option<String>,
    #[prop(optional)] icon_name: Option<IconName>,
    #[prop(optional)] action: Option<AnyView>,
) -> impl IntoView {
    view! {
        <div class="empty-state">
            {icon_name.map(|name| view! {
                <span class="empty-state-icon" aria-hidden="true">
                    <Icon name=name size=EMPTY_ICON_SIZE />
                </span>
            })}
            <h3 class="empty-state-title">{title}</h3>
            {message.map(|msg| view! {
                <p class="empty-state-message">{msg}</p>
            })}
            {action.map(|content| view! {
                <div class="empty-state__action">{content}</div>
            })}
        </div>
    }
}
