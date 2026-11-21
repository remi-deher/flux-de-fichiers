//! Erreurs de validation d'une condition.

use std::fmt;

/// Condition refusée à l'enregistrement d'un flux.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConditionError {
    TooDeep { max: usize },
    TooManyRules { max: usize },
    InvalidRegex(String),
}

impl fmt::Display for ConditionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooDeep { max } => {
                write!(f, "condition trop imbriquée (au plus {max} niveaux de groupes)")
            }
            Self::TooManyRules { max } => write!(f, "condition trop longue (au plus {max} règles)"),
            Self::InvalidRegex(pattern) => write!(f, "expression régulière invalide « {pattern} »"),
        }
    }
}

impl std::error::Error for ConditionError {}
