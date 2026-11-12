//! Gabarit commun des pages registre : le tableau ; la ligne ouverte s'affiche dans le tiroir d'inspection.

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
