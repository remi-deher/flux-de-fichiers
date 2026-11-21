//! Requêtes de chemin sur un arbre JSON (`a.b`, `arr[*]`, `arr[0]`, `arr[type=video].codec`).

use serde_json::Value;

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
