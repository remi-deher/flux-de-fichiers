//! Choix du modèle de l'assistant IA : liste des modèles proposés par le fournisseur.

use forgearr_shared::ai::{AiModelOption, AiProvider};
use leptos::prelude::*;

const AUTOMATIC_LABEL: &str = "Automatique (premier modèle proposé au compte)";

/// Entrée de la liste déroulante.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelChoice {
    pub value: String,
    pub label: String,
}

/// Choix affichés : « automatique » si le fournisseur le permet, le modèle enregistré s'il n'est
/// plus proposé (pour le voir et le remplacer), puis les modèles du fournisseur.
pub fn model_choices(
    catalog: &[AiModelOption],
    current: &str,
    automatic: bool,
) -> Vec<ModelChoice> {
    let current = current.trim();
    let mut choices = Vec::new();
    if automatic {
        choices.push(ModelChoice {
            value: String::new(),
            label: AUTOMATIC_LABEL.to_owned(),
        });
    }
    if !current.is_empty() && !catalog.iter().any(|model| model.id == current) {
        choices.push(ModelChoice {
            value: current.to_owned(),
            label: format!("{current} (non proposé par le fournisseur)"),
        });
    }
    choices.extend(catalog.iter().map(|model| ModelChoice {
        value: model.id.clone(),
        label: model.label.clone(),
    }));
    choices
}

/// Aide affichée sous la saisie libre : la raison fournie par le fournisseur, si on la connaît.
fn unavailable_help(issue: Option<&str>) -> String {
    match issue {
        Some(reason) => format!("Liste indisponible, saisie manuelle : {reason}"),
        None => "Liste indisponible : saisie manuelle. Connectez-vous ou testez la connexion pour obtenir la liste.".to_owned(),
    }
}

/// Liste déroulante des modèles proposés ; saisie libre seulement tant qu'aucune liste n'est connue
/// (fournisseur injoignable, compte non connecté, serveur sans catalogue).
#[component]
pub fn AiModelField(
    model: RwSignal<String>,
    catalog: RwSignal<Vec<AiModelOption>>,
    /// Pourquoi la liste est vide, quand le fournisseur a répondu par une erreur.
    issue: RwSignal<Option<String>>,
    provider: RwSignal<AiProvider>,
    loading: Signal<bool>,
) -> impl IntoView {
    view! {
        <div class="form-group">
            <label for="ai-model" class="form-label">"Modèle"</label>
            {move || {
                let models = catalog.get();
                if models.is_empty() {
                    let current = provider.get();
                    let placeholder = if current.has_automatic_model() {
                        AUTOMATIC_LABEL
                    } else {
                        current.default_model()
                    };
                    let help = if loading.get() {
                        "Chargement des modèles proposés…".to_owned()
                    } else {
                        unavailable_help(issue.get().as_deref())
                    };
                    view! {
                        <input
                            id="ai-model"
                            class="form-input text-mono"
                            placeholder=placeholder
                            prop:value=move || model.get()
                            on:input=move |ev| model.set(event_target_value(&ev))
                        />
                        <p class="form-help">{help}</p>
                    }.into_any()
                } else {
                    let choices = model_choices(
                        &models,
                        &model.get_untracked(),
                        provider.get_untracked().has_automatic_model(),
                    );
                    view! {
                        <select
                            id="ai-model"
                            class="form-input text-mono"
                            on:change=move |ev| model.set(event_target_value(&ev))
                        >
                            {choices.into_iter().map(|choice| {
                                let value = choice.value.clone();
                                view! {
                                    <option value=choice.value selected=move || model.get() == value>
                                        {choice.label}
                                    </option>
                                }
                            }).collect_view()}
                        </select>
                    }.into_any()
                }
            }}
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog() -> Vec<AiModelOption> {
        vec![
            AiModelOption::new("a", None),
            AiModelOption::new("b", Some("B")),
        ]
    }

    #[test]
    fn unavailable_help_reports_the_provider_reason() {
        assert!(unavailable_help(None).contains("testez la connexion"));
        assert!(unavailable_help(Some("ChatGPT HTTP 400")).ends_with("ChatGPT HTTP 400"));
    }

    #[test]
    fn automatic_providers_get_an_automatic_choice_first() {
        let choices = model_choices(&catalog(), "", true);
        assert_eq!(choices[0].value, "");
        assert_eq!(choices.len(), 3);
        assert_eq!(choices[2].label, "B · b");
        assert_eq!(model_choices(&catalog(), "", false).len(), 2);
    }

    #[test]
    fn a_saved_model_the_provider_no_longer_offers_stays_visible() {
        let choices = model_choices(&catalog(), "gpt-5-codex", true);
        assert_eq!(choices[1].value, "gpt-5-codex");
        assert!(choices[1].label.contains("non proposé"));
        // Un modèle toujours proposé n'est pas dupliqué.
        assert_eq!(model_choices(&catalog(), "a", false).len(), 2);
    }
}
