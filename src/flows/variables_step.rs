//! Bloc primitif « Définir une variable » : écrit `vars.<nom>` dans le contexte de la tâche.

use forgearr_shared::flows::{FlowVariables, scalar_to_string};
use serde::{Deserialize, Serialize};
use serde_json::{Number, Value};

/// Type de la valeur écrite.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum VariableValueType {
    #[default]
    Text,
    Number,
    Bool,
}

/// Configuration du bloc, déjà substituée (`value` peut venir d'un modèle `{$…}`).
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetVariableConfig {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub value: Value,
    #[serde(default)]
    pub value_type: VariableValueType,
}

fn convert(value: &Value, value_type: VariableValueType) -> Result<Value, String> {
    let text = scalar_to_string(Some(value));
    let text = text.trim();
    match value_type {
        VariableValueType::Text => Ok(Value::String(scalar_to_string(Some(value)))),
        VariableValueType::Number => text
            .parse::<i64>()
            .map(Value::from)
            .ok()
            .or_else(|| {
                text.parse::<f64>()
                    .ok()
                    .and_then(Number::from_f64)
                    .map(Value::Number)
            })
            .ok_or_else(|| format!("« {text} » n'est pas un nombre")),
        VariableValueType::Bool => match text.to_ascii_lowercase().as_str() {
            "true" | "1" | "oui" | "yes" => Ok(Value::Bool(true)),
            "false" | "0" | "non" | "no" | "" => Ok(Value::Bool(false)),
            _ => Err(format!("« {text} » n'est pas un booléen")),
        },
    }
}

/// Applique le bloc ; renvoie le message de l'étape (le nom seul, jamais la valeur).
pub fn apply_set_variable(config: &Value, vars: &mut FlowVariables) -> Result<String, String> {
    let config: SetVariableConfig = serde_json::from_value(config.clone())
        .map_err(|_| "configuration invalide du bloc « Définir une variable »".to_owned())?;
    let name = config.name.trim();
    let value = convert(&config.value, config.value_type)?;
    vars.set_custom(name, value)
        .map_err(|err| err.to_string())?;
    Ok(format!("variable vars.{name} définie"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn apply(config: Value) -> (Result<String, String>, FlowVariables) {
        let mut vars = FlowVariables::default();
        let result = apply_set_variable(&config, &mut vars);
        (result, vars)
    }

    #[test]
    fn converts_text_number_and_bool() {
        let (result, vars) = apply(json!({"name": "codec", "value": "hevc"}));
        assert_eq!(result.as_deref(), Ok("variable vars.codec définie"));
        assert_eq!(vars.lookup("vars.codec"), Some(&json!("hevc")));

        let (_, vars) = apply(json!({"name": "crf", "value": " 22 ", "valueType": "number"}));
        assert_eq!(vars.lookup("vars.crf"), Some(&json!(22)));
        let (_, vars) = apply(json!({"name": "ratio", "value": "1.5", "valueType": "number"}));
        assert_eq!(vars.lookup("vars.ratio"), Some(&json!(1.5)));
        let (_, vars) = apply(json!({"name": "hdr", "value": "Oui", "valueType": "bool"}));
        assert_eq!(vars.lookup("vars.hdr"), Some(&json!(true)));
        let (_, vars) = apply(json!({"name": "taille", "value": 42}));
        assert_eq!(vars.lookup("vars.taille"), Some(&json!("42")));
    }

    fn rejected(config: Value) -> bool {
        apply(config).0.is_err()
    }

    #[test]
    fn rejects_invalid_names_and_conversions() {
        assert!(rejected(json!({"name": "a b", "value": "x"})));
        assert!(rejected(json!({"name": "", "value": "x"})));
        assert!(rejected(json!({"name": "file.name", "value": "x"})));
        assert!(rejected(
            json!({"name": "n", "value": "abc", "valueType": "number"})
        ));
        assert!(rejected(
            json!({"name": "b", "value": "peut-être", "valueType": "bool"})
        ));
    }
}
