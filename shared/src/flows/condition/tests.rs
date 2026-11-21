use super::*;
use crate::flows::FlowVariables;
use serde_json::{Value, json};

fn rule(property: &str, operator: &str, target: &str) -> ConditionConfig {
    config_from(json!({"property": property, "operator": operator, "target": target}))
}

fn config_from(value: Value) -> ConditionConfig {
    serde_json::from_value(value).unwrap_or_default()
}

fn leaf(property: &str, operator: ConditionOperator, target: &str) -> ConditionNode {
    ConditionNode::Rule(ConditionRule::new(property, operator, target))
}

fn doc() -> Value {
    json!({
        "format": "EPUB",
        "pages": 320,
        "tracks": [
            {"type": "video", "codec": "HEVC"},
            {"type": "audio", "codec": "AAC", "properties": {"language": "fre"}}
        ]
    })
}

fn nested(levels: usize) -> ConditionGroup {
    let mut group = ConditionGroup::new(
        ConditionCombinator::All,
        vec![leaf("format", ConditionOperator::Exists, "")],
    );
    for _ in 1..levels {
        group = ConditionGroup::new(ConditionCombinator::All, vec![ConditionNode::Group(group)]);
    }
    group
}

#[test]
fn evaluates_structured_array_predicates_and_generic_documents() {
    let doc = doc();
    assert!(evaluate_condition(
        &rule("tracks[type=video].codec", "eq", "hevc"),
        &doc
    ));
    assert!(evaluate_condition(
        &rule("tracks[type=video].codec", "neq", "h264"),
        &doc
    ));
    assert!(evaluate_condition(
        &rule("tracks[type=audio].properties.language", "contains", "fre"),
        &doc
    ));
    assert!(evaluate_condition(&rule("format", "exact", "epub"), &doc));
    assert!(evaluate_condition(&rule("pages", "gt", "200"), &doc));
    assert!(evaluate_condition(&rule("any", "eq", "aac"), &doc));
    assert!(!evaluate_condition(
        &rule("missing.path", "eq", "hevc"),
        &doc
    ));
}

#[test]
fn legacy_eq_still_means_contains() {
    assert_eq!(
        ConditionOperator::from_loose("eq"),
        ConditionOperator::Contains
    );
    assert!(evaluate_condition(
        &rule("tracks[type=video].codec", "eq", "hev"),
        &doc()
    ));
}

#[test]
fn neq_alias_maps_to_not_contains() {
    assert_eq!(
        ConditionOperator::from_loose("neq"),
        ConditionOperator::NotContains
    );
    assert!(!evaluate_condition(
        &rule("tracks[type=video].codec", "neq", "hev"),
        &doc()
    ));
}

/// Non-régression : avant les opérateurs typés, ces chaînes inconnues signifiaient « contient ».
#[test]
fn legacy_unknown_operators_keep_contains_meaning() {
    for legacy in ["ne", "equals", "match", "ge", "le"] {
        assert_eq!(
            ConditionOperator::from_loose(legacy),
            ConditionOperator::Contains
        );
    }
    assert!(evaluate_condition(
        &rule("tracks[type=video].codec", "ne", "hevc"),
        &doc()
    ));
}

#[test]
fn unknown_or_uppercase_operator_falls_back() {
    assert_eq!(
        rule("format", "EQ", "ep").rule.operator,
        ConditionOperator::Contains
    );
    assert_eq!(
        rule("format", "foo", "ep").rule.operator,
        ConditionOperator::Contains
    );
    assert_eq!(
        rule("format", " GTE ", "1").rule.operator,
        ConditionOperator::Gte
    );
    let numeric_operator = config_from(json!({"property": "format", "operator": 3}));
    assert_eq!(numeric_operator.rule.operator, ConditionOperator::Contains);
}

#[test]
fn serializes_canonical_names() {
    for operator in ConditionOperator::ALL {
        assert_eq!(
            serde_json::to_value(operator).ok(),
            Some(json!(operator.as_str()))
        );
        assert_eq!(ConditionOperator::from_loose(operator.as_str()), operator);
    }
    for combinator in ConditionCombinator::ALL {
        assert_eq!(
            serde_json::to_value(combinator).ok(),
            Some(json!(combinator.as_str()))
        );
    }
}

