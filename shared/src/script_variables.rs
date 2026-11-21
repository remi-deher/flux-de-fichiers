//! Schéma des variables d'un script (`runtime.variables`, ADR 0025) et sa validation.

use serde::{Deserialize, Serialize};
use serde_json::{Number, Value};
use std::fmt;

/// Nombre maximal de variables déclarées par un script.
pub const MAX_VARIABLES: usize = 64;
/// Longueur maximale du nom d'une variable.
pub const MAX_VARIABLE_NAME_LEN: usize = 64;
/// Nombre maximal d'options d'une variable de type choix.
pub const MAX_CHOICE_OPTIONS: usize = 64;

/// Clé de `config` d'un nœud qui porte les valeurs des variables secrètes (chiffrées au repos).
pub const SECRETS_CONFIG_KEY: &str = "secrets";
/// Autre objet de `config` chiffré au repos (en-têtes d'un webhook) : son nom n'est pas une variable.
pub const HEADERS_CONFIG_KEY: &str = "custom_headers";
/// Noms de `config` réservés : une variable ne peut pas les porter.
pub const RESERVED_VARIABLE_NAMES: [&str; 2] = [SECRETS_CONFIG_KEY, HEADERS_CONFIG_KEY];
/// Valeur affichée à la place d'un secret déjà enregistré (jamais sa valeur).
pub const SECRET_PLACEHOLDER: &str = "••••••••";

/// Forme d'une variable : décide du champ de saisie et du type de la valeur dans `config`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VariableKind {
    #[default]
    Text,
    Multiline,
    Number,
    Boolean,
    Choice,
    Secret,
}

impl VariableKind {
    pub const ALL: &'static [Self] = &[
        Self::Text,
        Self::Multiline,
        Self::Number,
        Self::Boolean,
        Self::Choice,
        Self::Secret,
    ];

    /// Identifiant du type tel qu'écrit dans le manifeste.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Multiline => "multiline",
            Self::Number => "number",
            Self::Boolean => "boolean",
            Self::Choice => "choice",
            Self::Secret => "secret",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Text => "Texte",
            Self::Multiline => "Texte long",
            Self::Number => "Nombre",
            Self::Boolean => "Oui / non",
            Self::Choice => "Choix",
            Self::Secret => "Secret",
        }
    }

    pub fn from_tag(tag: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|kind| kind.as_str() == tag)
    }
}

/// Variable d'un script, déclarée dans son manifeste.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScriptVariable {
    pub name: String,
    /// Libellé affiché ; le nom sert de libellé s'il est vide.
    #[serde(default)]
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(rename = "type", default)]
    pub kind: VariableKind,
    #[serde(default, rename = "default", skip_serializing_if = "Option::is_none")]
    pub default_value: Option<Value>,
    #[serde(default)]
    pub required: bool,
    /// Options d'une variable de type choix (vide pour les autres types).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<Number>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<Number>,
}

impl ScriptVariable {
    /// Libellé à afficher : le libellé déclaré, sinon le nom.
    pub fn display_label(&self) -> &str {
        if self.label.trim().is_empty() {
            &self.name
        } else {
            &self.label
        }
    }
}

/// Le texte saisi ne correspond pas au type de la variable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidDefaultText;

/// Raison pour laquelle un schéma de variables est refusé.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VariableSchemaError {
    TooMany,
    InvalidName(String),
    ReservedName(String),
    DuplicateName(String),
    InvalidDefault(String),
    SecretWithDefault(String),
    InvalidOptions(String),
    InvalidRange(String),
}

