//! Sous-catégories officielles des plugins (ADR 0023).
//!
//! Le vocabulaire appartient à Flux : un dépôt de plugins référence un identifiant, il ne
//! peut ni en créer ni fournir de libellé. Le registre est un fichier de données versionné,
//! embarqué à la compilation.

use super::vocab::PluginCategory;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};
use std::fmt;
use std::sync::OnceLock;

/// Longueur maximale d'un identifiant de sous-catégorie.
pub const SUBCATEGORY_ID_MAX_LEN: usize = 64;
/// Langue de repli quand un libellé n'existe pas dans la langue demandée.
pub const FALLBACK_LANGUAGE: &str = "en";
/// Langues obligatoires pour chaque entrée du registre.
pub const REQUIRED_LANGUAGES: [&str; 2] = ["fr", "en"];
/// Version de forme du fichier de registre reconnue par cette version de Flux.
pub const SUPPORTED_REGISTRY_VERSION: u32 = 1;

const EMBEDDED_REGISTRY: &str = include_str!("subcategories.v1.json");
const ID_SEPARATOR: char = '-';

/// Pourquoi un identifiant de sous-catégorie est refusé.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubcategoryIdError {
    Empty,
    TooLong,
    InvalidCharacter,
    EdgeSeparator,
}

impl fmt::Display for SubcategoryIdError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Empty => "identifiant vide",
            Self::TooLong => "identifiant trop long",
            Self::InvalidCharacter => "caractère interdit (minuscules, chiffres et tirets seuls)",
            Self::EdgeSeparator => "tiret en tête ou en fin d'identifiant",
        })
    }
}

impl std::error::Error for SubcategoryIdError {}

/// Identifiant stable d'une sous-catégorie : minuscules, chiffres et tirets, sans tiret en
/// tête ni en fin, 64 caractères au plus.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct SubcategoryId(String);

impl SubcategoryId {
    pub fn parse(raw: &str) -> Result<Self, SubcategoryIdError> {
        if raw.is_empty() {
            return Err(SubcategoryIdError::Empty);
        }
        if raw.len() > SUBCATEGORY_ID_MAX_LEN {
            return Err(SubcategoryIdError::TooLong);
        }
        let allowed = |character: char| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == ID_SEPARATOR
        };
        if !raw.chars().all(allowed) {
            return Err(SubcategoryIdError::InvalidCharacter);
        }
        if raw.starts_with(ID_SEPARATOR) || raw.ends_with(ID_SEPARATOR) {
            return Err(SubcategoryIdError::EdgeSeparator);
        }
        Ok(Self(raw.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SubcategoryId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for SubcategoryId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        Self::parse(&raw).map_err(serde::de::Error::custom)
    }
}

/// Sous-catégorie lue sur un document de dépôt tiers : une valeur absente, non textuelle ou
/// mal formée devient `None` plutôt que de rejeter toute la fiche.
pub fn deserialize_optional_subcategory<'de, D>(
    deserializer: D,
) -> Result<Option<SubcategoryId>, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(Option::<Value>::deserialize(deserializer)?
        .as_ref()
        .and_then(Value::as_str)
        .and_then(|raw| SubcategoryId::parse(raw).ok()))
}

/// Pourquoi une sous-catégorie déclarée par un dépôt n'est pas retenue (repli sur la catégorie).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubcategoryIssue {
    Malformed(SubcategoryIdError),
    Unknown,
    CategoryMismatch {
        expected: PluginCategory,
        declared: Option<PluginCategory>,
    },
}

impl fmt::Display for SubcategoryIssue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Malformed(error) => write!(formatter, "identifiant invalide : {error}"),
            Self::Unknown => formatter.write_str("absente du registre officiel"),
            Self::CategoryMismatch { expected, declared } => write!(
                formatter,
                "rattachée à la catégorie « {} », le plugin déclare « {} »",
                expected.as_str(),
                declared.map_or("aucune", PluginCategory::as_str)
            ),
        }
    }
}

/// Entrée du registre officiel.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubcategoryDef {
    pub id: SubcategoryId,
    pub category: PluginCategory,
    pub labels: BTreeMap<String, String>,
    pub order: u32,
}

impl SubcategoryDef {
    /// Libellé dans la langue demandée, sinon en anglais, sinon l'identifiant.
    pub fn label(&self, language: &str) -> &str {
        self.labels
            .get(language)
            .or_else(|| self.labels.get(FALLBACK_LANGUAGE))
            .map_or(self.id.as_str(), String::as_str)
    }
}

