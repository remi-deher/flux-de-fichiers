//! Vocabulaire fermé du catalogue. Le dépôt de plugins tranche pour `type` et
//! `capability` ; la catégorie appartient à Flux.

use serde::Deserializer;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Nature d'une pièce du catalogue (`type` du manifeste et de l'index).
///
/// L'ensemble est celui de `schema/manifest.v1.1.schema.json`, qui couvre aussi
/// `catalogue/index.json`. Une chaîne inconnue est refusée.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CatalogItemType {
    #[serde(rename = "plugin")]
    Plugin,
    #[serde(rename = "app")]
    App,
    #[serde(rename = "flow")]
    Flow,
    #[serde(rename = "preset")]
    Preset,
    #[serde(rename = "resource.font")]
    ResourceFont,
    #[serde(rename = "resource.image")]
    ResourceImage,
    #[serde(rename = "resource.audio")]
    ResourceAudio,
    #[serde(rename = "resource.color")]
    ResourceColor,
    #[serde(rename = "resource.language")]
    ResourceLanguage,
    #[serde(rename = "resource.template")]
    ResourceTemplate,
    #[serde(rename = "pack")]
    Pack,
    #[serde(rename = "block")]
    Block,
}

impl CatalogItemType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Plugin => "plugin",
            Self::App => "app",
            Self::Flow => "flow",
            Self::Preset => "preset",
            Self::ResourceFont => "resource.font",
            Self::ResourceImage => "resource.image",
            Self::ResourceAudio => "resource.audio",
            Self::ResourceColor => "resource.color",
            Self::ResourceLanguage => "resource.language",
            Self::ResourceTemplate => "resource.template",
            Self::Pack => "pack",
            Self::Block => "block",
        }
    }
}

/// Ce qu'une pièce sait faire.
///
/// Le schéma d'index laisse `capability` libre, et les manifestes d'outils
/// emploient déjà `probe` et `remux-mp4` en plus de l'enum étroit du schéma de
/// manifeste (`remux`, `database`, `transcode`, `analyze`, `other`). L'union
/// des valeurs réellement publiées par le dépôt est fermée : une chaîne
/// inconnue est refusée, un sous-ensemble trop court empêcherait de lire l'index.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CatalogCapability {
    #[serde(rename = "remux")]
    Remux,
    #[serde(rename = "database")]
    Database,
    #[serde(rename = "transcode")]
    Transcode,
    #[serde(rename = "analyze")]
    Analyze,
    #[serde(rename = "other")]
    Other,
    #[serde(rename = "video")]
    Video,
    #[serde(rename = "audio")]
    Audio,
    #[serde(rename = "integration")]
    Integration,
    #[serde(rename = "remux-mp4")]
    RemuxMp4,
    #[serde(rename = "probe")]
    Probe,
    #[serde(rename = "in-place")]
    InPlace,
    #[serde(rename = "faststart")]
    Faststart,
}

impl CatalogCapability {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Remux => "remux",
            Self::Database => "database",
            Self::Transcode => "transcode",
            Self::Analyze => "analyze",
            Self::Other => "other",
            Self::Video => "video",
            Self::Audio => "audio",
            Self::Integration => "integration",
            Self::RemuxMp4 => "remux-mp4",
            Self::Probe => "probe",
            Self::InPlace => "in-place",
            Self::Faststart => "faststart",
        }
    }
}

/// Catégorie fermée, propriété de Flux (schéma de bloc : `media`, `data`, `io`, `other`).
///
/// Distincte de `type` et de `capability`. Une valeur inconnue devient `other`
/// et ne produit pas de pastille. Les mots libres sont des `tags`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginCategory {
    Media,
    Data,
    Io,
    Other,
}

impl PluginCategory {
    pub fn from_label(raw: &str) -> Self {
        match raw {
            "media" => Self::Media,
            "data" => Self::Data,
            "io" => Self::Io,
            "other" => Self::Other,
            _ => Self::Other,
        }
    }

    pub fn shows_badge(self) -> bool {
        match self {
            Self::Media | Self::Data | Self::Io => true,
            Self::Other => false,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Media => "media",
            Self::Data => "data",
            Self::Io => "io",
            Self::Other => "other",
        }
    }
}

pub(crate) fn hides_category_badge(category: &Option<PluginCategory>) -> bool {
    match category {
        Some(value) => !value.shows_badge(),
        None => true,
    }
}