impl fmt::Display for VariableSchemaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooMany => write!(f, "au plus {MAX_VARIABLES} variables par script"),
            Self::InvalidName(name) => write!(
                f,
                "nom de variable invalide « {name} » (minuscules, chiffres et _, commence par une lettre, {MAX_VARIABLE_NAME_LEN} caractères au plus)"
            ),
            Self::ReservedName(name) => {
                write!(
                    f,
                    "le nom « {name} » est réservé à la configuration du nœud"
                )
            }
            Self::DuplicateName(name) => write!(f, "variable « {name} » déclarée deux fois"),
            Self::InvalidDefault(name) => {
                write!(
                    f,
                    "valeur par défaut de « {name} » incompatible avec son type"
                )
            }
            Self::SecretWithDefault(name) => {
                write!(
                    f,
                    "le secret « {name} » ne peut pas avoir de valeur par défaut"
                )
            }
            Self::InvalidOptions(name) => write!(
                f,
                "options de « {name} » invalides (un choix exige de 1 à {MAX_CHOICE_OPTIONS} options distinctes et non vides, les autres types aucune)"
            ),
            Self::InvalidRange(name) => {
                write!(
                    f,
                    "bornes de « {name} » invalides (min ≤ max, nombres seulement)"
                )
            }
        }
    }
}

impl std::error::Error for VariableSchemaError {}

fn is_valid_name(name: &str) -> bool {
    let mut chars = name.chars();
    name.len() <= MAX_VARIABLE_NAME_LEN
        && chars.next().is_some_and(|first| first.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

fn options_are_valid(variable: &ScriptVariable) -> bool {
    if variable.kind != VariableKind::Choice {
        return variable.options.is_empty();
    }
    let distinct = variable.options.iter().enumerate().all(|(index, option)| {
        !option.trim().is_empty() && !variable.options[..index].contains(option)
    });
    !variable.options.is_empty() && variable.options.len() <= MAX_CHOICE_OPTIONS && distinct
}

fn range_is_valid(variable: &ScriptVariable) -> bool {
    match (&variable.min, &variable.max) {
        (None, None) => true,
        (min, max) if variable.kind == VariableKind::Number => {
            match (
                min.as_ref().and_then(Number::as_f64),
                max.as_ref().and_then(Number::as_f64),
            ) {
                (Some(low), Some(high)) => low <= high,
                _ => true,
            }
        }
        _ => false,
    }
}

fn default_is_valid(variable: &ScriptVariable, value: &Value) -> bool {
    match (variable.kind, value) {
        (VariableKind::Text | VariableKind::Multiline, Value::String(_))
        | (VariableKind::Boolean, Value::Bool(_)) => true,
        (VariableKind::Choice, Value::String(choice)) => variable.options.contains(choice),
        (VariableKind::Number, Value::Number(number)) => number.as_f64().is_some_and(|n| {
            variable
                .min
                .as_ref()
                .and_then(Number::as_f64)
                .is_none_or(|low| n >= low)
                && variable
                    .max
                    .as_ref()
                    .and_then(Number::as_f64)
                    .is_none_or(|high| n <= high)
        }),
        _ => false,
    }
}

/// Vérifie le schéma d'un script : noms uniques et sûrs, types, options, bornes et défauts cohérents.
pub fn validate_variables(variables: &[ScriptVariable]) -> Result<(), VariableSchemaError> {
    if variables.len() > MAX_VARIABLES {
        return Err(VariableSchemaError::TooMany);
    }
    for (index, variable) in variables.iter().enumerate() {
        let name = variable.name.clone();
        if !is_valid_name(&variable.name) {
            return Err(VariableSchemaError::InvalidName(name));
        }
        if RESERVED_VARIABLE_NAMES.contains(&variable.name.as_str()) {
            return Err(VariableSchemaError::ReservedName(name));
        }
        if variables[..index]
            .iter()
            .any(|other| other.name == variable.name)
        {
            return Err(VariableSchemaError::DuplicateName(name));
        }
        if !options_are_valid(variable) {
            return Err(VariableSchemaError::InvalidOptions(name));
        }
        if !range_is_valid(variable) {
            return Err(VariableSchemaError::InvalidRange(name));
        }
        match (&variable.default_value, variable.kind) {
            (Some(_), VariableKind::Secret) => {
                return Err(VariableSchemaError::SecretWithDefault(name));
            }
            (Some(value), _) if !default_is_valid(variable, value) => {
                return Err(VariableSchemaError::InvalidDefault(name));
            }
            _ => {}
        }
    }
    Ok(())
}

/// Texte d'une valeur par défaut pour un champ de saisie (vide si absente).
pub fn default_to_text(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(text)) => text.clone(),
        Some(other) => other.to_string(),
        None => String::new(),
    }
}

