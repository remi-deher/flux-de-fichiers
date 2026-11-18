//! Aperçu lisible des arguments d'une action d'écriture, montré avant confirmation.

use serde_json::{Map, Value};

const MASK: &str = "***";
const MAX_PREVIEW_CHARS: usize = 2000;
/// Un script est montré presque en entier : l'utilisateur doit pouvoir relire ce qu'il autorise.
const MAX_CODE_PREVIEW_CHARS: usize = 20_000;
const CODE_FIELD: &str = "code";
/// Fragments de nom de champ dont la valeur est un secret ; comparés en minuscules.
const SECRET_KEY_FRAGMENTS: [&str; 8] = [
    "secret",
    "token",
    "password",
    "passwd",
    "apikey",
    "api_key",
    "authorization",
    "webhook",
];

fn is_secret_key(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    SECRET_KEY_FRAGMENTS
        .iter()
        .any(|fragment| lower.contains(fragment))
        || lower == "headers"
        || lower == "custom_headers"
        || lower == "customheaders"
}

fn mask(value: &Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, inner)| {
                    let shown = if is_secret_key(key) {
                        Value::String(MASK.to_owned())
                    } else {
                        mask(inner)
                    };
                    (key.clone(), shown)
                })
                .collect::<Map<_, _>>(),
        ),
        Value::Array(items) => Value::Array(items.iter().map(mask).collect()),
        other => other.clone(),
    }
}

fn truncate(text: &str, limit: usize) -> String {
    let mut chars = text.chars();
    let head: String = chars.by_ref().take(limit).collect();
    if chars.next().is_some() {
        format!("{head}\n…")
    } else {
        head
    }
}

/// Arguments en JSON indenté, secrets masqués, bornés à `MAX_PREVIEW_CHARS` caractères.
///
/// Le champ `code` d'un script est montré à part, tel quel : c'est ce que l'utilisateur autorise.
pub fn action_preview(args: &Value) -> String {
    let code = args.get(CODE_FIELD).and_then(Value::as_str);
    let mut rest = mask(args);
    if code.is_some()
        && let Some(map) = rest.as_object_mut()
    {
        map.remove(CODE_FIELD);
    }
    let head = truncate(
        &serde_json::to_string_pretty(&rest).unwrap_or_else(|_| "{}".to_owned()),
        MAX_PREVIEW_CHARS,
    );
    match code {
        Some(code) => format!(
            "{head}\n\n{CODE_FIELD} :\n{}",
            truncate(code, MAX_CODE_PREVIEW_CHARS)
        ),
        None => head,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn secrets_are_masked_at_any_depth_and_other_fields_stay_readable() {
        let args = json!({
            "id": "plugin.radarr",
            "settings": { "apiKey": "super-secret-value", "url": "http://radarr" },
            "custom_headers": { "X-Auth": "abc" },
            "targets": [{ "webhookUrl": "https://hooks/secret", "name": "n" }]
        });
        let preview = action_preview(&args);
        assert!(preview.contains("plugin.radarr") && preview.contains("http://radarr"));
        assert!(preview.contains("\"name\": \"n\""));
        for leaked in ["super-secret-value", "abc", "hooks/secret"] {
            assert!(!preview.contains(leaked), "{leaked} ne doit pas apparaître");
        }
    }

    #[test]
    fn a_script_is_shown_in_full_next_to_its_masked_settings() {
        let code = "#!/bin/sh\necho \"bonjour\"\n".repeat(50);
        let preview = action_preview(&json!({
            "name": "Nettoyage",
            "interpreter": "sh",
            "token": "ne-pas-montrer",
            "code": code
        }));
        assert!(preview.contains("\"name\": \"Nettoyage\""));
        assert!(preview.contains(&code));
        assert!(!preview.contains("ne-pas-montrer"));
        let huge = action_preview(&json!({ "code": "y".repeat(MAX_CODE_PREVIEW_CHARS * 2) }));
        assert!(huge.ends_with('…'));
    }

    #[test]
    fn long_previews_are_cut() {
        let preview = action_preview(&json!({ "blob": "x".repeat(MAX_PREVIEW_CHARS * 2) }));
        assert!(preview.chars().count() <= MAX_PREVIEW_CHARS + 2);
        assert!(preview.ends_with('…'));
    }
}
