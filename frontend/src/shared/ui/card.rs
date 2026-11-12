//! Composant Carte / Panneau de surface atomique réutilisable.

use leptos::prelude::*;

/// Conteneur de surface Material Design 3 Expressive avec bordure et élévation.
#[component]
pub fn Card(
    #[prop(optional)] id: Option<&'static str>,
    #[prop(optional)] class: Option<&'static str>,
    #[prop(optional)] aria_labelledby: Option<&'static str>,
    children: Children,
) -> impl IntoView {
    let final_class = match class {
        Some(extra) => format!("section-panel {extra}"),
        None => "section-panel".to_string(),
    };

    view! {
        <section
            id=id
            class=final_class
            aria-labelledby=aria_labelledby
        >
            {children()}
        </section>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_card_classes() {
        let card = view! {
            <Card id="test-card" class="custom-card">
                <p>"Contenu"</p>
            </Card>
        };
        let html = card.to_html();
        assert!(html.contains("class=\"section-panel custom-card\""));
        assert!(html.contains("id=\"test-card\""));
    }
}