pub(crate) fn deserialize_optional_category<'de, D>(
    deserializer: D,
) -> Result<Option<PluginCategory>, D::Error>
where
    D: Deserializer<'de>,
{
    match Option::<String>::deserialize(deserializer)? {
        Some(raw) => Ok(Some(PluginCategory::from_label(&raw))),
        None => Ok(None),
    }
}

/// Catégorie et tags lus sur un document de catalogue, sans exiger un manifeste complet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DocumentLabels {
    pub category: Option<PluginCategory>,
    pub category_declared: bool,
    pub tags: Vec<String>,
    pub tags_declared: bool,
}

pub(crate) fn labels_from_document(value: &Value) -> DocumentLabels {
    let category_declared = matches!(value.get("category"), Some(Value::String(_)));
    let category = value
        .get("category")
        .and_then(Value::as_str)
        .map(PluginCategory::from_label);
    let (tags_declared, tags) = match value.get("tags") {
        Some(Value::Array(items)) => (
            true,
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect(),
        ),
        _ => (false, Vec::new()),
    };
    DocumentLabels {
        category,
        category_declared,
        tags,
        tags_declared,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_types_roundtrip_and_reject_unknown() -> Result<(), serde_json::Error> {
        let kinds = [
            CatalogItemType::Plugin,
            CatalogItemType::App,
            CatalogItemType::Flow,
            CatalogItemType::Preset,
            CatalogItemType::ResourceFont,
            CatalogItemType::ResourceImage,
            CatalogItemType::ResourceAudio,
            CatalogItemType::ResourceColor,
            CatalogItemType::ResourceLanguage,
            CatalogItemType::ResourceTemplate,
            CatalogItemType::Pack,
            CatalogItemType::Block,
        ];
        for kind in kinds {
            let json = serde_json::to_string(&kind)?;
            assert_eq!(json, format!("\"{}\"", kind.as_str()));
            assert_eq!(serde_json::from_str::<CatalogItemType>(&json)?, kind);
        }
        assert!(serde_json::from_str::<CatalogItemType>("\"integration\"").is_err());
        assert!(serde_json::from_str::<CatalogItemType>("\"extension\"").is_err());
        Ok(())
    }

    #[test]
    fn catalog_capabilities_cover_the_index_and_the_tool_schema() -> Result<(), serde_json::Error> {
        let capabilities = [
            CatalogCapability::Remux,
            CatalogCapability::Database,
            CatalogCapability::Transcode,
            CatalogCapability::Analyze,
            CatalogCapability::Other,
            CatalogCapability::Video,
            CatalogCapability::Audio,
            CatalogCapability::Integration,
            CatalogCapability::RemuxMp4,
            CatalogCapability::Probe,
            CatalogCapability::InPlace,
            CatalogCapability::Faststart,
        ];
        for capability in capabilities {
            let json = serde_json::to_string(&capability)?;
            assert_eq!(json, format!("\"{}\"", capability.as_str()));
            assert_eq!(
                serde_json::from_str::<CatalogCapability>(&json)?,
                capability
            );
        }
        assert!(serde_json::from_str::<CatalogCapability>("\"studio\"").is_err());
        assert!(serde_json::from_str::<CatalogCapability>("\"encode\"").is_err());
        Ok(())
    }

    #[test]
    fn unknown_category_becomes_other_without_a_badge() {
        assert_eq!(PluginCategory::from_label("media"), PluginCategory::Media);
        assert!(PluginCategory::Media.shows_badge());
        assert_eq!(PluginCategory::from_label("studio"), PluginCategory::Other);
        assert_eq!(PluginCategory::from_label("other"), PluginCategory::Other);
        assert!(!PluginCategory::Other.shows_badge());

        let labels = labels_from_document(&serde_json::json!({
            "category": "studio",
            "tags": ["arr", "tv"]
        }));
        assert!(labels.category_declared);
        assert_eq!(labels.category, Some(PluginCategory::Other));
        assert!(labels.tags_declared);
        assert_eq!(labels.tags, vec!["arr".to_owned(), "tv".to_owned()]);
        assert!(!labels.tags.iter().any(|tag| tag == "studio"));
    }

    #[test]
    fn tags_are_not_read_as_a_category() {
        let labels = labels_from_document(&serde_json::json!({
            "tags": ["media", "arr"]
        }));
        assert!(!labels.category_declared);
        assert_eq!(labels.category, None);
        assert_eq!(labels.tags, vec!["media".to_owned(), "arr".to_owned()]);
    }
}
