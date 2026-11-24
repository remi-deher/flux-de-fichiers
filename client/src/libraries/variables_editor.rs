//! Déclaration des variables d'un script local : lignes éditables converties vers le schéma (ADR 0025).

use super::rows::{KeyedRow, edit_row, next_key, read_row, remove_row, row_keys};
use forgearr_shared::script_variables::{
    InvalidDefaultText, ScriptVariable, VariableKind, default_from_text, default_to_text,
    validate_variables,
};
use leptos::prelude::*;
use serde_json::{Number, Value};

/// Ligne en cours d'édition : le schéma plus les textes saisis tels quels, convertis à l'enregistrement.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct VariableRow {
    key: u32,
    variable: ScriptVariable,
    default_text: String,
    options_text: String,
    min_text: String,
    max_text: String,
}

impl KeyedRow for VariableRow {
    fn key(&self) -> u32 {
        self.key
    }
}

fn number_text(number: Option<&Number>) -> String {
    number.map(Number::to_string).unwrap_or_default()
}

fn bound_from_text(raw: &str) -> Result<Option<Number>, InvalidDefaultText> {
    match default_from_text(VariableKind::Number, raw)? {
        Some(Value::Number(number)) => Ok(Some(number)),
        _ => Ok(None),
    }
}

/// Noms des variables déclarées dans les lignes (les noms vides sont ignorés).
pub fn variable_names(rows: &[VariableRow]) -> Vec<String> {
    rows.iter()
        .map(|row| row.variable.name.clone())
        .filter(|name| !name.is_empty())
        .collect()
}

/// Lignes éditables d'après les variables enregistrées.
pub fn rows_from(variables: Vec<ScriptVariable>) -> Vec<VariableRow> {
    (0u32..)
        .zip(variables)
        .map(|(key, variable)| VariableRow {
            key,
            default_text: default_to_text(variable.default_value.as_ref()),
            options_text: variable.options.join(", "),
            min_text: number_text(variable.min.as_ref()),
            max_text: number_text(variable.max.as_ref()),
            variable,
        })
        .collect()
}

fn row_to_variable(row: &VariableRow) -> Result<ScriptVariable, String> {
    let mut variable = row.variable.clone();
    variable.name = variable.name.trim().to_owned();
    variable.label = variable.label.trim().to_owned();
    variable.description = variable
        .description
        .map(|text| text.trim().to_owned())
        .filter(|text| !text.is_empty());
    let invalid = |what: &str| format!("« {} » : {what} incompatible avec le type.", variable.name);
    variable.default_value = default_from_text(variable.kind, &row.default_text)
        .map_err(|_| invalid("valeur par défaut"))?;
    variable.options = if variable.kind == VariableKind::Choice {
        row.options_text
            .split(',')
            .map(str::trim)
            .filter(|option| !option.is_empty())
            .map(str::to_owned)
            .collect()
    } else {
        Vec::new()
    };
    let is_number = variable.kind == VariableKind::Number;
    variable.min = if is_number {
        bound_from_text(&row.min_text).map_err(|_| invalid("minimum"))?
    } else {
        None
    };
    variable.max = if is_number {
        bound_from_text(&row.max_text).map_err(|_| invalid("maximum"))?
    } else {
        None
    };
    Ok(variable)
}

/// Schéma validé d'après les lignes saisies, ou le message à afficher à l'utilisateur.
pub fn rows_to_variables(rows: &[VariableRow]) -> Result<Vec<ScriptVariable>, String> {
    let variables = rows
        .iter()
        .map(row_to_variable)
        .collect::<Result<Vec<_>, _>>()?;
    validate_variables(&variables).map_err(|err| err.to_string())?;
    Ok(variables)
}

