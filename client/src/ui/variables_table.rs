//! Liste en lecture seule des variables déclarées par un script (ADR 0025).

use super::{Tag, TagTone};
use forgearr_shared::script_variables::{ScriptVariable, VariableKind, default_to_text};
use leptos::prelude::*;

/// Valeur par défaut, options et bornes d'une variable, en une ligne lisible.
fn variable_details(variable: &ScriptVariable) -> Vec<String> {
    let mut details = Vec::new();
    if variable.kind != VariableKind::Secret && variable.default_value.is_some() {
        details.push(format!(
            "Défaut : {}",
            default_to_text(variable.default_value.as_ref())
        ));
    }
    if !variable.options.is_empty() {
        details.push(format!("Options : {}", variable.options.join(", ")));
    }
    if let (Some(min), Some(max)) = (&variable.min, &variable.max) {
        details.push(format!("Bornes : {min} à {max}"));
    } else if let Some(min) = &variable.min {
        details.push(format!("Minimum : {min}"));
    } else if let Some(max) = &variable.max {
        details.push(format!("Maximum : {max}"));
    }
    details
}

#[component]
pub fn VariablesTable(variables: Vec<ScriptVariable>) -> impl IntoView {
    if variables.is_empty() {
        return view! { <p class="text-dim text-sm">"Ce script ne déclare aucune variable."</p> }
            .into_any();
    }
    view! {
        <ul class="step-list">
            {variables.into_iter().map(|variable| {
                let details = variable_details(&variable);
                view! {
                    <li class="step-item">
                        <div class="step-item__head">
                            <span class="text-strong">{variable.display_label().to_owned()}</span>
                            <Tag label=variable.name.clone() tone=TagTone::Mono />
                            <Tag label=variable.kind.label() tone=TagTone::Primary />
                            {variable.required.then(|| view! { <Tag label="Requise" tone=TagTone::Success /> })}
                        </div>
                        {variable.description.clone().map(|text| view! {
                            <div class="step-item__meta text-dim">{text}</div>
                        })}
                        {(!details.is_empty()).then(|| view! {
                            <div class="step-item__meta">{details.join(" · ")}</div>
                        })}
                    </li>
                }
            }).collect_view()}
        </ul>
    }
    .into_any()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn variable(kind: VariableKind) -> ScriptVariable {
        ScriptVariable {
            name: "crf".into(),
            label: String::new(),
            description: None,
            kind,
            default_value: None,
            required: false,
            options: Vec::new(),
            min: None,
            max: None,
        }
    }

    #[test]
    fn details_list_default_options_and_bounds() {
        let mut number = variable(VariableKind::Number);
        number.default_value = Some(json!(23));
        number.min = Some(0.into());
        number.max = Some(51.into());
        assert_eq!(variable_details(&number), vec!["Défaut : 23", "Bornes : 0 à 51"]);

        let mut choice = variable(VariableKind::Choice);
        choice.options = vec!["fr".into(), "en".into()];
        assert_eq!(variable_details(&choice), vec!["Options : fr, en"]);

        assert!(variable_details(&variable(VariableKind::Text)).is_empty());
    }

    #[test]
    fn a_secret_never_shows_a_default() {
        let mut secret = variable(VariableKind::Secret);
        secret.default_value = Some(json!("fuite"));
        assert!(variable_details(&secret).is_empty());
    }
}
