//! Édition pure d'une condition composée : chemins dans l'arbre, conversions plat ↔ groupe.
//!
//! L'éditeur travaille toujours sur un groupe racine. Une condition d'une seule règle, non
//! inversée, est réécrite au format plat historique (`property`, `operator`, `target`) pour rester
//! lisible par les anciens centraux et par les puces du canevas.

use forgearr_shared::flows::{
    ConditionCombinator, ConditionConfig, ConditionGroup, ConditionNode, ConditionOperator,
    ConditionRule, MAX_CONDITION_DEPTH, MAX_CONDITION_RULES, OwnedFieldKind, OwnedFieldSpec,
};
use serde_json::{Map, Value};

/// Clé de configuration des règles combinées.
pub const GROUP_KEY: &str = "group";

/// Clés du format plat, éditées par l'éditeur de condition plutôt que par des champs séparés.
const FLAT_RULE_KEYS: [&str; 3] = ["property", "operator", "target"];

/// Modification appliquée à l'élément désigné par un chemin d'indices depuis le groupe racine.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConditionEdit {
    SetCombinator(ConditionCombinator),
    ToggleNegate,
    AddRule,
    AddGroup,
    Remove,
    SetProperty(String),
    SetOperator(ConditionOperator),
    SetTarget(String),
}

/// Lit la condition d'un bloc ; une configuration illisible vaut la condition par défaut.
pub fn read_config(config: &Value) -> ConditionConfig {
    serde_json::from_value(config.clone()).unwrap_or_default()
}

/// Combinaison lue depuis une valeur de liste ; une valeur inconnue vaut « ET ».
pub fn parse_combinator(raw: &str) -> ConditionCombinator {
    serde_json::from_value(Value::String(raw.to_owned())).unwrap_or_default()
}

/// Groupe racine édité : le groupe enregistré, sinon la règle plate seule.
pub fn working_group(config: &ConditionConfig) -> ConditionGroup {
    match config.root() {
        ConditionNode::Group(group) => group,
        rule @ ConditionNode::Rule(_) => ConditionGroup::new(ConditionCombinator::All, vec![rule]),
    }
}

/// Réécrit la configuration depuis le groupe édité (format plat si une seule règle simple).
fn store(config: &mut ConditionConfig, group: ConditionGroup) {
    let single_rule = match group.rules.as_slice() {
        [ConditionNode::Rule(rule)] if !group.negate => Some(rule.clone()),
        [] => Some(ConditionRule::default()),
        _ => None,
    };
    match single_rule {
        Some(rule) => {
            config.rule = rule;
            config.group = None;
        }
        None => {
            config.rule = ConditionRule::default();
            config.group = Some(group);
        }
    }
}

fn group_at_mut<'a>(
    group: &'a mut ConditionGroup,
    path: &[usize],
) -> Option<&'a mut ConditionGroup> {
    let Some((&head, rest)) = path.split_first() else {
        return Some(group);
    };
    match group.rules.get_mut(head)? {
        ConditionNode::Group(child) => group_at_mut(child, rest),
        ConditionNode::Rule(_) => None,
    }
}

fn rule_at_mut<'a>(
    group: &'a mut ConditionGroup,
    path: &[usize],
) -> Option<&'a mut ConditionRule> {
    let (&last, parent) = path.split_last()?;
    match group_at_mut(group, parent)?.rules.get_mut(last)? {
        ConditionNode::Rule(rule) => Some(rule),
        ConditionNode::Group(_) => None,
    }
}

/// Vrai si un sous-groupe peut être ajouté dans le groupe désigné par `path`.
pub fn can_add_group(path: &[usize]) -> bool {
    path.len() + 2 <= MAX_CONDITION_DEPTH
}

fn has_room(root: &ConditionGroup) -> bool {
    ConditionNode::Group(root.clone()).rule_count() < MAX_CONDITION_RULES
}

fn edit_group(
    root: &mut ConditionGroup,
    path: &[usize],
    change: impl FnOnce(&mut ConditionGroup),
) -> bool {
    let Some(group) = group_at_mut(root, path) else {
        return false;
    };
    change(group);
    true
}

