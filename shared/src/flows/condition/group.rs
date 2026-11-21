//! Groupes ET/OU de règles, imbrication bornée.

use super::rule::{ConditionRule, describe_rule};
use serde::{Deserialize, Serialize};

/// Nombre maximal de niveaux de groupes imbriqués (le groupe racine compte pour un).
pub const MAX_CONDITION_DEPTH: usize = 4;

/// Nombre maximal de règles simples dans une condition.
pub const MAX_CONDITION_RULES: usize = 32;

/// Combinaison des règles d'un groupe.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConditionCombinator {
    /// Toutes les règles doivent être vraies (ET).
    #[default]
    #[serde(alias = "and")]
    All,
    /// Au moins une règle doit être vraie (OU).
    #[serde(alias = "or")]
    Any,
}

impl ConditionCombinator {
    pub const ALL: [Self; 2] = [Self::All, Self::Any];

    /// Libellé court : `ET` ou `OU`.
    pub fn label(self) -> &'static str {
        match self {
            Self::All => "ET",
            Self::Any => "OU",
        }
    }

    /// Valeur sérialisée canonique.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Any => "any",
        }
    }
}

/// Élément d'un groupe : sous-groupe ou règle simple.
///
/// `Group` est essayé en premier : il exige la clé `rules`, alors qu'une règle accepte tout objet.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ConditionNode {
    Group(ConditionGroup),
    Rule(ConditionRule),
}

/// Groupe de règles combinées par ET ou OU, éventuellement inversé.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConditionGroup {
    #[serde(default)]
    pub combinator: ConditionCombinator,
    /// Obligatoire : c'est le discriminant qui distingue un groupe d'une règle.
    pub rules: Vec<ConditionNode>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub negate: bool,
}

impl Default for ConditionNode {
    fn default() -> Self {
        Self::Rule(ConditionRule::default())
    }
}

impl ConditionNode {
    /// Évalue le nœud ; `depth` compte les groupes déjà traversés (0 à la racine).
    pub fn evaluate_with(&self, depth: usize, eval_rule: &dyn Fn(&ConditionRule) -> bool) -> bool {
        match self {
            Self::Rule(rule) => eval_rule(rule),
            Self::Group(group) => group.evaluate_with(depth, eval_rule),
        }
    }

    /// Nombre de niveaux de groupes (0 pour une règle simple).
    pub fn depth(&self) -> usize {
        match self {
            Self::Rule(_) => 0,
            Self::Group(group) => group.depth(),
        }
    }

    /// Parcourt toutes les règles simples, en profondeur.
    pub fn for_each_rule(&self, visit: &mut dyn FnMut(&ConditionRule)) {
        match self {
            Self::Rule(rule) => visit(rule),
            Self::Group(group) => group
                .rules
                .iter()
                .for_each(|child| child.for_each_rule(visit)),
        }
    }

    /// Nombre total de règles simples.
    pub fn rule_count(&self) -> usize {
        let mut count = 0;
        self.for_each_rule(&mut |_| count += 1);
        count
    }

    /// Description lisible : `(a ~ x ET b > 3)`.
    pub fn describe(&self) -> String {
        match self {
            Self::Rule(rule) => describe_rule(rule),
            Self::Group(group) => group.describe(),
        }
    }
}

impl ConditionGroup {
    pub fn new(combinator: ConditionCombinator, rules: Vec<ConditionNode>) -> Self {
        Self {
            combinator,
            rules,
            negate: false,
        }
    }

    /// Évalue le groupe ; au-delà de [`MAX_CONDITION_DEPTH`], renvoie `false` par prudence.
    pub fn evaluate_with(&self, depth: usize, eval_rule: &dyn Fn(&ConditionRule) -> bool) -> bool {
        if depth >= MAX_CONDITION_DEPTH {
            return false;
        }
        let child = |node: &ConditionNode| node.evaluate_with(depth + 1, eval_rule);
        let result = match self.combinator {
            ConditionCombinator::All => self.rules.iter().all(child),
            ConditionCombinator::Any => self.rules.iter().any(child),
        };
        result != self.negate
    }

    /// Nombre de niveaux de groupes, celui-ci compris.
    pub fn depth(&self) -> usize {
        1 + self.rules.iter().map(ConditionNode::depth).max().unwrap_or(0)
    }

    fn describe(&self) -> String {
        let separator = format!(" {} ", self.combinator.label());
        let inner: Vec<String> = self.rules.iter().map(ConditionNode::describe).collect();
        let body = format!("({})", inner.join(&separator));
        if self.negate {
            format!("NON {body}")
        } else {
            body
        }
    }
}
