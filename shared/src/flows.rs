//! Contrat des flux : enregistrement, graphe et blocs exécutables (ADR 0012, 0014).
//!
//! Le moteur du central et l'éditeur du client lisent les mêmes types : un champ décrit ici est
//! celui que le moteur lit, et un graphe relu puis renvoyé ne perd aucune clé inconnue.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize, de::Deserializer, ser::Serializer};
use serde_json::{Map, Value};
use std::fmt;

mod catalog;
mod graph;
mod owned;

pub use catalog::{BlockSpec, FieldKind, FieldSpec, block_catalog, block_spec};
pub use graph::{GraphError, GraphPosition, layout};
pub use owned::{OwnedBlockSpec, OwnedFieldKind, OwnedFieldSpec, find_owned, native_owned_catalog};

/// Flux enregistré, tel que l'API le renvoie (définition JSON en texte, secrets masqués).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(sqlx::FromRow))]
pub struct FlowRecord {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub file_mask: String,
    pub is_default: bool,
    pub definition: String,
    /// Dernière version enregistrée ; les versions passées sont immuables.
    pub version: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Version figée d'un flux, telle que listée par `GET /api/v1/flows/{id}/versions`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(sqlx::FromRow))]
pub struct FlowVersionSummary {
    pub version: i64,
    pub name: String,
    pub created_at: DateTime<Utc>,
}

/// Corps de `PUT /api/v1/flows/{id}`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UpdateFlowRequest {
    pub name: String,
    pub description: Option<String>,
    pub file_mask: String,
    pub is_default: bool,
    pub definition: String,
}

impl UpdateFlowRequest {
    /// Enregistre `graph` en gardant les métadonnées du flux ouvert.
    pub fn keeping(flow: &FlowRecord, graph: &FlowGraph) -> Result<Self, serde_json::Error> {
        Ok(Self {
            name: flow.name.clone(),
            description: flow.description.clone(),
            file_mask: flow.file_mask.clone(),
            is_default: flow.is_default,
            definition: serde_json::to_string(graph)?,
        })
    }
}

/// Longueur maximale de la partie lisible d'un identifiant de flux.
pub const FLOW_ID_SLUG_MAX: usize = 40;

/// Identifiant d'un nouveau flux : `flow.<nom simplifié>-<suffixe>`.
pub fn new_flow_id(name: &str, suffix: &str) -> String {
    let mut slug = String::new();
    for ch in name.chars().flat_map(char::to_lowercase) {
        let ch = match ch {
            'à' | 'â' | 'ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'î' | 'ï' => 'i',
            'ô' | 'ö' => 'o',
            'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            other => other,
        };
        if ch.is_ascii_alphanumeric() {
            slug.push(ch);
        } else if !slug.ends_with('-') && !slug.is_empty() {
            slug.push('-');
        }
    }
    let slug: String = slug
        .trim_end_matches('-')
        .chars()
        .take(FLOW_ID_SLUG_MAX)
        .collect();
    let slug = slug.trim_end_matches('-');
    let readable = if slug.is_empty() { "nouveau" } else { slug };
    format!("flow.{readable}-{suffix}")
}

/// Type de bloc d'un graphe de flux. Les valeurs inconnues restent traversables.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum FlowBlockKind {
    FileInput,
    Condition,
    Mkvpropedit,
    RemuxMkv,
    RemuxMp4,
    VideoEncode,
    AudioEncode,
    MediaInspect,
    Webhook,
    SonarrRescan,
    SonarrRename,
    RadarrRescan,
    RadarrRename,
    FileOutput,
    Other(String),
}

impl FlowBlockKind {
    pub fn parse(value: &str) -> Self {
        match value {
            "file-input" => Self::FileInput,
            "condition" => Self::Condition,
            "mkvpropedit" => Self::Mkvpropedit,
            // Identifiants plugins (ADR 0006) et alias historiques de l'éditeur natif.
            "remux-mkv" | "block.video.remux-mkv" => Self::RemuxMkv,
            "remux-mp4" | "block.video.remux-mp4" => Self::RemuxMp4,
            "video-encode" | "block.video.encode" => Self::VideoEncode,
            "audio-encode" | "block.audio.encode" => Self::AudioEncode,
            "media-inspect" | "block.media.inspect" => Self::MediaInspect,
            "webhook" => Self::Webhook,
            "sonarr-rescan" => Self::SonarrRescan,
            "sonarr-rename" => Self::SonarrRename,
            "radarr-rescan" => Self::RadarrRescan,
            "radarr-rename" => Self::RadarrRename,
            "file-output" => Self::FileOutput,
            other => Self::Other(other.to_owned()),
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::FileInput => "file-input",
            Self::Condition => "condition",
            Self::Mkvpropedit => "mkvpropedit",
            Self::RemuxMkv => "block.video.remux-mkv",
            Self::RemuxMp4 => "block.video.remux-mp4",
            Self::VideoEncode => "block.video.encode",
            Self::AudioEncode => "block.audio.encode",
            Self::MediaInspect => "block.media.inspect",
            Self::Webhook => "webhook",
            Self::SonarrRescan => "sonarr-rescan",
            Self::SonarrRename => "sonarr-rename",
            Self::RadarrRescan => "radarr-rescan",
            Self::RadarrRename => "radarr-rename",
            Self::FileOutput => "file-output",
            Self::Other(value) => value,
        }
    }
}