fn edit_rule(
    root: &mut ConditionGroup,
    path: &[usize],
    change: impl FnOnce(&mut ConditionRule),
) -> bool {
    let Some(rule) = rule_at_mut(root, path) else {
        return false;
    };
    change(rule);
    true
}

fn push_child(root: &mut ConditionGroup, path: &[usize], node: ConditionNode) -> bool {
    edit_group(root, path, |group| group.rules.push(node))
}

fn remove_at(root: &mut ConditionGroup, path: &[usize]) -> bool {
    let Some((&last, parent)) = path.split_last() else {
        return false;
    };
    match group_at_mut(root, parent) {
        Some(group) if last < group.rules.len() => {
            group.rules.remove(last);
            true
        }
        _ => false,
    }
}

fn new_subgroup() -> ConditionNode {
    ConditionNode::Group(ConditionGroup::new(
        ConditionCombinator::All,
        vec![ConditionNode::default()],
    ))
}

fn apply_to_group(root: &mut ConditionGroup, path: &[usize], edit: ConditionEdit) -> bool {
    match edit {
        ConditionEdit::SetCombinator(combinator) => {
            edit_group(root, path, |group| group.combinator = combinator)
        }
        ConditionEdit::ToggleNegate => edit_group(root, path, |group| group.negate = !group.negate),
        ConditionEdit::AddRule => {
            has_room(root) && push_child(root, path, ConditionNode::default())
        }
        ConditionEdit::AddGroup => {
            has_room(root) && can_add_group(path) && push_child(root, path, new_subgroup())
        }
        ConditionEdit::Remove => remove_at(root, path),
        ConditionEdit::SetProperty(property) => {
            edit_rule(root, path, |rule| rule.property = property)
        }
        ConditionEdit::SetOperator(operator) => {
            edit_rule(root, path, |rule| rule.operator = operator)
        }
        ConditionEdit::SetTarget(target) => edit_rule(root, path, |rule| rule.target = target),
    }
}

/// Applique une modification ; renvoie `false` si le chemin est invalide ou une borne atteinte.
pub fn apply_edit(config: &mut ConditionConfig, path: &[usize], edit: ConditionEdit) -> bool {
    let mut group = working_group(config);
    let applied = apply_to_group(&mut group, path, edit);
    if applied {
        store(config, group);
    }
    applied
}

/// Clés à fusionner dans `node.config` ; `group` vaut `null` au format plat pour effacer l'ancien.
pub fn condition_patch(config: &ConditionConfig) -> Map<String, Value> {
    let mut patch = match serde_json::to_value(config) {
        Ok(Value::Object(map)) => map,
        _ => Map::new(),
    };
    patch.entry(GROUP_KEY).or_insert(Value::Null);
    patch
}

