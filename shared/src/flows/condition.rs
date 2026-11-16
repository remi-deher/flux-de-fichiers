//! Évaluation générique d'aiguillage conditionnel sur un arbre JSON de sonde.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Configuration d'une condition d'aiguillage déclarative.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConditionConfig {
    #[serde(default)]
    pub property: String,
    #[serde(default = "default_op")]
    pub operator: String,
    #[serde(default)]
    pub target: String,
}

fn default_op() -> String {
    "eq".into()
}

fn literal_matches(value: &Value, expected: &str) -> bool {
    let expected = expected.trim();
    match value {
        Value::String(text) => text.eq_ignore_ascii_case(expected),
        Value::Bool(flag) => flag.to_string().eq_ignore_ascii_case(expected),
        Value::Number(num) => num.to_string() == expected,
        _ => false,
    }
}

fn collect_values<'a>(current: &'a Value, segments: &[&str], out: &mut Vec<&'a Value>) {
    let Some((&head, rest)) = segments.split_first() else {
        out.push(current);
        return;
    };

    if let Some(open) = head.find('[')
        && let Some(inside) = head[open + 1..].strip_suffix(']')
    {
        let key = &head[..open];
        let target = if key.is_empty() {
            Some(current)
        } else {
            current.get(key)
        };
        let Some(items) = target.and_then(Value::as_array) else {
            return;
        };
        if inside == "*" {
            for item in items {
                collect_values(item, rest, out);
            }
        } else if let Some((filter_key, filter_val)) = inside.split_once('=') {
            let filter_key = filter_key.trim();
            for item in items {
                if item
                    .get(filter_key)
                    .is_some_and(|val| literal_matches(val, filter_val))
                {
                    collect_values(item, rest, out);
                }
            }
        } else if let Ok(index) = inside.parse::<usize>()
            && let Some(item) = items.get(index)
        {
            collect_values(item, rest, out);
        }
        return;
    }

    if let Some(next) = current.get(head) {
        collect_values(next, rest, out);
    }
}

/// Extrait toutes les valeurs pointées par une requête JSON (`a.b`, `arr[*]`, `arr[0]`, `arr[type=video].codec`).
pub fn extract_json_values<'a>(root: &'a Value, query: &str) -> Vec<&'a Value> {
    let segments: Vec<&str> = query.split('.').filter(|s| !s.is_empty()).collect();
    let mut out = Vec::new();
    collect_values(root, &segments, &mut out);
    out
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

/// Évalue une condition d'aiguillage générique sur un arbre JSON issu d'une sonde.
pub fn evaluate_condition(config: &ConditionConfig, probe_info: &Value) -> bool {
    let prop = config.property.trim();
    let op = config.operator.trim().to_lowercase();
    let target = config.target.trim().to_lowercase();

    if prop.is_empty() || prop.eq_ignore_ascii_case("any") || prop == "*" {
        let match_found = probe_info.to_string().to_lowercase().contains(&target);
        return match op.as_str() {
            "neq" | "not_contains" => !match_found,
            _ => match_found,
        };
    }

    let values = extract_json_values(probe_info, prop);
    match op.as_str() {
        "exists" => !values.is_empty(),
        "not_exists" => values.is_empty(),
        "exact" => values
            .iter()
            .any(|v| value_as_lower_string(v).trim() == target),
        "gt" | "gte" | "lt" | "lte" => {
            let Ok(rhs) = target.parse::<f64>() else {
                return false;
            };
            values
                .iter()
                .filter_map(|v| value_as_f64(v))
                .any(|lhs| match op.as_str() {
                    "gt" => lhs > rhs,
                    "gte" => lhs >= rhs,
                    "lt" => lhs < rhs,
                    "lte" => lhs <= rhs,
                    _ => false,
                })
        }
        "neq" | "not_contains" => !values
            .iter()
            .any(|v| value_as_lower_string(v).contains(&target)),
        _ => values
            .iter()
            .any(|v| value_as_lower_string(v).contains(&target)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn evaluates_structured_array_predicates_and_generic_documents() {
        let doc = json!({
            "format": "EPUB",
            "pages": 320,
            "tracks": [
                {"type": "video", "codec": "HEVC"},
                {"type": "audio", "codec": "AAC", "properties": {"language": "fre"}}
            ]
        });
        assert!(evaluate_condition(
            &ConditionConfig {
                property: "tracks[type=video].codec".into(),
                operator: "eq".into(),
                target: "hevc".into(),
            },
            &doc
        ));
        assert!(evaluate_condition(
            &ConditionConfig {
                property: "tracks[type=video].codec".into(),
                operator: "neq".into(),
                target: "h264".into(),
            },
            &doc
        ));
        assert!(evaluate_condition(
            &ConditionConfig {
                property: "tracks[type=audio].properties.language".into(),
                operator: "contains".into(),
                target: "fre".into(),
            },
            &doc
        ));
        assert!(evaluate_condition(
            &ConditionConfig {
                property: "format".into(),
                operator: "exact".into(),
                target: "epub".into(),
            },
            &doc
        ));
        assert!(evaluate_condition(
            &ConditionConfig {
                property: "pages".into(),
                operator: "gt".into(),
                target: "200".into(),
            },
            &doc
        ));
        assert!(evaluate_condition(
            &ConditionConfig {
                property: "any".into(),
                operator: "eq".into(),
                target: "aac".into(),
            },
            &doc
        ));
        assert!(!evaluate_condition(
            &ConditionConfig {
                property: "missing.path".into(),
                operator: "eq".into(),
                target: "hevc".into(),
            },
            &doc
        ));
    }
}
