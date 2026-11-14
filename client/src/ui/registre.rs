//! Registre : tableau de données du thème (`data-table`), ligne ouverte mise en évidence.

use leptos::prelude::*;

#[component]
pub fn Registre(
    aria_label: &'static str,
    headers: Vec<&'static str>,
    children: Children,
) -> impl IntoView {
    view! {
        <div class="table-wrapper">
            <table class="data-table" aria-label=aria_label>
                <thead>
                    <tr>
                        {headers.into_iter().map(|header| view! { <th scope="col">{header}</th> }).collect_view()}
                    </tr>
                </thead>
                <tbody>{children()}</tbody>
            </table>
        </div>
    }
}

/// Ligne unique d'un registre vide, en chargement ou en erreur.
#[component]
pub fn RegistreMessage(span: usize, #[prop(into)] text: String) -> impl IntoView {
    view! { <tr><td colspan=span.to_string() class="empty-state-text">{text}</td></tr> }
}
