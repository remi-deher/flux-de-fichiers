//! Aperçu en lecture seule du code d'un script du catalogue, affiché dans le tiroir du dépôt ou de
//! l'entrée du catalogue, avec l'ajout à la librairie locale (« ma collection »).

use super::{CATALOG_SCRIPTS_PATH, REPOSITORIES_PATH};
use crate::api::{self, ApiError};
use crate::live::{load, use_live};
use crate::route::encode_segment;
use crate::ui::{Tag, TagTone, Toasts, VariablesTable};
use forgearr_shared::exposed_variables::RequirementStatus;
use forgearr_shared::highlight::{CodeToken, ScriptPreviewView};
use forgearr_shared::live::ChangeTopic;
use forgearr_shared::local::LocalScriptSummary;
use leptos::prelude::*;

const CONFLICT_STATUS: u16 = 409;

/// Chemin de l'API qui renvoie le code d'un script d'un dépôt.
pub fn script_preview_path(repo_id: &str, entry_id: &str) -> String {
    format!(
        "{REPOSITORIES_PATH}/{}/scripts/{}",
        encode_segment(repo_id),
        encode_segment(entry_id)
    )
}

/// Chemin de l'API qui renvoie le code d'un script du catalogue, quel que soit son dépôt.
pub fn catalog_script_path(entry_id: &str) -> String {
    format!("{CATALOG_SCRIPTS_PATH}/{}", encode_segment(entry_id))
}

/// Chemin de l'API qui copie un script du catalogue dans la librairie locale.
pub fn catalog_script_import_path(entry_id: &str) -> String {
    format!("{}/import", catalog_script_path(entry_id))
}

/// Un script de ce nom est-il déjà dans la librairie ?
fn is_already_imported(error: &ApiError) -> bool {
    matches!(
        error,
        ApiError::Rejected {
            status: CONFLICT_STATUS,
            ..
        }
    )
}

/// Message affiché quand l'ajout d'un script à la librairie échoue.
fn import_error_message(error: &ApiError) -> String {
    if is_already_imported(error) {
        "Ce script est déjà dans votre librairie (un script porte déjà ce nom).".to_owned()
    } else {
        error.message()
    }
}

/// Le script est-il dans la collection ? `None` tant que le central n'a pas répondu.
fn in_collection(script: Option<&Result<ScriptPreviewView, ApiError>>) -> Option<bool> {
    script.map(|read| read.as_ref().is_ok_and(|view| view.in_collection))
}

/// Libellé du bouton d'ajout à la librairie.
fn import_label(imported: bool) -> &'static str {
    if imported {
        "Dans ma collection"
    } else {
        "Ajouter à ma collection"
    }
}

/// Bouton qui copie le script dans la librairie locale, où il devient modifiable.
#[component]
fn ImportButton(import_path: String, in_collection: Signal<Option<bool>>) -> impl IntoView {
    let pending = RwSignal::new(false);
    let live = use_live();
    // Ajout de cette page, en attendant que le central le confirme ; ensuite seul le central fait foi.
    let just_added = RwSignal::new(false);
    Effect::new(move |_| {
        if in_collection.get() == Some(true) {
            just_added.set(false);
        }
    });
    let imported = move || just_added.get() || in_collection.get() == Some(true);
    let import = move |_| {
        let path = import_path.clone();
        pending.set(true);
        leptos::task::spawn_local(async move {
            match api::post::<(), LocalScriptSummary>(&path, None).await {
                Ok(saved) => {
                    let _ = just_added.try_set(true);
                    Toasts::notify_success(format!(
                        "« {} » ajouté à votre librairie (section Scripts).",
                        saved.name
                    ));
                }
                Err(error) => {
                    if is_already_imported(&error) {
                        let _ = just_added.try_set(true);
                    }
                    Toasts::notify_error(import_error_message(&error));
                }
            }
            live.touch(&[ChangeTopic::Catalog]);
            // Le tiroir a pu se fermer pendant l'appel.
            let _ = pending.try_set(false);
        });
    };
    view! {
        <button type="button" class="btn btn-primary btn-sm" disabled=move || pending.get() || in_collection.get().is_none() || imported()
            on:click=import>
            {move || import_label(imported())}
        </button>
    }
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

/// Aperçu du script lu à `path`. `on_back` ajoute un retour à la liste ; `import_path` ajoute le
/// bouton « Ajouter à ma collection » (réservé à qui gère les plugins).
#[component]
pub fn ScriptPreview(
    path: String,
    #[prop(optional_no_strip)] on_back: Option<Callback<()>>,
    #[prop(optional_no_strip)] import_path: Option<String>,
) -> impl IntoView {
    let script = load::<ScriptPreviewView>(&[ChangeTopic::Catalog], path);
    let in_collection = Memo::new(move |_| in_collection(script.read().as_ref()));
    view! {
        <div class="pack-components">
            {on_back.map(|on_back| view! {
                <button type="button" class="btn btn-secondary btn-sm" on:click=move |_| on_back.run(())>
                    "← Retour au contenu"
                </button>
            })}
            {import_path.map(|import_path| view! { <ImportButton import_path=import_path in_collection=in_collection.into() /> })}
            {move || match script.get() {
                None => view! { <p class="text-dim">"Chargement du script…"</p> }.into_any(),
                Some(Err(error)) => view! { <p class="text-danger">{error.message()}</p> }.into_any(),
                Some(Ok(ScriptPreviewView { script, lines, requirements, dependencies_unverified, .. })) => view! {
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
    fn catalog_script_paths_encode_the_entry_id_and_end_with_import_for_the_copy() {
        let preview = catalog_script_path("script.tag mkv");
        assert!(preview.starts_with(CATALOG_SCRIPTS_PATH));
        assert!(!preview.contains(' '));
        assert_eq!(
            catalog_script_import_path("script.x"),
            format!("{CATALOG_SCRIPTS_PATH}/script.x/import")
        );
    }

    #[test]
    fn a_name_conflict_is_explained_and_other_failures_keep_their_own_message() {
        let conflict = ApiError::Rejected {
            status: CONFLICT_STATUS,
            code: "conflict".into(),
            detail: None,
        };
        assert!(import_error_message(&conflict).contains("déjà dans votre librairie"));
        assert!(is_already_imported(&conflict));
        assert!(!is_already_imported(&ApiError::Network));
        assert_eq!(
            import_error_message(&ApiError::Network),
            ApiError::Network.message()
        );
    }

    #[test]
    fn the_collection_state_is_known_from_the_server_answer() {
        let view = |in_collection| ScriptPreviewView {
            script: LocalScriptSummary {
                id: "script.x".into(),
                slug: "x".into(),
                name: "X".into(),
                description: None,
                interpreter: Default::default(),
                entrypoint: "run.sh".into(),
                code: String::new(),
                category: None,
                tags: Vec::new(),
                variables: Vec::new(),
                requires: Vec::new(),
                dependencies_unverified: false,
            },
            lines: Vec::new(),
            requirements: Vec::new(),
            dependencies_unverified: false,
            in_collection,
        };
        assert_eq!(in_collection(None), None);
        assert_eq!(in_collection(Some(&Ok(view(true)))), Some(true));
        assert_eq!(in_collection(Some(&Ok(view(false)))), Some(false));
        assert_eq!(in_collection(Some(&Err(ApiError::Network))), Some(false));
    }

    #[test]
    fn the_import_button_tells_when_the_script_is_in_the_collection() {
        assert_ne!(import_label(true), import_label(false));
    }

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
