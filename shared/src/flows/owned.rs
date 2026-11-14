//! Fiches de blocs sérialisables : catalogue natif + blocs lus dans les dépôts de plugins.

use super::{BlockSpec, FieldKind, FieldSpec, FlowBlockKind, block_catalog};
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

impl OwnedFieldKind {
    fn from_static(kind: FieldKind) -> Self {
        match kind {
            FieldKind::Text => Self::Text,
            FieldKind::Bool => Self::Bool,
            FieldKind::Headers => Self::Headers,
            FieldKind::Select(options) => Self::Select(
                options
                    .iter()
                    .map(|(value, label)| ((*value).to_owned(), (*label).to_owned()))
                    .collect(),
            ),
        }
    }
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
}

impl OwnedFieldSpec {
    pub fn from_static(field: &FieldSpec) -> Self {
        Self {
            name: field.name.to_owned(),
            label: field.label.to_owned(),
            kind: OwnedFieldKind::from_static(field.kind),
            required: field.required,
            help: field.help.to_owned(),
        }
    }
}

/// Fiche d'un bloc exécutable ou découvert dans un dépôt, version détenue.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OwnedBlockSpec {
    pub kind: String,
    pub label: String,
    pub category: String,
    pub description: String,
    pub has_input: bool,
    pub ports: Vec<(String, String)>,
    pub terminal: bool,
    pub fields: Vec<OwnedFieldSpec>,
    /// Bloc issu d'un dépôt de plugins (`type: block` dans le catalogue).
    #[serde(default)]
    pub from_plugin: bool,
    /// Opération déclarée par le manifeste plugin, si connue.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation_id: Option<String>,
}

impl OwnedBlockSpec {
    pub fn from_static(spec: &BlockSpec) -> Self {
        Self {
            kind: spec.kind.to_owned(),
            label: spec.label.to_owned(),
            category: spec.category.to_owned(),
            description: spec.description.to_owned(),
            has_input: spec.has_input,
            ports: spec
                .ports
                .iter()
                .map(|(name, label)| ((*name).to_owned(), (*label).to_owned()))
                .collect(),
            terminal: spec.terminal,
            fields: spec
                .fields
                .iter()
                .map(OwnedFieldSpec::from_static)
                .collect(),
            from_plugin: false,
            operation_id: None,
        }
    }

    /// Réglages initiaux : défauts sains pour chaque champ.
    pub fn default_config(&self) -> Value {
        let mut config = serde_json::Map::new();
        for field in &self.fields {
            let value = match &field.kind {
                OwnedFieldKind::Bool => json!(matches!(
                    field.name.as_str(),
                    "disable_track_stats" | "faststart"
                )),
                OwnedFieldKind::Headers => json!({}),
                OwnedFieldKind::Select(options) => {
                    json!(options.first().map_or("", |(value, _)| value.as_str()))
                }
                OwnedFieldKind::Text => match field.name.as_str() {
                    "audio_languages" | "subtitle_languages" => json!("all"),
                    _ => json!(""),
                },
            };
            config.insert(field.name.clone(), value);
        }
        Value::Object(config)
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

/// Catalogue natif, en fiches détenues (sans les blocs des dépôts).
pub fn native_owned_catalog() -> Vec<OwnedBlockSpec> {
    block_catalog()
        .iter()
        .map(OwnedBlockSpec::from_static)
        .collect()
}

/// Cherche une fiche dans une liste détenue.
pub fn find_owned<'a>(
    catalog: &'a [OwnedBlockSpec],
    kind: &FlowBlockKind,
) -> Option<&'a OwnedBlockSpec> {
    let key = kind.as_str();
    catalog.iter().find(|spec| spec.kind == key).or_else(|| {
        // Anciens identifiants courts encore présents dans des graphes enregistrés.
        catalog.iter().find(|spec| {
            FlowBlockKind::parse(&spec.kind) == *kind && !matches!(kind, FlowBlockKind::Other(_))
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_owned_catalog_roundtrips_kinds() {
        let owned = native_owned_catalog();
        assert!(!owned.is_empty());
        for spec in &owned {
            let kind = FlowBlockKind::parse(&spec.kind);
            assert!(!matches!(kind, FlowBlockKind::Other(_)), "{}", spec.kind);
            assert_eq!(
                find_owned(&owned, &kind).map(|found| found.kind.as_str()),
                Some(spec.kind.as_str())
            );
        }
    }

    #[test]
    fn default_config_fills_select_defaults() {
        let webhook = native_owned_catalog()
            .into_iter()
            .find(|spec| spec.kind == "webhook")
            .expect("webhook");
        let config = webhook.default_config();
        assert_eq!(config["method"], "POST");
        assert_eq!(webhook.missing_fields(&config), vec!["URL".to_owned()]);
    }
}
