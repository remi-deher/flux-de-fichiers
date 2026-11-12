//! Ligne « libellé / valeur » d'une grille de détail (`detail-grid`).

use leptos::prelude::*;

/// Valeur affichée tant que le script client n'a rien renseigné.
const PLACEHOLDER_VALUE: &str = "—";

#[component]
pub fn DetailRow(
    label: &'static str,
    #[prop(optional)] id: Option<&'static str>,
    #[prop(optional)] children: Option<Children>,
) -> impl IntoView {
    view! {
        <div class="detail-term">{label}</div>
        <div id=id class="detail-val">
            {match children {
                Some(content) => content().into_any(),
                None => PLACEHOLDER_VALUE.into_any(),
            }}
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_row_without_children_shows_the_placeholder() {
        let html = view! { <DetailRow label="Libellé" id="row-id" /> }.to_html();
        assert!(html.contains("class=\"detail-term\""));
        assert!(html.contains("id=\"row-id\""));
        assert!(html.contains(PLACEHOLDER_VALUE));
    }

    #[test]
    fn test_row_with_children_replaces_the_placeholder() {
        let html = view! { <DetailRow label="Libellé"><code>"valeur"</code></DetailRow> }.to_html();
        assert!(html.contains("<code>"));
        assert!(!html.contains(PLACEHOLDER_VALUE));
    }
}
