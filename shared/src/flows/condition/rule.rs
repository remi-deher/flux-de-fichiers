//! Règle simple `propriété opérateur cible` et son évaluation sur un arbre JSON.

use super::operator::{self, ConditionOperator, IN_LIST_SEPARATOR};
use super::path::extract_json_values;
use regex::{Regex, RegexBuilder};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

/// Taille maximale d'une expression régulière compilée (garde-fou mémoire).
pub const REGEX_SIZE_LIMIT: usize = 1 << 20;

/// Propriétés qui désignent tout le document plutôt qu'un chemin.
const WILDCARD_PROPERTIES: [&str; 2] = ["any", "*"];

/// Libellé d'une propriété vide dans les descriptions.
const WILDCARD_LABEL: &str = "any";

/// Désérialise un texte en acceptant nombres et booléens (configurations écrites par l'IA ou MCP).
fn lenient_string<'de, D: Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
    Ok(match Value::deserialize(deserializer)? {
        Value::String(text) => text,
        Value::Number(number) => number.to_string(),
        Value::Bool(flag) => flag.to_string(),
        _ => String::new(),
    })
}

/// Règle simple d'aiguillage : `property operator target`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConditionRule {
    #[serde(default, deserialize_with = "lenient_string")]
    pub property: String,
    #[serde(default, deserialize_with = "operator::lenient")]
    pub operator: ConditionOperator,
    #[serde(default, deserialize_with = "lenient_string")]
    pub target: String,
}

impl ConditionRule {
    pub fn new(
        property: impl Into<String>,
        operator: ConditionOperator,
        target: impl Into<String>,
    ) -> Self {
        Self {
            property: property.into(),
            operator,
            target: target.into(),
        }
    }

    /// Vrai si la règle porte sur tout le document (`any`, `*` ou propriété vide).
    pub fn is_wildcard(&self) -> bool {
        let property = self.property.trim();
        property.is_empty()
            || WILDCARD_PROPERTIES
                .iter()
                .any(|wildcard| property.eq_ignore_ascii_case(wildcard))
    }
}

/// Compile une expression de règle `regex` (insensible à la casse, taille bornée).
pub fn build_regex(pattern: &str) -> Result<Regex, regex::Error> {
    RegexBuilder::new(pattern.trim())
        .case_insensitive(true)
        .size_limit(REGEX_SIZE_LIMIT)
        .build()
}

fn value_as_lower_string(value: &Value) -> String {
    match value {
        Value::String(text) => text.to_lowercase(),
        Value::Null => String::new(),
        other => other.to_string().to_lowercase(),
    }
}

fn value_as_f64(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_str().and_then(|s| s.trim().parse::<f64>().ok()))
}

fn compare(operator: ConditionOperator, lhs: f64, rhs: f64) -> Option<bool> {
    match operator {
        ConditionOperator::Gt => Some(lhs > rhs),
        ConditionOperator::Gte => Some(lhs >= rhs),
        ConditionOperator::Lt => Some(lhs < rhs),
        ConditionOperator::Lte => Some(lhs <= rhs),
        _ => None,
    }
}

fn any_contains(values: &[&Value], target: &str) -> bool {
    values
        .iter()
        .any(|value| value_as_lower_string(value).contains(target))
}

fn any_exact(values: &[&Value], target: &str) -> bool {
    values
        .iter()
        .any(|value| value_as_lower_string(value).trim() == target)
}

fn any_numeric(operator: ConditionOperator, values: &[&Value], target: &str) -> bool {
    let Ok(rhs) = target.parse::<f64>() else {
        return false;
    };
    values
        .iter()
        .filter_map(|value| value_as_f64(value))
        .any(|lhs| compare(operator, lhs, rhs).unwrap_or(false))
}

fn any_regex(values: &[&Value], pattern: &str) -> bool {
    let Ok(regex) = build_regex(pattern) else {
        return false;
    };
    values
        .iter()
        .any(|value| regex.is_match(&value_as_lower_string(value)))
}

fn any_in(values: &[&Value], target: &str) -> bool {
    let options: Vec<&str> = target
        .split(IN_LIST_SEPARATOR)
        .map(str::trim)
        .filter(|option| !option.is_empty())
        .collect();
    values
        .iter()
        .any(|value| options.contains(&value_as_lower_string(value).trim()))
}

/// Évalue une règle simple sur un arbre JSON issu d'une sonde.
pub fn evaluate_rule(rule: &ConditionRule, probe_info: &Value) -> bool {
    let target = rule.target.trim().to_lowercase();

    if rule.is_wildcard() {
        let found = probe_info.to_string().to_lowercase().contains(&target);
        return found != rule.operator.is_negation();
    }

    let values = extract_json_values(probe_info, rule.property.trim());
    match rule.operator {
        ConditionOperator::Contains => any_contains(&values, &target),
        ConditionOperator::NotContains => !any_contains(&values, &target),
        ConditionOperator::Exact => any_exact(&values, &target),
        ConditionOperator::NotExact => !any_exact(&values, &target),
        ConditionOperator::Exists => !values.is_empty(),
        ConditionOperator::NotExists => values.is_empty(),
        ConditionOperator::Gt
        | ConditionOperator::Gte
        | ConditionOperator::Lt
        | ConditionOperator::Lte => any_numeric(rule.operator, &values, &target),
        ConditionOperator::Regex => any_regex(&values, &rule.target),
        ConditionOperator::In => any_in(&values, &target),
    }
}

/// Décrit une règle en une ligne : `propriété ~ cible`.
pub fn describe_rule(rule: &ConditionRule) -> String {
    let property = match rule.property.trim() {
        "" => WILDCARD_LABEL,
        other => other,
    };
    let symbol = rule.operator.symbol();
    if rule.operator.needs_target() {
        format!("{property} {symbol} {}", rule.target.trim())
    } else {
        format!("{property} {symbol}")
    }
}
