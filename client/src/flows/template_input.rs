//! Saisie de texte acceptant les modèles `{$variable}` et aide sur le protocole des scripts.

use super::fields::{current, set_field};
use super::state::Editor;
use forgearr_shared::flows::{
    CUSTOM_PREFIX, FlowGraph, IntrinsicVariable, ProtocolField, SCRIPT_INPUT_FIELDS,
    SCRIPT_OUTPUT_FIELDS, SCRIPT_SCHEMA_VERSION, SET_VARIABLE_BLOCK, is_known_expression,
    template_tokens,
};
use leptos::prelude::*;
use serde_json::Value;

/// Aide de syntaxe affichée sous chaque champ texte.
const TEMPLATE_HELP: &str =
    "Modèles : {$file.stem}, {$vars.nom|défaut} ; {{$ pour un « {$ » littéral.";

/// Variable proposée à l'insertion : expression et libellé.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VariableSuggestion {
    pub expr: String,
    pub label: String,
}

/// Variables intrinsèques, puis `vars.<nom>` des blocs « Définir une variable » du graphe.
pub fn variable_suggestions(graph: &FlowGraph) -> Vec<VariableSuggestion> {
    let mut suggestions: Vec<VariableSuggestion> = IntrinsicVariable::ALL
        .into_iter()
        .map(|var| VariableSuggestion {
            expr: var.key().to_owned(),
            label: var.label().to_owned(),
        })
        .collect();
    for node in &graph.nodes {
        let name = node
            .config
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let expr = format!("{CUSTOM_PREFIX}{}", name.trim());
        if node.kind.as_str().ends_with(SET_VARIABLE_BLOCK)
            && is_known_expression(&expr)
            && !suggestions.iter().any(|known| known.expr == expr)
        {
            suggestions.push(VariableSuggestion {
                expr,
                label: format!("Variable définie par « {} »", node.name),
            });
        }
    }
    suggestions
}

/// Expressions d'un modèle qu'aucun exécuteur ne sait résoudre.
pub fn unknown_tokens(text: &str) -> Vec<String> {
    template_tokens(text)
        .into_iter()
        .filter(|expr| !is_known_expression(expr))
        .collect()
}

/// Champ texte d'un bloc : saisie libre, insertion de variables et signalement des inconnues.
#[component]
pub fn TemplateInput(
    editor: Editor,
    node_id: StoredValue<String>,
    field_name: StoredValue<String>,
    input_id: String,
    label: String,
) -> impl IntoView {
    let value = move || {
        current(editor, &node_id.read_value(), &field_name.get_value())
            .as_str()
            .unwrap_or_default()
            .to_owned()
    };
    let write = move |text: String| {
        set_field(
            editor,
            &node_id.read_value(),
            &field_name.get_value(),
            Value::String(text),
        );
    };
    let suggestions = Memo::new(move |_| editor.graph.with(variable_suggestions));
    let unknown = move || unknown_tokens(&value());
    view! {
        <label for=input_id.clone() class="form-label">{label}</label>
        <input id=input_id class="form-input"
            prop:value=value
            on:input=move |event| write(event_target_value(&event)) />
        <p class="form-help">{TEMPLATE_HELP}</p>
        {move || {
            let unknown = unknown();
            (!unknown.is_empty()).then(|| view! {
                <p class="form-help text-warning">
                    {format!("Variables inconnues : {}", unknown.join(", "))}
                </p>
            })
        }}
        <details>
            <summary class="form-help">"Insérer une variable"</summary>
            <div class="cluster">
                {move || suggestions.get().into_iter().map(|suggestion| {
                    let token = format!("{{${}}}", suggestion.expr);
                    view! {
                        <button type="button" class="btn btn-secondary btn-sm" title=suggestion.label
                            on:click=move |_| write(format!("{}{token}", value()))>
                            {suggestion.expr.clone()}
                        </button>
                    }
                }).collect_view()}
            </div>
        </details>
    }
}

fn protocol_rows(fields: &'static [ProtocolField]) -> impl IntoView {
    fields
        .iter()
        .map(|(name, description)| {
            view! { <li><code>{*name}</code>" : "{*description}</li> }
        })
        .collect_view()
}

/// Rappel du protocole stdin/stdout des scripts, généré depuis les constantes partagées.
#[component]
pub fn ScriptProtocolHelp() -> impl IntoView {
    view! {
        <details>
            <summary class="form-help">{format!("Protocole (version {SCRIPT_SCHEMA_VERSION})")}</summary>
            <p class="form-help">"Entrée JSON sur stdin :"</p>
            <ul class="form-help">{protocol_rows(&SCRIPT_INPUT_FIELDS)}</ul>
            <p class="form-help">"Réponse JSON facultative sur stdout :"</p>
            <ul class="form-help">{protocol_rows(&SCRIPT_OUTPUT_FIELDS)}</ul>
        </details>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn graph_with_set_variable(name: &str) -> Result<FlowGraph, serde_json::Error> {
        serde_json::from_value(json!({
            "nodes": [
                {"id": "a", "type": "essentiel.set-variable", "name": "Codec", "config": {"name": name}},
                {"id": "b", "type": "file-output", "name": "Sortie", "config": {"name": "ignoré"}}
            ],
            "connections": []
        }))
    }

    #[test]
    fn suggests_intrinsic_and_defined_variables() -> Result<(), serde_json::Error> {
        let suggestions = variable_suggestions(&graph_with_set_variable("codec")?);
        let exprs: Vec<&str> = suggestions.iter().map(|s| s.expr.as_str()).collect();
        assert!(exprs.contains(&"file.stem"));
        assert!(exprs.contains(&"vars.codec"));
        assert!(!exprs.contains(&"vars.ignoré"));
        assert_eq!(exprs.len(), IntrinsicVariable::ALL.len() + 1);
        let invalid = variable_suggestions(&graph_with_set_variable("nom invalide")?);
        assert_eq!(invalid.len(), IntrinsicVariable::ALL.len());
        Ok(())
    }

    #[test]
    fn flags_unknown_tokens_only() {
        assert!(unknown_tokens("{$file.stem}-{$vars.x}{$setting.url}{{$brut}").is_empty());
        assert_eq!(
            unknown_tokens("{$fichier.nom}"),
            vec!["fichier.nom".to_owned()]
        );
    }
}
