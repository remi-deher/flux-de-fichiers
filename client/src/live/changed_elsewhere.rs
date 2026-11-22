//! Avertissement d'un formulaire ouvert dont le contenu a changé ailleurs.

use crate::plugins::{StatusBanner, StoreTone};
use leptos::prelude::*;

/// Bandeau « modifié ailleurs » ; `on_reload` ajoute le bouton qui remplace la saisie.
#[component]
pub fn ChangedElsewhere(
    text: &'static str,
    #[prop(optional_no_strip)] on_reload: Option<Callback<()>>,
) -> impl IntoView {
    let action = on_reload.map(|on_reload| {
        view! {
            <button type="button" class="btn btn-secondary btn-sm" on:click=move |_| on_reload.run(())>
                "Recharger"
            </button>
        }
        .into_any()
    });
    view! {
        <StatusBanner title="Modifié ailleurs" text=Some(text.to_owned()) tone=StoreTone::Warning action=action />
    }
}