#[component]
fn VariableRowEditor(rows: RwSignal<Vec<VariableRow>>, key: u32, read_only: bool) -> impl IntoView {
    let kind = move || read_row(rows, key, |row| row.variable.kind).unwrap_or_default();
    let text = move |read: fn(&VariableRow) -> String| {
        move || read_row(rows, key, read).unwrap_or_default()
    };
    let name_id = format!("var-name-{key}");
    view! {
        <div class="step-item">
            <div class="form-group">
                <label class="form-label" for=name_id.clone()>"Nom (minuscules, chiffres, _)"</label>
                <input id=name_id class="form-input text-mono" disabled=read_only placeholder="crf"
                    prop:value=text(|row| row.variable.name.clone())
                    on:input=move |e| edit_row(rows, key, |row| row.variable.name = event_target_value(&e)) />
            </div>
            <div class="form-group">
                <label class="form-label">"Libellé"</label>
                <input class="form-input" disabled=read_only placeholder="Qualité vidéo"
                    prop:value=text(|row| row.variable.label.clone())
                    on:input=move |e| edit_row(rows, key, |row| row.variable.label = event_target_value(&e)) />
            </div>
            <div class="form-group">
                <label class="form-label">"Type"</label>
                <select class="form-input" disabled=read_only
                    prop:value=move || kind().as_str()
                    on:change=move |e| edit_row(rows, key, |row| {
                        row.variable.kind = VariableKind::from_tag(&event_target_value(&e))
                            .unwrap_or(row.variable.kind);
                        if row.variable.kind == VariableKind::Secret {
                            row.default_text.clear();
                        }
                    })>
                    {VariableKind::ALL.iter().map(|&variant| view! {
                        <option value=variant.as_str()>{variant.label()}</option>
                    }).collect_view()}
                </select>
            </div>
            {move || (kind() != VariableKind::Secret).then(|| view! {
                <div class="form-group">
                    <label class="form-label">"Valeur par défaut"</label>
                    <input class="form-input" disabled=read_only
                        placeholder=move || if kind() == VariableKind::Boolean { "true ou false" } else { "" }
                        prop:value=text(|row| row.default_text.clone())
                        on:input=move |e| edit_row(rows, key, |row| row.default_text = event_target_value(&e)) />
                </div>
            })}
            {move || (kind() == VariableKind::Choice).then(|| view! {
                <div class="form-group">
                    <label class="form-label">"Options (séparées par des virgules)"</label>
                    <input class="form-input" disabled=read_only placeholder="fr, en"
                        prop:value=text(|row| row.options_text.clone())
                        on:input=move |e| edit_row(rows, key, |row| row.options_text = event_target_value(&e)) />
                </div>
            })}
            {move || (kind() == VariableKind::Number).then(|| view! {
                <div class="cluster">
                    <input class="form-input" disabled=read_only placeholder="Minimum" aria-label="Minimum"
                        prop:value=text(|row| row.min_text.clone())
                        on:input=move |e| edit_row(rows, key, |row| row.min_text = event_target_value(&e)) />
                    <input class="form-input" disabled=read_only placeholder="Maximum" aria-label="Maximum"
                        prop:value=text(|row| row.max_text.clone())
                        on:input=move |e| edit_row(rows, key, |row| row.max_text = event_target_value(&e)) />
                </div>
            })}
            <div class="form-group">
                <label class="form-label">"Description"</label>
                <input class="form-input" disabled=read_only
                    prop:value=text(|row| row.variable.description.clone().unwrap_or_default())
                    on:input=move |e| edit_row(rows, key, |row| row.variable.description = Some(event_target_value(&e))) />
            </div>
            <label class="form-label">
                <input type="checkbox" disabled=read_only
                    prop:checked=move || read_row(rows, key, |row| row.variable.required).unwrap_or(false)
                    on:change=move |e| edit_row(rows, key, |row| row.variable.required = event_target_checked(&e)) />
                " Requise"
            </label>
            {(!read_only).then(|| view! {
                <button type="button" class="btn btn-danger-quiet btn-sm"
                    on:click=move |_| remove_row(rows, key)>
                    "Retirer la variable"
                </button>
            })}
        </div>
    }
}

/// Liste éditable des variables d'un script, avec ajout et retrait de lignes.
#[component]
pub fn VariablesEditor(rows: RwSignal<Vec<VariableRow>>, read_only: bool) -> impl IntoView {
    let add = move |_| {
        rows.update(|all| {
            let key = next_key(all);
            all.push(VariableRow {
                key,
                ..VariableRow::default()
            });
        });
    };
    view! {
        <div class="form-group">
            <span class="form-label">"Variables réglables dans le flux"</span>
            <p class="form-help">"Le script les lit dans « config » sur stdin."</p>
            <For
                each=move || row_keys(rows)
                key=|key| *key
                children=move |key| view! { <VariableRowEditor rows=rows key=key read_only=read_only /> }
            />
            {(!read_only).then(|| view! {
                <button type="button" class="btn btn-secondary btn-sm" on:click=add>"Ajouter une variable"</button>
            })}
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn row(name: &str, kind: VariableKind) -> VariableRow {
        let variable = ScriptVariable {
            name: name.into(),
            kind,
            ..ScriptVariable::default()
        };
        VariableRow {
            key: 0,
            variable,
            ..VariableRow::default()
        }
    }

    #[test]
    fn rows_round_trip_the_declared_variables() {
        let crf = ScriptVariable {
            name: "crf".into(),
            kind: VariableKind::Number,
            default_value: Some(json!(23)),
            min: Some(0.into()),
            max: Some(51.into()),
            ..ScriptVariable::default()
        };
        let langue = ScriptVariable {
            name: "langue".into(),
            kind: VariableKind::Choice,
            options: vec!["fr".into(), "en".into()],
            default_value: Some(json!("fr")),
            ..ScriptVariable::default()
        };
        let variables = vec![crf, langue];
        assert_eq!(
            rows_to_variables(&rows_from(variables.clone())),
            Ok(variables)
        );
    }

    #[test]
    fn typed_texts_are_converted_and_bad_ones_name_the_variable() {
        let mut number = row("crf", VariableKind::Number);
        number.default_text = "23".into();
        number.min_text = "0".into();
        assert_eq!(
            rows_to_variables(std::slice::from_ref(&number)).map(|v| v[0].default_value.clone()),
            Ok(Some(json!(23)))
        );
        number.default_text = "abc".into();
        let error = rows_to_variables(&[number]).err().unwrap_or_default();
        assert!(error.contains("crf") && error.contains("valeur par défaut"));
    }

    #[test]
    fn options_and_bounds_are_dropped_when_the_type_does_not_use_them() {
        let mut text = row("nom", VariableKind::Text);
        text.options_text = "a, b".into();
        text.min_text = "1".into();
        let converted = rows_to_variables(&[text]);
        assert!(converted.is_ok_and(|v| v[0].options.is_empty() && v[0].min.is_none()));
    }

    #[test]
    fn schema_errors_are_reported_to_the_user() {
        let rows = [
            row("crf", VariableKind::Text),
            row("crf", VariableKind::Text),
        ];
        assert!(
            rows_to_variables(&rows)
                .err()
                .is_some_and(|msg| msg.contains("deux fois"))
        );
        assert!(rows_to_variables(&[row("Mauvais Nom", VariableKind::Text)]).is_err());
    }

    #[test]
    fn a_secret_keeps_no_default_even_if_text_was_typed_before() {
        let mut secret = row("jeton", VariableKind::Secret);
        secret.default_text = "ancien".into();
        let converted = rows_to_variables(&[secret]);
        assert!(converted.is_ok_and(|v| v[0].default_value.is_none()));
    }
}