/// Champs affichés : si le bloc a un éditeur de condition, il remplace les champs plats.
pub fn visible_fields(fields: Vec<OwnedFieldSpec>) -> Vec<OwnedFieldSpec> {
    if !fields
        .iter()
        .any(|field| field.kind == OwnedFieldKind::Condition)
    {
        return fields;
    }
    fields
        .into_iter()
        .filter(|field| !FLAT_RULE_KEYS.contains(&field.name.as_str()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn flat() -> ConditionConfig {
        ConditionConfig::single("codec", ConditionOperator::Exact, "hevc")
    }

    fn field(name: &str, kind: OwnedFieldKind) -> OwnedFieldSpec {
        OwnedFieldSpec {
            name: name.into(),
            label: name.into(),
            kind,
            required: false,
            help: String::new(),
            default: None,
        }
    }

    #[test]
    fn editing_a_single_rule_keeps_the_flat_format() {
        let mut config = flat();
        assert!(apply_edit(&mut config, &[0], ConditionEdit::SetTarget("av1".into())));
        assert_eq!(config, ConditionConfig::single("codec", ConditionOperator::Exact, "av1"));
        assert!(apply_edit(
            &mut config,
            &[0],
            ConditionEdit::SetOperator(ConditionOperator::Regex)
        ));
        assert_eq!(config.rule.operator, ConditionOperator::Regex);
        assert!(config.group.is_none());
    }

    #[test]
    fn adding_a_second_rule_converts_flat_to_group() {
        let mut config = flat();
        assert!(apply_edit(&mut config, &[], ConditionEdit::AddRule));
        assert_eq!(config.rule, ConditionRule::default());
        let Some(group) = &config.group else {
            panic!("un groupe est attendu");
        };
        assert_eq!(group.rules.len(), 2);
        assert_eq!(
            group.rules[0],
            ConditionNode::Rule(ConditionRule::new("codec", ConditionOperator::Exact, "hevc"))
        );
        let patch = condition_patch(&config);
        assert_eq!(patch.get("property"), Some(&json!("")));
        assert!(patch.get(GROUP_KEY).is_some_and(Value::is_object));
    }

    #[test]
    fn removing_back_to_one_rule_returns_to_flat() {
        let mut config = flat();
        assert!(apply_edit(&mut config, &[], ConditionEdit::AddRule));
        assert!(apply_edit(&mut config, &[1], ConditionEdit::Remove));
        assert_eq!(config, flat());
        assert_eq!(condition_patch(&config).get(GROUP_KEY), Some(&Value::Null));
        assert!(!apply_edit(&mut config, &[5], ConditionEdit::Remove));
        assert!(!apply_edit(&mut config, &[], ConditionEdit::Remove));
    }

    #[test]
    fn subgroups_are_bounded_by_the_depth_limit() {
        let mut config = flat();
        let mut path = Vec::new();
        while can_add_group(&path) {
            assert!(apply_edit(&mut config, &path, ConditionEdit::AddGroup));
            let Some(group) = config.group.as_ref().and_then(|root| {
                let mut current = root;
                for &index in &path {
                    let ConditionNode::Group(child) = &current.rules[index] else {
                        return None;
                    };
                    current = child;
                }
                Some(current)
            }) else {
                panic!("le groupe ajouté doit exister");
            };
            path.push(group.rules.len() - 1);
        }
        assert_eq!(path.len() + 1, MAX_CONDITION_DEPTH);
        assert!(!apply_edit(&mut config, &path, ConditionEdit::AddGroup));
        assert!(apply_edit(&mut config, &path, ConditionEdit::AddRule));
    }

    #[test]
    fn combinator_and_negation_apply_to_groups_only() {
        let mut config = flat();
        assert!(apply_edit(
            &mut config,
            &[],
            ConditionEdit::SetCombinator(ConditionCombinator::Any)
        ));
        assert!(apply_edit(&mut config, &[], ConditionEdit::ToggleNegate));
        assert!(config.group.as_ref().is_some_and(|group| group.negate));
        assert!(!apply_edit(&mut config, &[0], ConditionEdit::ToggleNegate));
        assert!(!apply_edit(&mut config, &[], ConditionEdit::SetTarget("x".into())));
    }

    #[test]
    fn rule_count_is_bounded() {
        let mut config = flat();
        for _ in 1..MAX_CONDITION_RULES {
            assert!(apply_edit(&mut config, &[], ConditionEdit::AddRule));
        }
        assert!(!apply_edit(&mut config, &[], ConditionEdit::AddRule));
    }

    #[test]
    fn reads_configs_and_combinators_leniently() {
        assert_eq!(read_config(&json!("x")), ConditionConfig::default());
        assert_eq!(
            read_config(&json!({"property": "codec", "operator": "exact", "target": "hevc"})),
            flat()
        );
        assert_eq!(parse_combinator("any"), ConditionCombinator::Any);
        assert_eq!(parse_combinator("or"), ConditionCombinator::Any);
        assert_eq!(parse_combinator("?"), ConditionCombinator::All);
    }

    #[test]
    fn condition_field_hides_flat_fields() {
        let fields = vec![
            field("property", OwnedFieldKind::Text),
            field("operator", OwnedFieldKind::Text),
            field("target", OwnedFieldKind::Text),
            field(GROUP_KEY, OwnedFieldKind::Condition),
        ];
        let visible = visible_fields(fields);
        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].name, GROUP_KEY);
        let plain = vec![field("property", OwnedFieldKind::Text)];
        assert_eq!(visible_fields(plain).len(), 1);
    }
}
