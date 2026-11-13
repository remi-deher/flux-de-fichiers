//! Renvoi vers l'écran porté dans le client WASM (ADR 0014).

use leptos::prelude::*;

#[component]
pub fn NewInterfaceLink(href: &'static str, purpose: &'static str) -> impl IntoView {
    view! {
        <p class="form-help">
            {purpose}
            " : "
            <a href=href>"nouvelle interface"</a>
        </p>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_link_names_its_purpose_and_target() {
        let html =
            view! { <NewInterfaceLink href="/app/flows" purpose="Éditer les flux" /> }.to_html();
        assert!(html.contains("href=\"/app/flows\""));
        assert!(html.contains("Éditer les flux"));
    }
}