/// Registre officiel des sous-catégories.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubcategoryRegistry {
    pub version: u32,
    pub subcategories: Vec<SubcategoryDef>,
}

impl SubcategoryRegistry {
    /// Lit et valide un registre : version reconnue, identifiants uniques, libellés complets.
    pub fn parse(json: &str) -> Result<Self, String> {
        let registry: Self = serde_json::from_str(json).map_err(|error| error.to_string())?;
        if registry.version != SUPPORTED_REGISTRY_VERSION {
            return Err(format!(
                "version de registre {} non prise en charge",
                registry.version
            ));
        }
        let mut seen = HashSet::new();
        for def in &registry.subcategories {
            if !seen.insert(&def.id) {
                return Err(format!("identifiant en double : {}", def.id));
            }
            for language in REQUIRED_LANGUAGES {
                let filled = def
                    .labels
                    .get(language)
                    .is_some_and(|label| !label.trim().is_empty());
                if !filled {
                    return Err(format!("{} : libellé « {language} » manquant", def.id));
                }
            }
        }
        Ok(registry)
    }

    /// Registre embarqué. S'il était illisible (un test de CI l'empêche), il équivaut à un
    /// registre vide : tout plugin retombe sur sa catégorie.
    pub fn embedded() -> &'static Self {
        static REGISTRY: OnceLock<SubcategoryRegistry> = OnceLock::new();
        REGISTRY.get_or_init(|| Self::parse(EMBEDDED_REGISTRY).unwrap_or_default())
    }

    pub fn get(&self, id: &SubcategoryId) -> Option<&SubcategoryDef> {
        self.subcategories.iter().find(|def| &def.id == id)
    }

    /// Valide une valeur déclarée par un dépôt face à la catégorie du plugin.
    pub fn check(
        &self,
        raw: &str,
        category: Option<PluginCategory>,
    ) -> Result<&SubcategoryDef, SubcategoryIssue> {
        let id = SubcategoryId::parse(raw).map_err(SubcategoryIssue::Malformed)?;
        let def = self.get(&id).ok_or(SubcategoryIssue::Unknown)?;
        if category == Some(def.category) {
            Ok(def)
        } else {
            Err(SubcategoryIssue::CategoryMismatch {
                expected: def.category,
                declared: category,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID_REGISTRY: &str = r#"{"version":1,"subcategories":[
        {"id":"media-servers","category":"media","labels":{"fr":"Serveurs média","en":"Media servers"},"order":10}
    ]}"#;

    #[test]
    fn subcategory_id_accepts_only_lowercase_digits_and_inner_hyphens() {
        assert!(SubcategoryId::parse("media-servers").is_ok());
        assert!(SubcategoryId::parse("s3").is_ok());
        assert_eq!(SubcategoryId::parse(""), Err(SubcategoryIdError::Empty));
        assert_eq!(
            SubcategoryId::parse("Media-Servers"),
            Err(SubcategoryIdError::InvalidCharacter)
        );
        assert_eq!(
            SubcategoryId::parse("media servers"),
            Err(SubcategoryIdError::InvalidCharacter)
        );
        assert_eq!(
            SubcategoryId::parse("-media"),
            Err(SubcategoryIdError::EdgeSeparator)
        );
        assert_eq!(
            SubcategoryId::parse("media-"),
            Err(SubcategoryIdError::EdgeSeparator)
        );
        let too_long = "a".repeat(SUBCATEGORY_ID_MAX_LEN + 1);
        assert_eq!(
            SubcategoryId::parse(&too_long),
            Err(SubcategoryIdError::TooLong)
        );
        assert!(SubcategoryId::parse(&"a".repeat(SUBCATEGORY_ID_MAX_LEN)).is_ok());
    }

    #[test]
    fn subcategory_id_roundtrips_and_strict_deserialization_rejects_bad_values()
    -> Result<(), serde_json::Error> {
        let id: SubcategoryId = serde_json::from_str("\"media-servers\"")?;
        assert_eq!(serde_json::to_string(&id)?, "\"media-servers\"");
        assert_eq!(id.to_string(), "media-servers");
        assert!(serde_json::from_str::<SubcategoryId>("\"Media\"").is_err());
        Ok(())
    }

    #[test]
    fn lenient_deserializer_turns_bad_values_into_none() -> Result<(), serde_json::Error> {
        #[derive(Deserialize)]
        struct Holder {
            #[serde(default, deserialize_with = "deserialize_optional_subcategory")]
            value: Option<SubcategoryId>,
        }
        let parse = |raw: &str| serde_json::from_str::<Holder>(raw).map(|holder| holder.value);
        assert!(parse(r#"{"value":"media-servers"}"#)?.is_some());
        assert_eq!(parse(r#"{"value":"Media Servers"}"#)?, None);
        assert_eq!(parse(r#"{"value":42}"#)?, None);
        assert_eq!(parse(r#"{"value":null}"#)?, None);
        assert_eq!(parse("{}")?, None);
        Ok(())
    }

    #[test]
    fn embedded_registry_is_valid_and_complete() -> Result<(), String> {
        let registry = SubcategoryRegistry::parse(EMBEDDED_REGISTRY)?;
        assert_eq!(registry.version, SUPPORTED_REGISTRY_VERSION);
        assert!(!registry.subcategories.is_empty());
        assert_eq!(SubcategoryRegistry::embedded(), &registry);
        let servers = SubcategoryId::parse("media-servers").map_err(|error| error.to_string())?;
        let def = registry.get(&servers).ok_or("media-servers absent")?;
        assert_eq!(def.category, PluginCategory::Media);
        assert_eq!(def.label("fr"), "Serveurs média");
        Ok(())
    }

    #[test]
    fn label_falls_back_to_english_then_to_the_identifier() -> Result<(), String> {
        let registry = SubcategoryRegistry::parse(VALID_REGISTRY)?;
        let def = registry
            .subcategories
            .first()
            .ok_or("registre vide")?
            .clone();
        assert_eq!(def.label("en"), "Media servers");
        assert_eq!(def.label("de"), "Media servers");
        let mut bare = def;
        bare.labels.clear();
        assert_eq!(bare.label("fr"), "media-servers");
        Ok(())
    }

    #[test]
    fn check_accepts_only_known_ids_under_their_parent_category() -> Result<(), String> {
        let registry = SubcategoryRegistry::parse(VALID_REGISTRY)?;
        assert!(
            registry
                .check("media-servers", Some(PluginCategory::Media))
                .is_ok()
        );
        assert_eq!(
            registry.check("Media Servers", Some(PluginCategory::Media)),
            Err(SubcategoryIssue::Malformed(
                SubcategoryIdError::InvalidCharacter
            ))
        );
        assert_eq!(
            registry.check("studio", Some(PluginCategory::Media)),
            Err(SubcategoryIssue::Unknown)
        );
        assert_eq!(
            registry.check("media-servers", Some(PluginCategory::Io)),
            Err(SubcategoryIssue::CategoryMismatch {
                expected: PluginCategory::Media,
                declared: Some(PluginCategory::Io),
            })
        );
        assert_eq!(
            registry.check("media-servers", None),
            Err(SubcategoryIssue::CategoryMismatch {
                expected: PluginCategory::Media,
                declared: None,
            })
        );
        Ok(())
    }

    #[test]
    fn issues_explain_themselves_in_french() {
        assert_eq!(
            SubcategoryIssue::Unknown.to_string(),
            "absente du registre officiel"
        );
        let mismatch = SubcategoryIssue::CategoryMismatch {
            expected: PluginCategory::Media,
            declared: None,
        };
        assert!(mismatch.to_string().contains("« media »"));
        assert!(mismatch.to_string().contains("« aucune »"));
    }

    #[test]
    fn invalid_registries_are_rejected() {
        let wrong_version = VALID_REGISTRY.replace("\"version\":1", "\"version\":2");
        assert!(SubcategoryRegistry::parse(&wrong_version).is_err());

        let duplicate = r#"{"version":1,"subcategories":[
            {"id":"a","category":"media","labels":{"fr":"A","en":"A"},"order":1},
            {"id":"a","category":"media","labels":{"fr":"A","en":"A"},"order":2}
        ]}"#;
        assert!(SubcategoryRegistry::parse(duplicate).is_err());

        let missing_label = r#"{"version":1,"subcategories":[
            {"id":"a","category":"media","labels":{"fr":"A"},"order":1}
        ]}"#;
        assert!(SubcategoryRegistry::parse(missing_label).is_err());

        let blank_label = r#"{"version":1,"subcategories":[
            {"id":"a","category":"media","labels":{"fr":" ","en":"A"},"order":1}
        ]}"#;
        assert!(SubcategoryRegistry::parse(blank_label).is_err());

        let unknown_field = VALID_REGISTRY.replace("\"order\":10", "\"order\":10,\"icon\":\"x\"");
        assert!(SubcategoryRegistry::parse(&unknown_field).is_err());

        let bad_id = VALID_REGISTRY.replace("media-servers", "Media Servers");
        assert!(SubcategoryRegistry::parse(&bad_id).is_err());
        assert!(SubcategoryRegistry::parse("pas du json").is_err());
    }
}