#[test]
fn flat_legacy_config_deserializes_without_group() {
    let config = config_from(json!({
        "property": "format", "operator": "eq", "target": "epub", "label": "autre clé"
    }));
    assert_eq!(
        config,
        ConditionConfig::single("format", ConditionOperator::Contains, "epub")
    );
    let serialized = serde_json::to_value(&config).unwrap_or_default();
    assert!(serialized.get("group").is_none());
    assert_eq!(config_from(json!({})), ConditionConfig::default());
}

#[test]
fn group_all_and_any() {
    let doc = doc();
    let rules = vec![
        leaf("format", ConditionOperator::Exact, "epub"),
        leaf("pages", ConditionOperator::Lt, "100"),
    ];
    let all =
        ConditionConfig::grouped(ConditionGroup::new(ConditionCombinator::All, rules.clone()));
    let any = ConditionConfig::grouped(ConditionGroup::new(ConditionCombinator::Any, rules));
    assert!(!evaluate_condition(&all, &doc));
    assert!(evaluate_condition(&any, &doc));
    assert_eq!(describe_condition(&all), "(format = epub ET pages < 100)");
    assert_eq!(describe_condition(&any), "(format = epub OU pages < 100)");
}

#[test]
fn legacy_combinator_aliases() {
    let config = config_from(json!({"group": {"combinator": "or", "rules": [
        {"property": "format", "operator": "exact", "target": "pdf"},
        {"property": "pages", "operator": "gte", "target": "320"}
    ]}}));
    assert_eq!(
        config.group.as_ref().map(|group| group.combinator),
        Some(ConditionCombinator::Any)
    );
    assert!(evaluate_condition(&config, &doc()));
}

#[test]
fn nested_group_respects_depth_limit() {
    let allowed = ConditionConfig::grouped(nested(MAX_CONDITION_DEPTH));
    assert!(evaluate_condition(&allowed, &doc()));
    assert_eq!(validate_condition(&allowed), Ok(()));

    let too_deep = ConditionConfig::grouped(nested(MAX_CONDITION_DEPTH + 1));
    assert!(!evaluate_condition(&too_deep, &doc()));
    assert_eq!(
        validate_condition(&too_deep),
        Err(ConditionError::TooDeep {
            max: MAX_CONDITION_DEPTH
        })
    );
}

#[test]
fn too_many_rules_is_rejected() {
    let rules = (0..=MAX_CONDITION_RULES)
        .map(|_| leaf("format", ConditionOperator::Exists, ""))
        .collect();
    let config = ConditionConfig::grouped(ConditionGroup::new(ConditionCombinator::All, rules));
    assert_eq!(
        validate_condition(&config),
        Err(ConditionError::TooManyRules {
            max: MAX_CONDITION_RULES
        })
    );
}

#[test]
fn negated_group() {
    let mut group = ConditionGroup::new(
        ConditionCombinator::All,
        vec![leaf("format", ConditionOperator::Exact, "epub")],
    );
    group.negate = true;
    let config = ConditionConfig::grouped(group);
    assert!(!evaluate_condition(&config, &doc()));
    assert_eq!(describe_condition(&config), "NON (format = epub)");
}

#[test]
fn regex_operator_case_insensitive_and_invalid_is_false() {
    let doc = doc();
    assert!(evaluate_condition(
        &rule("tracks[type=video].codec", "regex", "^hev"),
        &doc
    ));
    assert!(evaluate_condition(&rule("format", "regex", "^E.UB$"), &doc));
    let invalid = rule("format", "regex", "(");
    assert!(!evaluate_condition(&invalid, &doc));
    assert!(matches!(
        validate_condition(&invalid),
        Err(ConditionError::InvalidRegex(_))
    ));
}

#[test]
fn in_operator_splits_list() {
    let doc = doc();
    assert!(evaluate_condition(
        &rule("format", "in", "pdf, EPUB ,mobi"),
        &doc
    ));
    assert!(!evaluate_condition(&rule("format", "in", "pdf,mobi"), &doc));
    assert!(!evaluate_condition(&rule("format", "in", "ep"), &doc));
}

