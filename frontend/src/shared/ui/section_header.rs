//! Composant SectionHeader : En-tête générique de section de panneau.

use leptos::prelude::*;

#[component]
pub fn SectionHeader(
    #[prop(into)] title: String,
    #[prop(into, optional)] id: Option<&'static str>,
    #[prop(into, optional)] subtitle: Option<String>,
    #[prop(optional)] actions: Option<AnyView>,
) -> impl IntoView {
    view! {
        <div class="section-panel-header">
            <div>
                <h2 id=id class="section-title">{title}</h2>
                {subtitle.map(|sub| view! {
                    <p class="section-subtitle">{sub}</p>
                })}
            </div>
            {actions.map(|content| view! {
                <div class="section-panel-header__actions">{content}</div>
            })}
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_header_without_actions_has_no_actions_slot() {
        let html = view! { <SectionHeader title="Titre" /> }.to_html();
        assert!(html.contains("class=\"section-title\""));
        assert!(!html.contains("section-panel-header__actions"));
    }

    #[test]
    fn test_header_renders_actions_slot() {
        let actions = view! { <button>"Voir"</button> }.into_any();
        let html = view! { <SectionHeader title="Titre" actions=actions /> }.to_html();
        assert!(html.contains("section-panel-header__actions"));
        assert!(html.contains("Voir"));
    }
}
