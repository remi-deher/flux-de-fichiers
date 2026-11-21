//! Évaluation générique d'aiguillage conditionnel sur un arbre JSON de sonde.
//!
//! Une condition est soit une règle simple au format plat historique (`property`, `operator`,
//! `target`), soit un groupe ET/OU de règles (`group`), imbriqué sur au plus
//! [`MAX_CONDITION_DEPTH`] niveaux.

mod config;
mod error;
mod group;
mod operator;
mod path;
mod rule;
#[cfg(test)]
mod tests;

pub use config::{
    ConditionConfig, describe_condition, evaluate_condition, evaluate_condition_with_variables,
    validate_condition,
};
pub use error::ConditionError;
pub use group::{
    ConditionCombinator, ConditionGroup, ConditionNode, MAX_CONDITION_DEPTH, MAX_CONDITION_RULES,
};
pub use operator::{ConditionOperator, IN_LIST_SEPARATOR};
pub use path::extract_json_values;
pub use rule::{ConditionRule, evaluate_rule};
