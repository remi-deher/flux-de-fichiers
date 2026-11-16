//! Vocabulaire fermé du catalogue. Le dépôt de plugins tranche pour `type` et
//! `capability` ; la catégorie appartient à Flux.

use serde::Deserializer;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Nature d'une pièce du catalogue (`type` du manifeste et de l'index).
///
/// L'ensemble est celui de `schema/manifest.v1.1.schema.json`, qui couvre aussi
/// `index.json`. Une chaîne inconnue est refusée.
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

/// Section exposée à l'utilisateur. Les dépôts sont gérés à part (ils portent le catalogue,
/// ils n'en font pas partie) et les blocs n'ont aucune section : on ne les expose jamais.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CatalogSection {
    Applications,
    Plugins,
    Flows,
    /// Éléments externes appelables par un flux : image, musique, police, langue, etc.
    Other,
    Packs,
}

impl CatalogSection {
    pub const ALL: [Self; 5] = [
        Self::Applications,
        Self::Plugins,
        Self::Flows,
        Self::Other,
        Self::Packs,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Applications => "Applications",
            Self::Plugins => "Plugins",
            Self::Flows => "Flows",
            Self::Other => "Autre",
            Self::Packs => "Packs",
        }
    }
}

impl CatalogItemType {
    /// Section où la pièce s'affiche ; `None` pour un bloc, jamais exposé tel quel.
    pub fn section(self) -> Option<CatalogSection> {
        match self {
            Self::App => Some(CatalogSection::Applications),
            Self::Plugin => Some(CatalogSection::Plugins),
            Self::Flow => Some(CatalogSection::Flows),
            Self::Preset
            | Self::ResourceFont
            | Self::ResourceImage
            | Self::ResourceAudio
            | Self::ResourceColor
            | Self::ResourceLanguage
            | Self::ResourceTemplate => Some(CatalogSection::Other),
            Self::Pack => Some(CatalogSection::Packs),
            Self::Block => None,
        }
    }

    /// Sous-titre de la section « Autre » ; `None` hors de cette section.
    pub fn subtitle(self) -> Option<&'static str> {
        match self {
            Self::Preset => Some("Préréglages"),
            Self::ResourceFont => Some("Polices"),
            Self::ResourceImage => Some("Images"),
            Self::ResourceAudio => Some("Musiques et sons"),
            Self::ResourceColor => Some("Couleurs"),
            Self::ResourceLanguage => Some("Langues"),
            Self::ResourceTemplate => Some("Modèles"),
            _ => None,
        }
    }

    /// Une pièce peut-elle vivre seule dans un dépôt ? Un bloc appartient à un plugin.
    pub fn can_stand_alone(self) -> bool {
        self != Self::Block
    }

    /// Règle de dépendance : un plugin dépend d'applications, un flow de plugins ou
    /// d'éléments « Autre », un pack regroupe des pièces autonomes. Applications et
    /// « Autre » ne dépendent de rien ; un bloc ne dépend que de son plugin propriétaire.
    pub fn may_depend_on(self, target: Self) -> bool {
        let target_section = target.section();
        match self {
            Self::Plugin => target == Self::App,
            Self::Flow => target == Self::Plugin || target_section == Some(CatalogSection::Other),
            Self::Pack => target.can_stand_alone() && target != Self::Pack,
            _ => false,
        }
    }
}

/// Capacité déclarée par une pièce du catalogue (ouverte : toute chaîne non vide est acceptée).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct CatalogCapability(String);

impl CatalogCapability {
    pub fn new(value: impl Into<String>) -> Self {
        let value = value.into();
        let trimmed = value.trim();
        if trimmed.is_empty() {
            Self("other".to_owned())
        } else {
            Self(trimmed.to_owned())
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_probe(&self) -> bool {
        self.0 == "probe"
    }
}

impl Default for CatalogCapability {
    fn default() -> Self {
        Self("other".to_owned())
    }
}

impl TryFrom<String> for CatalogCapability {
    type Error = &'static str;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Err("la capacité ne peut pas être vide");
        }
        Ok(Self(trimmed.to_owned()))
    }
}

impl From<CatalogCapability> for String {
    fn from(cap: CatalogCapability) -> Self {
        cap.0
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

pub fn hides_category_badge(category: &Option<PluginCategory>) -> bool {
    match category {
        Some(value) => !value.shows_badge(),
        None => true,
    }
}

pub fn deserialize_optional_category<'de, D>(
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
pub struct DocumentLabels {
    pub category: Option<PluginCategory>,
    pub category_declared: bool,
    pub tags: Vec<String>,
    pub tags_declared: bool,
}

pub fn labels_from_document(value: &Value) -> DocumentLabels {
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

    const ALL_TYPES: [CatalogItemType; 12] = [
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

    #[test]
    fn only_blocks_have_no_section_and_cannot_stand_alone() {
        for kind in ALL_TYPES {
            let is_block = kind == CatalogItemType::Block;
            assert_eq!(kind.section().is_none(), is_block, "{kind:?}");
            assert_eq!(kind.can_stand_alone(), !is_block, "{kind:?}");
        }
    }

    #[test]
    fn other_section_has_a_subtitle_for_each_of_its_kinds() {
        for kind in ALL_TYPES {
            let in_other = kind.section() == Some(CatalogSection::Other);
            assert_eq!(kind.subtitle().is_some(), in_other, "{kind:?}");
        }
        for section in CatalogSection::ALL {
            assert!(!section.label().is_empty());
        }
    }

    #[test]
    fn dependency_rules_follow_the_catalog_model() {
        use CatalogItemType::{App, Block, Flow, Pack, Plugin, ResourceImage};
        assert!(Plugin.may_depend_on(App));
        assert!(!Plugin.may_depend_on(Flow));
        assert!(Flow.may_depend_on(Plugin));
        assert!(Flow.may_depend_on(ResourceImage));
        assert!(!Flow.may_depend_on(App));
        assert!(!App.may_depend_on(Plugin));
        assert!(!ResourceImage.may_depend_on(App));
        assert!(Pack.may_depend_on(App) && Pack.may_depend_on(Flow));
        assert!(!Pack.may_depend_on(Block) && !Pack.may_depend_on(Pack));
        assert!(!Block.may_depend_on(App));
    }

    #[test]
    fn catalog_capabilities_accept_any_non_empty_string() -> Result<(), serde_json::Error> {
        for raw in ["remux", "probe", "studio", "encode", "subtitles"] {
            let cap = CatalogCapability::new(raw);
            let json = serde_json::to_string(&cap)?;
            assert_eq!(json, format!("\"{raw}\""));
            assert_eq!(serde_json::from_str::<CatalogCapability>(&json)?, cap);
        }
        assert!(CatalogCapability::new("probe").is_probe());
        assert!(!CatalogCapability::new("transcode").is_probe());
        assert!(serde_json::from_str::<CatalogCapability>("\"   \"").is_err());
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
