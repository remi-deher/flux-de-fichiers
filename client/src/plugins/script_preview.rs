//! Aperçu en lecture seule du code d'un script d'un dépôt, affiché dans le tiroir du dépôt.

use super::REPOSITORIES_PATH;
use crate::api;
use crate::route::encode_segment;
use crate::ui::{Tag, TagTone, VariablesTable};
use forgearr_shared::exposed_variables::RequirementStatus;
use forgearr_shared::highlight::{CodeToken, ScriptPreviewView};
use leptos::prelude::*;

/// Chemin de l'API qui renvoie le code d'un script d'un dépôt.
pub fn script_preview_path(repo_id: &str, entry_id: &str) -> String {
    format!(
        "{REPOSITORIES_PATH}/{}/scripts/{}",
        encode_segment(repo_id),
        encode_segment(entry_id)
    )
}

/// Rend les lignes de jetons en `<span>` colorés, séparées par des retours à la ligne.
fn highlighted_lines(lines: Vec<Vec<CodeToken>>) -> impl IntoView {
    lines
        .into_iter()
        .map(|line| {
            view! {
                {line.into_iter().map(|token| view! {
                    <span class=token.kind.css_class()>{token.text}</span>
                }).collect_view()}
                "\n"
            }
        })
        .collect_view()
}

/// Texte d'un pointeur de dépôt à ajouter soi-même pour obtenir un fournisseur absent.
fn repository_hint(status: &RequirementStatus) -> Option<String> {
    if status.available {
        return None;
    }
    status.requirement.repository.as_ref().map(|repository| {
        format!(
            "Dépôt à ajouter vous-même : {} ({})",
            repository.id, repository.index_url
        )
    })
}

/// Dépendances du script sur des variables exposées par des plugins, et leur disponibilité.
#[component]
fn RequirementsList(requirements: Vec<RequirementStatus>, unverified: bool) -> impl IntoView {
    if requirements.is_empty() && !unverified {
        return ().into_any();
    }
    view! {
        <h3 class="form-label">"Dépendances"</h3>
        {unverified.then(|| view! {
            <p class="text-dim text-sm">
                "Dépendances non vérifiées : les variables exposées par les plugins se saisissent à la main."
            </p>
        })}
        <ul class="step-list">
            {requirements.into_iter().map(|status| {
                let hint = repository_hint(&status);
                view! {
                    <li class="step-item">
                        <div class="step-item__head">
                            <Tag label=status.requirement.variable.clone() tone=TagTone::Mono />
                            <Tag label=status.requirement.provider.clone() tone=TagTone::Neutral />
                            <Tag
                                label=if status.available { "Disponible" } else { "Fournisseur absent" }
                                tone=if status.available { TagTone::Success } else { TagTone::Danger }
                            />
                        </div>
                        {hint.map(|text| view! { <div class="step-item__meta text-dim">{text}</div> })}
                    </li>
                }
            }).collect_view()}
        </ul>
    }
    .into_any()
}

#[component]
pub fn ScriptPreview(repo_id: String, entry_id: String, on_back: Callback<()>) -> impl IntoView {
    let path = script_preview_path(&repo_id, &entry_id);
    let script = LocalResource::new(move || {
        let path = path.clone();
        async move { api::get::<ScriptPreviewView>(&path).await }
    });
    view! {
        <div class="pack-components">
            <button type="button" class="btn btn-secondary btn-sm" on:click=move |_| on_back.run(())>
                "← Retour au contenu"
            </button>
            {move || match script.get() {
                None => view! { <p class="text-dim">"Chargement du script…"</p> }.into_any(),
                Some(Err(error)) => view! { <p class="text-danger">{error.message()}</p> }.into_any(),
                Some(Ok(ScriptPreviewView { script, lines, requirements, dependencies_unverified })) => view! {
                    <h3 class="form-label">{script.name}</h3>
                    <div class="cluster">
                        <Tag label=script.interpreter.executable() tone=TagTone::Primary />
                        <Tag label=script.entrypoint tone=TagTone::Mono />
                    </div>
                    {script.description.map(|text| view! { <p class="text-dim text-sm">{text}</p> })}
                    <pre class="code-block-wrapper"><code>{highlighted_lines(lines)}</code></pre>
                    <h3 class="form-label">"Variables"</h3>
                    <VariablesTable variables=script.variables />
                    <RequirementsList requirements=requirements unverified=dependencies_unverified />
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

    #[test]
    fn a_repository_hint_is_shown_only_for_an_unavailable_provider_that_names_one() {
        use forgearr_shared::exposed_variables::{RepositoryPointer, ScriptRequirement};
        let requirement = ScriptRequirement {
            variable: "exiftool.camera.model".into(),
            provider: "app.exiftool".into(),
            contract_major: 1,
            repository: Some(RepositoryPointer {
                id: "repo.tiers".into(),
                index_url: "https://exemple.org/index.json".into(),
            }),
        };
        let missing = RequirementStatus {
            requirement: requirement.clone(),
            available: false,
        };
        assert!(repository_hint(&missing).is_some_and(|text| text.contains("repo.tiers")));
        let available = RequirementStatus {
            requirement: requirement.clone(),
            available: true,
        };
        assert_eq!(repository_hint(&available), None);
        let unnamed = RequirementStatus {
            requirement: ScriptRequirement {
                repository: None,
                ..requirement
            },
            available: false,
        };
        assert_eq!(repository_hint(&unnamed), None);
    }
}