/// Valeur par défaut tapée dans un champ de saisie : `None` si le champ est vide,
/// `Err` si le texte ne correspond pas au type.
pub fn default_from_text(
    kind: VariableKind,
    raw: &str,
) -> Result<Option<Value>, InvalidDefaultText> {
    let trimmed = raw.trim();
    match kind {
        VariableKind::Text | VariableKind::Multiline | VariableKind::Choice => {
            Ok((!raw.is_empty()).then(|| Value::String(raw.to_owned())))
        }
        VariableKind::Secret => Ok(None),
        _ if trimmed.is_empty() => Ok(None),
        VariableKind::Number => trimmed
            .parse::<i64>()
            .map(|n| Value::Number(n.into()))
            .or_else(|_| {
                trimmed
                    .parse::<f64>()
                    .ok()
                    .and_then(Number::from_f64)
                    .map(Value::Number)
                    .ok_or(InvalidDefaultText)
            })
            .map(Some),
        VariableKind::Boolean => match trimmed {
            "true" | "oui" => Ok(Some(Value::Bool(true))),
            "false" | "non" => Ok(Some(Value::Bool(false))),
            _ => Err(InvalidDefaultText),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn variable(name: &str, kind: VariableKind) -> ScriptVariable {
        ScriptVariable {
            name: name.to_owned(),
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
    fn manifest_json_round_trips_and_defaults_apply() -> Result<(), serde_json::Error> {
        let raw = r#"[{"name":"crf","type":"number","default":23,"min":0,"max":51,"required":true},{"name":"langue"}]"#;
        let parsed: Vec<ScriptVariable> = serde_json::from_str(raw)?;
        assert_eq!(parsed[0].kind, VariableKind::Number);
        assert_eq!(parsed[0].default_value, Some(json!(23)));
        assert_eq!(parsed[1].kind, VariableKind::Text);
        assert_eq!(parsed[1].display_label(), "langue");
        assert_eq!(validate_variables(&parsed), Ok(()));
        let again: Vec<ScriptVariable> = serde_json::from_str(&serde_json::to_string(&parsed)?)?;
        assert_eq!(again, parsed);
        Ok(())
    }

    #[test]
    fn kinds_have_unique_tags_and_labels() {
        for kind in VariableKind::ALL {
            assert_eq!(VariableKind::from_tag(kind.as_str()), Some(*kind));
            assert!(!kind.label().is_empty());
        }
        assert_eq!(VariableKind::from_tag("inconnu"), None);
    }

    #[test]
    fn rejects_names_reserved_for_the_node_configuration() {
        for reserved in RESERVED_VARIABLE_NAMES {
            assert_eq!(
                validate_variables(&[variable(reserved, VariableKind::Text)]),
                Err(VariableSchemaError::ReservedName(reserved.to_owned()))
            );
        }
    }

    #[test]
    fn rejects_unsafe_and_duplicate_names() {
        let too_long = "a".repeat(MAX_VARIABLE_NAME_LEN + 1);
        for bad in [
            "",
            "1crf",
            "Crf",
            "a-b",
            "mediainfo.codec",
            too_long.as_str(),
        ] {
            assert_eq!(
                validate_variables(&[variable(bad, VariableKind::Text)]),
                Err(VariableSchemaError::InvalidName(bad.to_owned()))
            );
        }
        let twice = [
            variable("crf", VariableKind::Text),
            variable("crf", VariableKind::Number),
        ];
        assert_eq!(
            validate_variables(&twice),
            Err(VariableSchemaError::DuplicateName("crf".into()))
        );
        let many: Vec<ScriptVariable> = (0..=MAX_VARIABLES)
            .map(|n| variable(&format!("v{n}"), VariableKind::Text))
            .collect();
        assert_eq!(validate_variables(&many), Err(VariableSchemaError::TooMany));
    }

    #[test]
    fn checks_defaults_against_the_type() {
        let mut number = variable("crf", VariableKind::Number);
        number.min = Some(Number::from(0));
        number.max = Some(Number::from(51));
        number.default_value = Some(json!(23));
        assert_eq!(validate_variables(std::slice::from_ref(&number)), Ok(()));
        number.default_value = Some(json!(99));
        assert_eq!(
            validate_variables(std::slice::from_ref(&number)),
            Err(VariableSchemaError::InvalidDefault("crf".into()))
        );
        number.default_value = Some(json!("23"));
        assert!(validate_variables(&[number]).is_err());

        let mut flag = variable("supprimer", VariableKind::Boolean);
        flag.default_value = Some(json!(true));
        assert_eq!(validate_variables(&[flag]), Ok(()));

        let mut secret = variable("jeton", VariableKind::Secret);
        secret.default_value = Some(json!("x"));
        assert_eq!(
            validate_variables(&[secret]),
            Err(VariableSchemaError::SecretWithDefault("jeton".into()))
        );
    }

    #[test]
    fn choices_need_distinct_options_and_a_default_among_them() {
        let mut choice = variable("langue", VariableKind::Choice);
        assert_eq!(
            validate_variables(std::slice::from_ref(&choice)),
            Err(VariableSchemaError::InvalidOptions("langue".into()))
        );
        choice.options = vec!["fr".into(), "en".into()];
        choice.default_value = Some(json!("fr"));
        assert_eq!(validate_variables(std::slice::from_ref(&choice)), Ok(()));
        choice.default_value = Some(json!("de"));
        assert!(validate_variables(std::slice::from_ref(&choice)).is_err());
        choice.default_value = None;
        choice.options = vec!["fr".into(), "fr".into()];
        assert!(validate_variables(&[choice]).is_err());

        let mut text = variable("nom", VariableKind::Text);
        text.options = vec!["a".into()];
        assert!(validate_variables(&[text]).is_err());
    }

    #[test]
    fn ranges_apply_to_numbers_only_and_must_be_ordered() {
        let mut number = variable("crf", VariableKind::Number);
        number.min = Some(Number::from(10));
        number.max = Some(Number::from(5));
        assert_eq!(
            validate_variables(&[number]),
            Err(VariableSchemaError::InvalidRange("crf".into()))
        );
        let mut text = variable("nom", VariableKind::Text);
        text.max = Some(Number::from(5));
        assert!(validate_variables(&[text]).is_err());
    }

    #[test]
    fn default_text_conversion_matches_the_kind() {
        assert_eq!(
            default_from_text(VariableKind::Number, " 23 "),
            Ok(Some(json!(23)))
        );
        assert_eq!(
            default_from_text(VariableKind::Number, "1.5"),
            Ok(Some(json!(1.5)))
        );
        assert_eq!(
            default_from_text(VariableKind::Number, "abc"),
            Err(InvalidDefaultText)
        );
        assert_eq!(default_from_text(VariableKind::Number, ""), Ok(None));
        assert_eq!(
            default_from_text(VariableKind::Boolean, "true"),
            Ok(Some(json!(true)))
        );
        assert_eq!(
            default_from_text(VariableKind::Boolean, "peut-être"),
            Err(InvalidDefaultText)
        );
        assert_eq!(
            default_from_text(VariableKind::Text, "fr"),
            Ok(Some(json!("fr")))
        );
        assert_eq!(default_from_text(VariableKind::Secret, "x"), Ok(None));
        assert_eq!(default_to_text(Some(&json!(23))), "23");
        assert_eq!(default_to_text(Some(&json!("fr"))), "fr");
        assert_eq!(default_to_text(None), "");
    }
}
