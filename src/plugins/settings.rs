//! Schéma de réglages déclaré par un manifeste, et résolution d'affichage.
//!
//! `settingsSchema` n'est pas dans le schéma JSON du manifeste ; Sonarr et Radarr
//! le publient déjà. Le dépôt tranche : on le lit. Les défauts préremplissent
//! un champ seulement si la clé n'a jamais été enregistrée.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// Type de champ connu des manifestes Sonarr et Radarr. Une valeur inconnue est refusée.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SettingsFieldType {
    Url,
    Secret,
    String,
}

/// Un champ de `settingsSchema.fields`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettingsField {
    pub name: String,
    pub label: String,
    #[serde(rename = "type")]
    pub field_type: SettingsFieldType,
    #[serde(default, rename = "default")]
    pub default_value: Option<String>,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub description: Option<String>,
}

/// Objet `settingsSchema` d'un manifeste de plugin.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettingsSchema {
    pub fields: Vec<SettingsField>,
}

/// Champ prêt à être affiché. `value` est la proposition, jamais une écriture en base.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedSettingsField {
    pub name: String,
    pub label: String,
    #[serde(rename = "type")]
    pub field_type: SettingsFieldType,
    pub required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub value: String,
}

/// Valeur affichée : clé sauvée (même vide), sinon défaut du manifeste, sinon vide.
///
/// Une valeur sauvée qui n'est pas une chaîne reste gagnante : elle s'affiche vide
/// et ne laisse pas revenir le défaut. L'objet sauvé n'est pas modifié.
pub fn resolve_setting_value(saved: Option<&Map<String, Value>>, field: &SettingsField) -> String {
    if let Some(map) = saved
        && let Some(value) = map.get(&field.name)
    {
        return match value {
            Value::String(text) => text.clone(),
            _ => String::new(),
        };
    }
    field.default_value.clone().unwrap_or_default()
}

pub fn resolve_settings_fields(
    saved: &Value,
    fields: &[SettingsField],
) -> Vec<ResolvedSettingsField> {
    let saved_map = saved.as_object();
    fields
        .iter()
        .map(|field| ResolvedSettingsField {
            name: field.name.clone(),
            label: field.label.clone(),
            field_type: field.field_type,
            required: field.required,
            description: field.description.clone(),
            value: resolve_setting_value(saved_map, field),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(name: &str, default_value: Option<&str>) -> SettingsField {
        SettingsField {
            name: name.to_owned(),
            label: name.to_owned(),
            field_type: SettingsFieldType::String,
            default_value: default_value.map(str::to_owned),
            required: false,
            description: None,
        }
    }

    #[test]
    fn saved_value_wins_over_default_including_empty() {
        let saved = serde_json::json!({
            "url": "",
            "apiKey": "clef",
            "pathMappingLocal": null
        });
        let map = saved.as_object();
        assert_eq!(
            resolve_setting_value(map, &field("url", Some("http://default"))),
            ""
        );
        assert_eq!(
            resolve_setting_value(map, &field("apiKey", Some("unused"))),
            "clef"
        );
        assert_eq!(
            resolve_setting_value(map, &field("pathMappingLocal", Some("/data"))),
            ""
        );
        assert_eq!(
            resolve_setting_value(map, &field("pathMappingRemote", Some("/tv"))),
            "/tv"
        );
        assert_eq!(resolve_setting_value(map, &field("note", None)), "");
        assert_eq!(saved["url"], "");
        assert!(saved.get("pathMappingRemote").is_none());
        assert!(saved.get("note").is_none());
    }

    #[test]
    fn resolving_a_form_does_not_write_defaults_into_the_saved_object() {
        let saved = serde_json::json!({});
        let before = saved.clone();
        let fields = [
            field("url", Some("http://example.test")),
            field("apiKey", None),
        ];
        let resolved = resolve_settings_fields(&saved, &fields);
        assert_eq!(resolved[0].value, "http://example.test");
        assert_eq!(resolved[1].value, "");
        assert_eq!(saved, before);
    }

    #[test]
    fn unknown_settings_field_type_is_rejected() {
        let raw = r#"{"name":"url","label":"URL","type":"hostname"}"#;
        assert!(serde_json::from_str::<SettingsField>(raw).is_err());
    }

    #[test]
    fn sonarr_shaped_schema_parses_without_persisting_defaults() -> Result<(), serde_json::Error> {
        let schema: SettingsSchema = serde_json::from_str(
            r#"{
                "fields": [
                    {"name":"url","label":"URL","type":"url","default":"http://example.test","required":true},
                    {"name":"apiKey","label":"Clé","type":"secret","required":true},
                    {"name":"pathMappingLocal","label":"Local","type":"string","default":"/data"}
                ]
            }"#,
        )?;
        let saved = serde_json::json!({"apiKey": ""});
        let resolved = resolve_settings_fields(&saved, &schema.fields);
        assert_eq!(resolved[0].field_type, SettingsFieldType::Url);
        assert_eq!(resolved[0].value, "http://example.test");
        assert_eq!(resolved[1].value, "");
        assert_eq!(resolved[2].value, "/data");
        assert!(saved.get("url").is_none());
        assert!(saved.get("pathMappingLocal").is_none());
        Ok(())
    }
}
