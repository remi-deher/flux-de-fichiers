//! Opérateur typé d'une règle de condition, tolérant aux chaînes historiques.

use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

/// Séparateur des valeurs admises par l'opérateur `in` (`mkv, mp4, avi`).
pub const IN_LIST_SEPARATOR: char = ',';

/// Opérateur de comparaison d'une règle.
///
/// `eq` et `neq` historiques signifient « contient » et « ne contient pas » (insensible à la
/// casse) : ils restent des alias de [`ConditionOperator::Contains`] et
/// [`ConditionOperator::NotContains`] pour que les flux déjà enregistrés gardent leur sens.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConditionOperator {
    #[default]
    #[serde(alias = "eq")]
    Contains,
    #[serde(alias = "neq")]
    NotContains,
    Exact,
    NotExact,
    Exists,
    NotExists,
    Gt,
    Gte,
    Lt,
    Lte,
    Regex,
    In,
}

impl ConditionOperator {
    /// Tous les opérateurs, dans l'ordre d'affichage de l'inspecteur.
    pub const ALL: [Self; 12] = [
        Self::Contains,
        Self::NotContains,
        Self::Exact,
        Self::NotExact,
        Self::Exists,
        Self::NotExists,
        Self::Gt,
        Self::Gte,
        Self::Lt,
        Self::Lte,
        Self::Regex,
        Self::In,
    ];

    /// Lit une chaîne libre (casse et espaces ignorés) ; une valeur inconnue vaut « contient ».
    pub fn from_loose(raw: &str) -> Self {
        serde_json::from_value(Value::String(raw.trim().to_lowercase())).unwrap_or_default()
    }

    /// Valeur sérialisée canonique.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Contains => "contains",
            Self::NotContains => "not_contains",
            Self::Exact => "exact",
            Self::NotExact => "not_exact",
            Self::Exists => "exists",
            Self::NotExists => "not_exists",
            Self::Gt => "gt",
            Self::Gte => "gte",
            Self::Lt => "lt",
            Self::Lte => "lte",
            Self::Regex => "regex",
            Self::In => "in",
        }
    }

    /// Symbole compact affiché sur les cartes du canevas.
    pub fn symbol(self) -> &'static str {
        match self {
            Self::Contains => "~",
            Self::NotContains => "!~",
            Self::Exact => "=",
            Self::NotExact => "!=",
            Self::Exists => "∃",
            Self::NotExists => "∄",
            Self::Gt => ">",
            Self::Gte => ">=",
            Self::Lt => "<",
            Self::Lte => "<=",
            Self::Regex => "≈",
            Self::In => "∈",
        }
    }

    /// Libellé de l'inspecteur.
    pub fn label(self) -> &'static str {
        match self {
            Self::Contains => "Contient",
            Self::NotContains => "Ne contient pas",
            Self::Exact => "Égal exactement",
            Self::NotExact => "Différent de",
            Self::Exists => "Existe",
            Self::NotExists => "N'existe pas",
            Self::Gt => "Supérieur à",
            Self::Gte => "Supérieur ou égal à",
            Self::Lt => "Inférieur à",
            Self::Lte => "Inférieur ou égal à",
            Self::Regex => "Correspond à l'expression",
            Self::In => "Fait partie de la liste",
        }
    }

    /// Indique si l'opérateur compare à une valeur cible.
    pub fn needs_target(self) -> bool {
        !matches!(self, Self::Exists | Self::NotExists)
    }

    /// Indique si l'opérateur est une négation.
    pub fn is_negation(self) -> bool {
        matches!(self, Self::NotContains | Self::NotExact | Self::NotExists)
    }
}

/// Désérialise un opérateur sans jamais échouer : chaîne libre, sinon « contient ».
pub(super) fn lenient<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<ConditionOperator, D::Error> {
    Ok(match Value::deserialize(deserializer)? {
        Value::String(raw) => ConditionOperator::from_loose(&raw),
        _ => ConditionOperator::default(),
    })
}
