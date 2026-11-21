//! Configuration racine d'un nœud condition, lue depuis `node.config`.

use super::error::ConditionError;
use super::group::{ConditionGroup, ConditionNode, MAX_CONDITION_DEPTH, MAX_CONDITION_RULES};
use super::operator::ConditionOperator;
use super::rule::{ConditionRule, build_regex, evaluate_rule};
use crate::flows::FlowVariables;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

/// Configuration d'une condition d'aiguillage déclarative.
///
/// Le format plat historique (`property`, `operator`, `target`) reste lu tel quel ; `group`,
/// optionnel, est prioritaire dès qu'il contient au moins une règle. `node.config` porte
/// d'autres clés : aucune n'est refusée.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConditionConfig {
    #[serde(flatten)]
    pub rule: ConditionRule,
    #[serde(default, deserialize_with = "lenient_group", skip_serializing_if = "Option::is_none")]
    pub group: Option<ConditionGroup>,
}

/// Un groupe absent, nul ou mal formé est ignoré : la règle plate reste évaluée.
fn lenient_group<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<ConditionGroup>, D::Error> {
    Ok(serde_json::from_value(Value::deserialize(deserializer)?).ok())
}

impl ConditionConfig {
    /// Condition mono-règle au format plat.
    pub fn single(
        property: impl Into<String>,
        operator: ConditionOperator,
        target: impl Into<String>,
    ) -> Self {
        Self {
            rule: ConditionRule::new(property, operator, target),
            group: None,
        }
    }

    /// Condition composée d'un groupe.
    pub fn grouped(group: ConditionGroup) -> Self {
        Self {
            rule: ConditionRule::default(),
            group: Some(group),
        }
    }

    /// Arbre effectivement évalué : le groupe s'il est non vide, sinon la règle plate.
    pub fn root(&self) -> ConditionNode {
        match &self.group {
            Some(group) if !group.rules.is_empty() => ConditionNode::Group(group.clone()),
            _ => ConditionNode::Rule(self.rule.clone()),
        }
    }
}

/// Évalue une condition d'aiguillage générique sur un arbre JSON issu d'une sonde.
pub fn evaluate_condition(config: &ConditionConfig, probe_info: &Value) -> bool {
    config
        .root()
        .evaluate_with(0, &|rule| evaluate_rule(rule, probe_info))
}

/// Évalue chaque règle sur les variables du flux (`vars.*`, `file.*`…) ou, sinon, sur la sonde.
pub fn evaluate_condition_with_variables(
    config: &ConditionConfig,
    probe_info: &Value,
    variables: &FlowVariables,
) -> bool {
    let document = variables.as_document();
    config.root().evaluate_with(0, &|rule| {
        if FlowVariables::is_variable_property(&rule.property) {
            evaluate_rule(rule, &document)
        } else {
            evaluate_rule(rule, probe_info)
        }
    })
}

/// Description lisible de la condition, utilisée par la simulation et l'aperçu.
pub fn describe_condition(config: &ConditionConfig) -> String {
    config.root().describe()
}

/// Vérifie les bornes (profondeur, nombre de règles) et la validité des expressions régulières.
pub fn validate_condition(config: &ConditionConfig) -> Result<(), ConditionError> {
    let root = config.root();
    if root.depth() > MAX_CONDITION_DEPTH {
        return Err(ConditionError::TooDeep {
            max: MAX_CONDITION_DEPTH,
        });
    }
    if root.rule_count() > MAX_CONDITION_RULES {
        return Err(ConditionError::TooManyRules {
            max: MAX_CONDITION_RULES,
        });
    }
    let mut invalid = None;
    root.for_each_rule(&mut |rule| {
        if invalid.is_none()
            && rule.operator == ConditionOperator::Regex
            && build_regex(&rule.target).is_err()
        {
            invalid = Some(rule.target.clone());
        }
    });
    invalid.map_or(Ok(()), |pattern| Err(ConditionError::InvalidRegex(pattern)))
}
