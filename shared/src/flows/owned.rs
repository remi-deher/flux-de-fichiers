//! Fiches de blocs sérialisables : chargées exclusivement depuis les dépôts de plugins.

use super::FlowBlockKind;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// Nature d'un champ, version détenue (API / plugins).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OwnedFieldKind {
    Text,
    Bool,
    /// Liste de choix : `(valeur, libellé)`.
    Select(Vec<(String, String)>),
    Headers,
}

/// Réglage d'un bloc, version détenue.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OwnedFieldSpec {
    pub name: String,
    pub label: String,
    pub kind: OwnedFieldKind,
    pub required: bool,
    #[serde(default)]
    pub help: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<Value>,
}

/// Fiche d'un bloc découvert dans un dépôt de plugins, version détenue.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OwnedBlockSpec {
    pub kind: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
    pub label: String,
    pub category: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    pub description: String,
    pub has_input: bool,
    pub ports: Vec<(String, String)>,
    pub terminal: bool,
    pub fields: Vec<OwnedFieldSpec>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub suggested_next: Vec<String>,
    /// Bloc issu d'un dépôt de plugins (`type: block` dans le catalogue).
    #[serde(default)]
    pub from_plugin: bool,
    /// Opération déclarée par le manifeste plugin, si connue.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation_id: Option<String>,
    /// Plugin propriétaire du bloc (`owner` dans `index.json`), si connu.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
}

impl OwnedBlockSpec {
    /// Réglages initiaux : défauts déclarés par le manifeste, sinon valeur neutre du type de champ.
    pub fn default_config(&self) -> Value {
        let mut config = serde_json::Map::new();
        for field in &self.fields {
            let value = field
                .default
                .as_ref()
                .map(|declared| normalize_default(&field.kind, declared))
                .unwrap_or_else(|| match &field.kind {
                    OwnedFieldKind::Bool => json!(false),
                    OwnedFieldKind::Headers => json!({}),
                    OwnedFieldKind::Select(options) => {
                        json!(options.first().map_or("", |(value, _)| value.as_str()))
                    }
                    OwnedFieldKind::Text => json!(""),
                });
            config.insert(field.name.clone(), value);
        }
        Value::Object(config)
    }

    /// Indique si cette fiche correspond à `kind` (identifiant canonique ou alias historique).
    pub fn matches_kind(&self, kind: &str) -> bool {
        self.kind == kind || self.aliases.iter().any(|alias| alias == kind)
    }

    /// Champs obligatoires restés vides.
    pub fn missing_fields(&self, config: &Value) -> Vec<String> {
        self.fields
            .iter()
            .filter(|field| field.required)
            .filter(|field| {
                config
                    .get(&field.name)
                    .and_then(Value::as_str)
                    .is_none_or(|text| text.trim().is_empty())
            })
            .map(|field| field.label.clone())
            .collect()
    }

    pub fn kind_enum(&self) -> FlowBlockKind {
        FlowBlockKind::parse(&self.kind)
    }
}

fn normalize_default(kind: &OwnedFieldKind, declared: &Value) -> Value {
    match (kind, declared) {
        (OwnedFieldKind::Select(_), Value::Number(number)) => Value::String(number.to_string()),
        (OwnedFieldKind::Select(_), Value::Bool(flag)) => Value::String(flag.to_string()),
        _ => declared.clone(),
    }
}

/// Cherche une fiche dans un catalogue par identifiant exact ou alias.
pub fn find_owned<'a>(
    catalog: &'a [OwnedBlockSpec],
    kind: &FlowBlockKind,
) -> Option<&'a OwnedBlockSpec> {
    let key = kind.as_str();
    catalog
        .iter()
        .find(|spec| spec.kind == key)
        .or_else(|| catalog.iter().find(|spec| spec.matches_kind(key)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_webhook() -> OwnedBlockSpec {
        OwnedBlockSpec {
            kind: "essentiel.webhook".into(),
            aliases: vec!["webhook".into()],
            label: "Webhook".into(),
            category: "Notification".into(),
            domain: Some("integration".into()),
            description: "Appelle une URL.".into(),
            has_input: true,
            ports: Vec::new(),
            terminal: false,
            fields: vec![
                OwnedFieldSpec {
                    name: "url".into(),
                    label: "URL".into(),
                    kind: OwnedFieldKind::Text,
                    required: true,
                    help: String::new(),
                    default: None,
                },
                OwnedFieldSpec {
                    name: "method".into(),
                    label: "Méthode".into(),
                    kind: OwnedFieldKind::Select(vec![
                        ("GET".into(), "GET".into()),
                        ("POST".into(), "POST".into()),
                    ]),
                    required: false,
                    help: String::new(),
                    default: Some(json!("POST")),
                },
                OwnedFieldSpec {
                    name: "fail_on_error".into(),
                    label: "Échec bloquant".into(),
                    kind: OwnedFieldKind::Bool,
                    required: false,
                    help: String::new(),
                    default: Some(json!(false)),
                },
            ],
            suggested_next: Vec::new(),
            from_plugin: true,
            operation_id: Some("operation.core.webhook".into()),
            owner: None,
        }
    }

    #[test]
    fn find_owned_resolves_canonical_and_alias() {
        let catalog = vec![sample_webhook()];
        assert_eq!(
            find_owned(&catalog, &FlowBlockKind::parse("essentiel.webhook"))
                .map(|s| s.kind.as_str()),
            Some("essentiel.webhook")
        );
        assert_eq!(
            find_owned(&catalog, &FlowBlockKind::parse("webhook")).map(|s| s.kind.as_str()),
            Some("essentiel.webhook")
        );
        assert!(find_owned(&catalog, &FlowBlockKind::parse("macro.unknown")).is_none());
    }

    #[test]
    fn default_config_uses_declared_defaults_and_reports_required_fields() {
        let webhook = sample_webhook();
        let config = webhook.default_config();
        assert_eq!(config["method"], "POST");
        assert_eq!(config["fail_on_error"], false);
        assert_eq!(webhook.missing_fields(&config), vec!["URL".to_owned()]);
    }
}
