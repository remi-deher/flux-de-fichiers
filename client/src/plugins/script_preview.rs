//! Aperçu en lecture seule du code d'un script d'un dépôt, affiché dans le tiroir du dépôt.

use super::REPOSITORIES_PATH;
use crate::api;
use crate::route::encode_segment;
use crate::ui::{Tag, TagTone};
use forgearr_shared::local::LocalScriptSummary;
use leptos::prelude::*;

/// Chemin de l'API qui renvoie le code d'un script d'un dépôt.
pub fn script_preview_path(repo_id: &str, entry_id: &str) -> String {
    format!(
        "{REPOSITORIES_PATH}/{}/scripts/{}",
        encode_segment(repo_id),
        encode_segment(entry_id)
    )
}

#[component]
pub fn ScriptPreview(
    repo_id: String,
    entry_id: String,
    on_back: Callback<()>,
) -> impl IntoView {
    let path = script_preview_path(&repo_id, &entry_id);
    let script = LocalResource::new(move || {
        let path = path.clone();
        async move { api::get::<LocalScriptSummary>(&path).await }
    });
    view! {
        <div class="pack-components">
            <button type="button" class="btn btn-secondary btn-sm" on:click=move |_| on_back.run(())>
                "← Retour au contenu"
            </button>
            {move || match script.get() {
                None => view! { <p class="text-dim">"Chargement du script…"</p> }.into_any(),
                Some(Err(error)) => view! { <p class="text-danger">{error.message()}</p> }.into_any(),
                Some(Ok(script)) => view! {
                    <h3 class="form-label">{script.name}</h3>
                    <div class="cluster">
                        <Tag label=script.interpreter.executable() tone=TagTone::Primary />
                        <Tag label=script.entrypoint tone=TagTone::Mono />
                    </div>
                    {script.description.map(|text| view! { <p class="text-dim text-sm">{text}</p> })}
                    <pre class="code-block-wrapper"><code>{script.code}</code></pre>
                }.into_any(),
            }}
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_preview_path_encodes_both_segments() {
        let path = script_preview_path("repo.custom", "script.tag mkv");
        assert!(path.starts_with(REPOSITORIES_PATH));
        assert!(path.contains("/repo.custom/scripts/"));
        assert!(!path.contains(' '));
    }
}