#[test]
fn not_exact() {
    let doc = doc();
    assert!(evaluate_condition(
        &rule("format", "not_exact", "epu"),
        &doc
    ));
    assert!(!evaluate_condition(
        &rule("format", "not_exact", "epub"),
        &doc
    ));
}

#[test]
fn any_property_with_negation() {
    let doc = doc();
    assert!(evaluate_condition(&rule("*", "not_contains", "h264"), &doc));
    assert!(!evaluate_condition(&rule("", "neq", "aac"), &doc));
    assert!(evaluate_condition(&rule("ANY", "contains", "aac"), &doc));
}

#[test]
fn untagged_prefers_group_when_rules_present() {
    let node: ConditionNode = serde_json::from_value(json!({
        "rules": [{"property": "format", "operator": "exists"}]
    }))
    .unwrap_or_default();
    assert!(matches!(node, ConditionNode::Group(_)));
    let rule_node: ConditionNode =
        serde_json::from_value(json!({"property": "format"})).unwrap_or_default();
    assert!(matches!(rule_node, ConditionNode::Rule(_)));
}

#[test]
fn round_trip_config_with_group() {
    let mut inner = ConditionGroup::new(
        ConditionCombinator::Any,
        vec![
            leaf("format", ConditionOperator::In, "epub,pdf"),
            leaf("pages", ConditionOperator::Gte, "10"),
        ],
    );
    inner.negate = true;
    let config = ConditionConfig::grouped(ConditionGroup::new(
        ConditionCombinator::All,
        vec![
            leaf("format", ConditionOperator::Regex, "^e"),
            ConditionNode::Group(inner),
        ],
    ));
    let value = serde_json::to_value(&config).unwrap_or_default();
    assert_eq!(config_from(value), config);
}

#[test]
fn numeric_target_accepted_if_lenient_string() {
    let config = config_from(json!({"property": "pages", "operator": "gte", "target": 320}));
    assert_eq!(config.rule.target, "320");
    assert!(evaluate_condition(&config, &doc()));
    let flag = config_from(json!({"property": "x", "operator": "exact", "target": true}));
    assert_eq!(flag.rule.target, "true");
}

#[test]
fn empty_group_falls_back_to_flat_rule() {
    let config = config_from(json!({
        "property": "format", "operator": "exact", "target": "epub", "group": {"rules": []}
    }));
    assert!(matches!(config.root(), ConditionNode::Rule(_)));
    assert!(evaluate_condition(&config, &doc()));
    for malformed in [json!(null), json!({}), json!("texte")] {
        let config = config_from(json!({
            "property": "format", "operator": "exact", "target": "epub", "group": malformed
        }));
        assert_eq!(config.group, None);
        assert!(evaluate_condition(&config, &doc()));
    }
}

#[test]
fn describe_rule_without_target() {
    let config = ConditionConfig::single("", ConditionOperator::Exists, "ignoré");
    assert_eq!(describe_condition(&config), "any ∃");
    assert!(
        ConditionOperator::ALL
            .iter()
            .all(|op| !op.label().is_empty())
    );
}

#[test]
fn reads_flow_variables_and_falls_back_to_the_probe() {
    let mut variables = FlowVariables::default();
    variables
        .set_custom("codec", json!("hevc"))
        .unwrap_or_default();
    variables.set_intrinsic(crate::flows::IntrinsicVariable::FileExt, json!("mkv"));
    let probe = json!({"format": {"format_name": "matroska"}});
    let check =
        |config: ConditionConfig| evaluate_condition_with_variables(&config, &probe, &variables);
    assert!(check(rule("vars.codec", "eq", "hevc")));
    assert!(check(rule("file.ext", "exact", "mkv")));
    assert!(check(rule("format.format_name", "eq", "matroska")));
    assert!(check(rule("vars.absente", "not_exists", "")));
    assert!(check(ConditionConfig::grouped(ConditionGroup::new(
        ConditionCombinator::All,
        vec![
            leaf("vars.codec", ConditionOperator::Exact, "hevc"),
            leaf("format.format_name", ConditionOperator::Contains, "matro"),
        ],
    ))));
}
