//! Gabarit commun des pages registre : tableau, puis bandeau de la ligne ouverte.

use leptos::prelude::*;

/// Tableau du registre. Le corps est rempli par le script client.
#[component]
pub fn RegistreTable(
    aria_label: &'static str,
    headers: Vec<&'static str>,
    body_id: &'static str,
    empty: &'static str,
) -> impl IntoView {
    let span = headers.len().to_string();
    view! {
        <div class="table-wrapper">
            <table class="data-table" aria-label=aria_label>
                <thead>
                    <tr>
                        {headers.into_iter().map(|header| view! {
                            <th scope="col">{header}</th>
                        }).collect_view()}
                    </tr>
                </thead>
                <tbody id=body_id>
                    <tr>
                        <td colspan=span class="empty-state-text">{empty}</td>
                    </tr>
                </tbody>
            </table>
        </div>
    }
}

/// Bandeau de la ligne ouverte : faits, puis un seul verbe.
#[component]
pub fn RegistreFact(
    id: &'static str,
    action_id: &'static str,
    action: &'static str,
) -> impl IntoView {
    let title_id = format!("{id}-title");
    let facts_id = format!("{id}-facts");
    view! {
        <div id=id class="registre-fact" hidden>
            <div class="registre-fact-copy">
                <p id=title_id class="registre-fact-title">"Sélectionnez une ligne"</p>
                <p id=facts_id class="registre-fact-facts"></p>
            </div>
            <button type="button" id=action_id class="btn btn-primary btn-sm registre-fact-action">
                {action}
            </button>
        </div>
    }
}