impl fmt::Display for FlowBlockKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Serialize for FlowBlockKind {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for FlowBlockKind {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(Self::parse(&String::deserialize(deserializer)?))
    }
}

/// Bloc d'un graphe de flux.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FlowNode {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: FlowBlockKind,
    pub name: String,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub config: Value,
    /// Clés inconnues du bloc, conservées telles quelles.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// Liaison orientée entre deux blocs ; `port` choisit une sortie (condition : `true`, `false`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlowConnection {
    pub from: String,
    pub to: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<String>,
}

/// Graphe complet d'un flux.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct FlowGraph {
    #[serde(default)]
    pub nodes: Vec<FlowNode>,
    #[serde(default)]
    pub connections: Vec<FlowConnection>,
    /// Clés inconnues du graphe, conservées telles quelles.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl FlowGraph {
    /// Définition stockée → graphe ; une définition illisible donne un graphe vide.
    pub fn parse(definition: &str) -> Self {
        serde_json::from_str(definition).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_roundtrip_keeps_unknown_blocks() -> Result<(), serde_json::Error> {
        for (stored, canonical) in [
            ("file-input", "file-input"),
            ("condition", "condition"),
            ("mkvpropedit", "mkvpropedit"),
            ("remux-mkv", "block.video.remux-mkv"),
            ("block.video.remux-mkv", "block.video.remux-mkv"),
            ("video-encode", "block.video.encode"),
            ("block.video.encode", "block.video.encode"),
            ("audio-encode", "block.audio.encode"),
            ("block.audio.encode", "block.audio.encode"),
            ("block.media.inspect", "block.media.inspect"),
            ("block.video.remux-mp4", "block.video.remux-mp4"),
            ("webhook", "webhook"),
            ("sonarr-rescan", "sonarr-rescan"),
            ("sonarr-rename", "sonarr-rename"),
            ("radarr-rescan", "radarr-rescan"),
            ("radarr-rename", "radarr-rename"),
            ("file-output", "file-output"),
        ] {
            let kind = FlowBlockKind::parse(stored);
            assert_eq!(kind.as_str(), canonical);
            assert_eq!(
                serde_json::from_str::<FlowBlockKind>(&serde_json::to_string(&kind)?)?,
                kind
            );
        }
        assert!(matches!(
            FlowBlockKind::parse("macro.unknown"),
            FlowBlockKind::Other(_)
        ));
        Ok(())
    }

    #[test]
    fn a_stored_definition_survives_a_roundtrip() -> Result<(), serde_json::Error> {
        let stored = serde_json::json!({
            "format": 1,
            "nodes": [
                {"id": "n1", "type": "file-input", "name": "Entrée", "position": {"x": 1}},
                {"id": "n2", "type": "condition", "name": "HEVC ?",
                 "config": {"property": "video.codec", "operator": "eq", "target": "hevc"}}
            ],
            "connections": [{"from": "n1", "to": "n2"}]
        });
        let graph = FlowGraph::parse(&stored.to_string());
        assert_eq!(serde_json::to_value(&graph)?, stored);
        assert_eq!(FlowGraph::parse("pas du json"), FlowGraph::default());
        Ok(())
    }

    #[test]
    fn new_flow_ids_are_readable_and_suffixed() {
        assert_eq!(
            new_flow_id("Transcodage HEVC Éco !", "abc"),
            "flow.transcodage-hevc-eco-abc"
        );
        assert_eq!(new_flow_id("", "x"), "flow.nouveau-x");
        assert_eq!(new_flow_id("  ---  ", "x"), "flow.nouveau-x");
        let long = new_flow_id(&"a".repeat(80), "x");
        assert_eq!(long.len(), "flow.".len() + FLOW_ID_SLUG_MAX + "-x".len());
    }

    #[test]
    fn saving_keeps_the_flow_metadata() -> Result<(), serde_json::Error> {
        let flow = FlowRecord {
            id: "f".into(),
            name: "Films".into(),
            description: Some("note".into()),
            file_mask: "*.mp4".into(),
            is_default: true,
            definition: "{}".into(),
            version: 3,
            created_at: DateTime::<Utc>::UNIX_EPOCH,
            updated_at: DateTime::<Utc>::UNIX_EPOCH,
        };
        let request = UpdateFlowRequest::keeping(&flow, &FlowGraph::default())?;
        assert_eq!(request.description.as_deref(), Some("note"));
        assert_eq!(request.file_mask, "*.mp4");
        assert!(request.is_default);
        Ok(())
    }
}
